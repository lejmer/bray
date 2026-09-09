use std::collections::BTreeMap;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundUnit, BoundUnitRoot, CheckedExpressionSemantics};
use bray_checker::{ExecutionCallArgument, ExecutionCallInput, ExecutionGuaranteeInput};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableConditionSet, CallableConditions, CallableConditionsQuery, CallableContractClause,
    CallableSymbolId, SymbolQueryRequest, TypeData,
};

use crate::compilation::binder::binding_query_error;
use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn type_finalizer_completion(
        &self,
        ty: bray_symbols::TypeId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<bool>, FactQueryError> {
        let selected = self.execution_lifecycle_inputs(
            [(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)],
            cancellation,
        )?;

        let (mut lifecycle, mut diagnostics) = selected.into_parts();

        let Some(finalizer) =
            lifecycle.remove(&(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer))
        else {
            return Ok(DiagnosticResult::new(false, diagnostics));
        };

        let certified = self.instance_callable_proofs(finalizer.callable(), cancellation)?;

        diagnostics = diagnostics.merged(certified.diagnostics());

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(false, diagnostics));
        }

        let context = crate::compilation::checker::CompilationCheckerContext::new(
            self.binding_context(cancellation)?,
        );

        let receiver = self.semantic_value_store()?.intern_constant_term(
            bray_symbols::ConstantTermData::CallableArgument(bray_symbols::SymbolOrdinal::new(0)),
        )?;

        let complete = bray_checker::prove_finalizer_completion(
            &context,
            finalizer.conditions(),
            certified.value(),
            &[receiver],
            finalizer.result(),
            &[],
        )?;

        Ok(DiagnosticResult::new(complete.is_some(), diagnostics))
    }

    pub(super) fn execution_guarantee_input(
        &self,
        request: bray_checker::CheckerUnitView<
            '_,
            crate::compilation::checker::CompilationCheckerContext<'_>,
        >,
        expressions: &CheckedExpressionSemantics,
        storage: &bray_bound_tree::StoragePlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<ExecutionGuaranteeInput>>, FactQueryError> {
        let bound = request.unit();

        if !matches!(
            bound.root(),
            BoundUnitRoot::CallableBody { .. } | BoundUnitRoot::AnonymousCallable { .. }
        ) {
            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let declared = self.body_conditions(bound, cancellation)?;

        let (mut conditions, mut diagnostics) = declared.into_parts();

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        self.normalize_execution_conditions(&mut conditions, cancellation)?;

        let cleanup_calls = memory
            .operations()
            .iter()
            .filter_map(|operation| {
                operation
                    .kind()
                    .cleanup_element()
                    .map(|element| (operation.expression(), element))
            })
            .collect::<BTreeMap<_, _>>();

        let cleanup_roots = storage
            .identity_entries()
            .filter_map(|(identity, _)| storage.storage_type(identity))
            // Borrowed inputs expose their reached types without owning their cleanup.
            // Mutation-footprint checking still needs those types' represented dependencies.
            .chain(
                storage
                    .access_entries()
                    .map(|(_, access)| access.reached_type()),
            )
            .chain(cleanup_calls.values().copied());

        let cleanup_dependencies = crate::compilation::checker::checker_result(
            match bray_checker::owned_cleanup_type_dependencies(request, cleanup_roots) {
                Ok(result) => bray_checker::CheckerOutcome::Complete(result),
                Err(error) => error.into(),
            },
        )?;

        diagnostics = diagnostics.merged(cleanup_dependencies.diagnostics());

        let (cleanup_dependencies, _) = cleanup_dependencies.into_parts();

        let lifecycle = self.execution_lifecycle_inputs(
            cleanup_dependencies.keys().flat_map(|&ty| {
                [
                    bray_symbols::TypeAssociatedLifecycleSlot::Finalizer,
                    bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
                ]
                .map(|slot| (ty, slot))
            }),
            cancellation,
        )?;

        diagnostics = diagnostics.merged(lifecycle.diagnostics());

        let (lifecycle, _) = lifecycle.into_parts();

        if conditions.execution_guarantees().is_empty()
            && conditions.guarded_postconditions().is_empty()
            && conditions.normal_completion_postconditions().is_empty()
            && cleanup_calls.is_empty()
            && !lifecycle
                .keys()
                .any(|(_, slot)| *slot == bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)
        {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let values = self.semantic_value_store()?;
        let available = self.available_compiler_known_symbols();

        let projections = self.execution_storage_projection_inputs(
            bound.root().into(),
            storage,
            cleanup_dependencies.keys().copied(),
            cancellation,
        )?;

        diagnostics = diagnostics.merged(projections.diagnostics());

        let (projections, _) = projections.into_parts();

        let mut roots = bound
            .tree()
            .expressions()
            .filter_map(|(_, expression)| match expression {
                bray_bound_tree::BoundExpression::ControlTransfer(transfer)
                    if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return =>
                {
                    transfer.operand()
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();

        for (_, expression) in bound.tree().expressions() {
            match expression {
                bray_bound_tree::BoundExpression::Assignment(assignment) => {
                    roots.extend(assignment.operands().iter().copied())
                }
                bray_bound_tree::BoundExpression::Match(matched) => {
                    roots.insert(matched.subject());
                }
                bray_bound_tree::BoundExpression::Structured(structured)
                    if matches!(
                        structured.kind(),
                        bray_bound_tree::BoundStructuredExpressionKind::PatternTest
                            | bray_bound_tree::BoundStructuredExpressionKind::PatternBinding
                            | bray_bound_tree::BoundStructuredExpressionKind::ResultPropagation
                            | bray_bound_tree::BoundStructuredExpressionKind::NullablePropagation
                    ) =>
                {
                    roots.extend(structured.operands().iter().copied())
                }
                _ => {}
            }
        }

        for (_, block) in bound.tree().blocks() {
            for item in block.items() {
                if let bray_bound_tree::BoundBlockItem::LocalBinding(binding) = item {
                    roots.insert(binding.initializer());
                }
            }
        }

        let mut scalars = std::collections::BTreeSet::new();

        for entry in expressions.types().entries() {
            if let TypeData::Named {
                definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                ..
            } = &*values.type_data(entry.result().ty())?
                && available
                    .symbol_representation(*definition)
                    .and_then(crate::compilation::representation::target_scalar)
                    .is_some()
            {
                roots.insert(entry.expression());
                scalars.insert(entry.expression());
            }
        }

        let calls = self.execution_call_inputs(bound, expressions, &mut roots, cancellation)?;
        diagnostics = diagnostics.merged(calls.diagnostics());

        let (calls, _) = calls.into_parts();

        let roots = roots.into_iter().collect::<Vec<_>>();

        let (result_ordinal, terms) =
            self.symbolic_expression_terms(bound, expressions, &roots, cancellation)?;

        let local_observations = self.symbolic_body_bindings(bound, result_ordinal)?;

        let storage_observations =
            self.symbolic_storage_inputs(bound, storage, &local_observations, cancellation)?;

        let mut observations = BTreeMap::new();

        for (root, term) in roots.into_iter().zip(terms) {
            if let Some(term) = term
                && let Some(term) =
                    self.expanded_predicate_condition(term, cancellation, |instance| {
                        Ok(Some(instance))
                    })?
            {
                observations.insert(root, term);
            }
        }

        let propagations = bound.tree().expressions().filter_map(|(expression, _)| {
            matches!(
                expressions.selections().expression(expression),
                Some(bray_bound_tree::SemanticSelection::Propagation(_))
            )
            .then_some(expression)
        });

        Ok(DiagnosticResult::new(
            Some(
                ExecutionGuaranteeInput::new(
                    conditions,
                    result_ordinal,
                    observations,
                    calls,
                    local_observations,
                    propagations,
                )
                .with_scalar_observations(scalars)
                .with_implicit_execution(projections, memory)
                .with_lifecycle(
                    lifecycle,
                    cleanup_dependencies,
                    storage_observations,
                    cleanup_calls,
                )
                .with_resolved_storage(values, storage)?,
            ),
            diagnostics,
        ))
    }

    fn execution_storage_projection_inputs(
        &self,
        site: bray_bound_tree::AnyBoundNodeId,
        storage: &bray_bound_tree::StoragePlan,
        cleanup_types: impl IntoIterator<Item = bray_symbols::TypeId>,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<Option<Vec<bray_bound_tree::CallableProofDependency>>>,
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;
        let binding = self.binding_context(cancellation)?;

        let owners = cleanup_types
            .into_iter()
            .chain(storage.owned_borrows().map(|(owner, _, _)| owner))
            .collect::<std::collections::BTreeSet<_>>();

        let mut diagnostics = DiagnosticBag::new();
        let mut dependencies = Vec::new();

        for owner in owners {
            let data = values.type_data(owner)?;

            let TypeData::OwnedIndirection { storage, target } = data.as_ref() else {
                continue;
            };

            for name in ["StorageBorrow", "StorageBorrowMut"] {
                let Some(member) = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                else {
                    return Ok(DiagnosticResult::new(None, diagnostics));
                };

                let selected = crate::compilation::operation::selected_storage_callable(
                    self,
                    &binding,
                    *storage,
                    *target,
                    &member,
                    cancellation,
                )?;

                diagnostics = diagnostics.merged(selected.diagnostics());

                let (selected, _) = selected.into_parts();

                let Some((_, _, callable, _)) = selected.filter(|_| !diagnostics.has_errors())
                else {
                    return Ok(DiagnosticResult::new(None, diagnostics));
                };

                let declared = self.execution_callable_conditions(callable, cancellation)?;
                diagnostics = diagnostics.merged(declared.diagnostics());

                if diagnostics.has_errors() {
                    return Ok(DiagnosticResult::new(None, diagnostics));
                }

                let (conditions, _) = declared.into_parts();

                let Some(proof) = bray_checker::storage_projection_proof_dependencies(
                    values,
                    &conditions,
                    callable,
                    site,
                )?
                else {
                    return Ok(DiagnosticResult::new(None, diagnostics));
                };

                dependencies.extend(proof);
            }
        }

        Ok(DiagnosticResult::new(Some(dependencies), diagnostics))
    }

    fn execution_call_inputs(
        &self,
        bound: &BoundUnit,
        expressions: &CheckedExpressionSemantics,
        roots: &mut std::collections::BTreeSet<bray_bound_tree::BoundExpressionId>,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<BTreeMap<bray_bound_tree::BoundExpressionId, ExecutionCallInput>>,
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;
        let mut diagnostics = DiagnosticBag::new();
        let mut calls = BTreeMap::new();

        'calls: for (expression, _) in bound.tree().expressions() {
            let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                expressions.selections().expression(expression)
            else {
                continue;
            };

            // Even a call without usable contracts produces a distinct result observation.
            // Its empty contract grants neither execution properties nor postconditions.
            if matches!(
                call.resolution().result(),
                bray_bound_tree::BoundCallResult::Immediate(_)
            ) {
                calls.insert(
                    expression,
                    ExecutionCallInput::new(CallableConditionSet::new([]), [], []),
                );
            }

            let mut inputs = call
                .receiver()
                .map(|receiver| {
                    (
                        ExecutionCallArgument::Expression(receiver.expression()),
                        None,
                    )
                })
                .into_iter()
                .collect::<Vec<_>>();

            for argument in call.parameter_arguments() {
                match argument {
                    bray_bound_tree::SelectedArgument::Explicit {
                        expression,
                        conversion,
                        ..
                    } => inputs.push((
                        ExecutionCallArgument::Expression(*expression),
                        Some(conversion),
                    )),
                    bray_bound_tree::SelectedArgument::Default {
                        parameter,
                        provider,
                        ..
                    } => {
                        let Some(value) =
                            self.inert_parameter_default_term(*parameter, *provider, cancellation)?
                        else {
                            continue 'calls;
                        };

                        inputs.push((ExecutionCallArgument::Constant(value), None));
                    }
                }
            }

            if !matches!(
                call.resolution().result(),
                bray_bound_tree::BoundCallResult::Immediate(_)
            ) || inputs.iter().any(|(_, conversion)| {
                conversion.is_some_and(|conversion| {
                    !matches!(
                        conversion.target(),
                        bray_bound_tree::ConversionTarget::Identity
                    )
                })
            }) || (!matches!(
                call.target(),
                bray_bound_tree::BoundCallableTarget::Indirect(_)
            ) && call.arguments().iter().any(|argument| {
                matches!(
                    argument,
                    bray_bound_tree::SelectedArgument::Explicit {
                        parameter: None,
                        ..
                    }
                )
            })) {
                continue;
            }

            let declared = match call.target() {
                bray_bound_tree::BoundCallableTarget::Declaration(instance) => {
                    self.execution_callable_conditions(instance, cancellation)?
                }
                bray_bound_tree::BoundCallableTarget::Indirect(ty) => {
                    let data = values.type_data(ty)?;

                    let TypeData::Callable(callable) = data.as_ref() else {
                        continue;
                    };

                    if callable.is_variadic() {
                        continue;
                    }

                    // Normalization owns an Arc-backed contract copy, leaving the callable type immutable.
                    let mut conditions = callable.conditions().clone();
                    self.normalize_execution_conditions(&mut conditions, cancellation)?;

                    DiagnosticResult::without_diagnostics(conditions)
                }
                bray_bound_tree::BoundCallableTarget::Predicate(_)
                | bray_bound_tree::BoundCallableTarget::Anonymous(_) => continue,
            };

            diagnostics = diagnostics.merged(declared.diagnostics());

            if declared.diagnostics().has_errors() {
                continue;
            }

            let (conditions, _) = declared.into_parts();

            let mut borrowed = std::collections::BTreeSet::new();
            let mut arguments = Vec::with_capacity(inputs.len());

            for (argument, conversion) in &inputs {
                let ExecutionCallArgument::Expression(argument) = argument else {
                    arguments.push(*argument);
                    continue;
                };

                let ty = conversion
                    .map(|conversion| conversion.target_type())
                    .or_else(|| {
                        call.receiver()
                            .filter(|receiver| receiver.expression() == *argument)
                            .map(|receiver| receiver.target_type())
                    })
                    .or_else(|| {
                        expressions
                            .types()
                            .expression(*argument)
                            .map(|result| result.ty())
                    });

                let is_borrowed = match ty {
                    Some(ty) => matches!(&*values.type_data(ty)?, TypeData::Borrow { .. }),
                    None => false,
                } || call.receiver().is_some_and(|receiver| {
                    receiver.expression() == *argument
                        && matches!(
                            receiver.mode(),
                            bray_symbols::ReceiverMode::Shared
                                | bray_symbols::ReceiverMode::Mutable
                        )
                });

                let mut observed = *argument;

                if is_borrowed {
                    // Execution contracts observe the borrowed referent, not a materialized address.
                    // Keep this interpretation out of constant-value and exported-template evaluation.
                    if let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                        bound.tree().expression(*argument)
                        && expression.kind()
                            == bray_bound_tree::BoundStructuredExpressionKind::Borrow
                        && let [referent] = expression.operands()
                    {
                        observed = *referent;
                    }

                    borrowed.insert(observed);
                }

                roots.insert(observed);
                arguments.push(ExecutionCallArgument::Expression(observed));
            }

            calls.insert(
                expression,
                ExecutionCallInput::new(conditions, arguments, borrowed),
            );
        }

        Ok(DiagnosticResult::new(calls, diagnostics))
    }

    fn execution_lifecycle_inputs(
        &self,
        selections: impl IntoIterator<
            Item = (
                bray_symbols::TypeId,
                bray_symbols::TypeAssociatedLifecycleSlot,
            ),
        >,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<
            BTreeMap<
                (
                    bray_symbols::TypeId,
                    bray_symbols::TypeAssociatedLifecycleSlot,
                ),
                bray_checker::ExecutionLifecycleInput,
            >,
        >,
        FactQueryError,
    > {
        let binding = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;

        let mut diagnostics = DiagnosticBag::new();
        let mut lifecycle = BTreeMap::new();

        for (ty, slot) in selections {
            cancellation.check()?;

            let selected =
                crate::compilation::operation::selected_lifecycle_callable(&binding, ty, slot)?;

            diagnostics = diagnostics.merged(selected.diagnostics());

            let (Some((instance, signature)), _) = selected.into_parts() else {
                continue;
            };

            let declared = self.execution_callable_conditions(instance, cancellation)?;

            diagnostics = diagnostics.merged(declared.diagnostics());

            let (conditions, _) = declared.into_parts();

            let data = values.type_data(signature.callable_type())?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::Type(signature.callable_type()),
                    SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature),
                )
                .into());
            };

            lifecycle.insert(
                (ty, slot),
                bray_checker::ExecutionLifecycleInput::new(
                    instance,
                    conditions,
                    signature.result(),
                    callable.execution(),
                ),
            );
        }

        Ok(DiagnosticResult::new(lifecycle, diagnostics))
    }

    pub(in crate::compilation) fn execution_callable_conditions(
        &self,
        instance: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<CallableConditionSet>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;

        let declared = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableConditionsQuery>::new(
                instance.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        let mut conditions = self
            .semantic_value_store()?
            .substitute_callable_conditions(declared.value(), instance.substitution())?;

        self.normalize_execution_conditions(&mut conditions, cancellation)?;

        Ok(DiagnosticResult::new(
            conditions,
            declared.diagnostics().clone(),
        ))
    }

    fn normalize_execution_conditions(
        &self,
        conditions: &mut CallableConditionSet,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        conditions.try_map_clauses(|clause| {
            let Some(predicate) = clause.predicate() else {
                return Ok(clause);
            };

            let condition = match predicate.condition() {
                Some(condition) => {
                    self.expanded_predicate_condition(condition, cancellation, |instance| {
                        Ok(Some(instance))
                    })?
                }
                None => None,
            };

            Ok(CallableContractClause::new(
                clause.ordinal(),
                clause.kind(),
                predicate.with_condition(condition),
            )
            .with_guard(clause.guard()))
        })
    }

    fn body_conditions(
        &self,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<CallableConditionSet>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;

        let owner = binding
            .symbols()
            .symbol_for_key(bound.key().declared_owner())
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Unit(bound.key().clone()),
                    SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
                )
            })?;

        if matches!(bound.root(), BoundUnitRoot::AnonymousCallable { .. }) {
            let source = bound.key().source().syntax();

            let signature = binding
                .callable_contract_input_signature(owner, source)
                .map_err(binding_query_error)?;

            let contract = binding
                .callable_type_contract(owner, source, signature.value())
                .map_err(binding_query_error)?;

            let diagnostics =
                DiagnosticBag::merged_all([signature.diagnostics(), contract.diagnostics()]);

            // The proof input retains the callable's immutable conditions independently.
            return Ok(DiagnosticResult::new(
                contract.value().conditions().clone(),
                diagnostics,
            ));
        }

        let callable = CallableSymbolId::try_from_any(owner).ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Unit(bound.key().clone()),
                SemanticQueryViolation::Missing(SemanticDataKind::CallableSignature),
            )
        })?;

        let conditions = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableConditionsQuery>::new(callable))
            .map_err(binding_query_error)?;

        // The proof input normalizes its own Arc-backed copy of the declared conditions.
        Ok(DiagnosticResult::new(
            conditions.value().clone(),
            conditions.diagnostics().clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn execution_guarantees_compose_through_verified_calls() {
        let compilation = compilation(
            r#"
            module app;

            func leaf()
                executes(pure, total) {}

            func middle()
                executes(pure, total)
            {
                leaf();
            }

            func root()
                executes(pure, total)
            {
                middle();
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn generic_execution_proofs_follow_the_selected_witness() {
        for (properties, body, accepted) in [
            ("pure, total", "return true;", true),
            ("pure, total", "return read<Value>(&self);", false),
            ("pure", "return read<Value>(&self);", true),
            ("pure, total", "panic(1);", false),
        ] {
            let source = format!(
                r#"
                module app;
                trait Reader
                {{
                    func read() -> bool executes({properties});
                }}
                struct Value
                {{
                }}
                impl Value(Reader)
                {{
                    func read() -> bool executes({properties})
                    {{
                        {body}
                    }}
                }}
                func read<T>(pos value: &T) -> bool with(T: Reader) executes({properties})
                {{
                    return value.read();
                }}
                func root() -> bool executes({properties})
                {{
                    let value = Value
                    {{
                    }};
                    return read<Value>(&value);
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn generic_method_arguments_reach_selected_execution_proofs() {
        let source = r#"
        module app;

        trait Reader
        {
            func read<Value>(pos value: Value) -> Value
                executes(pure, total);
        }

        struct ReaderValue
        {
        }

        impl ReaderValue(Reader)
        {
            func read<Output>(pos value: Output) -> Output
                executes(pure, total)
            {
                return value;
            }
        }

        func apply<T>(pos reader: &T) -> bool
            with(T: Reader)
            executes(pure, total)
        {
            return reader.read<bool>(true);
        }

        func root() -> bool
            executes(pure, total)
        {
            let reader = ReaderValue
            {
            };

            return apply<ReaderValue>(&reader);
        }
        "#;

        let compilation = compilation(source);
        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn callable_and_trait_contracts_allow_weaker_execution_lane_requirements() {
        for (required, provided, accepted) in [
            ("requires(blocking_execution())", "", true),
            ("", "requires(blocking_execution())", false),
        ] {
            let source = format!(
                r#"
                module app;

                callable Operation = func() -> i32
                    {required};

                func operation() -> i32
                    {provided}
                {{
                    return 1;
                }}

                func select() -> Operation
                {{
                    return operation;
                }}

                trait Reader
                {{
                    func read() -> i32
                        {required};
                }}

                struct Value {{}}

                impl Value(Reader)
                {{
                    func read() -> i32
                        {provided}
                    {{
                        return 1;
                    }}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn callable_value_contracts_support_purity_without_inventing_totality() {
        for (provided, required, accepted) in [
            ("executes(pure)", "executes(pure)", true),
            ("executes(pure, total)", "executes(pure)", true),
            ("", "executes(pure)", false),
            ("executes(total)", "executes(pure)", false),
            ("executes(pure, total)", "executes(pure, total)", false),
        ] {
            let source = format!(
                r#"
                module app;
                func root(pos operation: func(pos value: bool) -> bool {provided}, pos value: bool) -> bool {required}
                {{
                    return operation(value);
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );

            if !accepted {
                assert!(diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenExecutionGuarantee));
            }
        }
    }

    #[test]
    fn callable_value_cleanup_is_pure_and_total_without_invoking_the_callable() {
        for callable in [
            "func() -> bool",
            "async func() -> bool",
            "trusted func() -> bool",
        ] {
            let source = format!(
                r#"
                module app;
                func root(pos operation: {callable}) executes(pure, total)
                {{
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn storage_projection_contracts_require_pure_total_checked_bodies() {
        for (contract, shared, mutable, accepted) in [
            (
                "executes(pure, total)",
                "return &storage.value;",
                "return &mut storage.value;",
                true,
            ),
            (
                "",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(total)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure, total) requires(storage.value == 0)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "when(true) { executes(pure, total) }",
                "return &storage.value;",
                "return &mut storage.value;",
                true,
            ),
            (
                "executes(pure, total)",
                "panic(1);",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure, total)",
                "return &storage.value;",
                "loop {}",
                false,
            ),
            (
                "executes(pure, total)",
                "return &storage.value;",
                "storage.value = 1; return &mut storage.value;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Policy
                {{
                    mut value: i32;
                }}
                impl Policy(Storage<i32>)
                {{
                    trusted static func create(pos value: i32) -> Self
                    {{
                        return Self
                        {{
                            value = value
                        }};
                    }}
                    static func borrow(pos storage: &Self) -> &i32 {contract}
                    {{
                        {shared}
                    }}
                    static func borrow_mut(pos storage: &mut Self) -> &mut i32 {contract}
                    {{
                        {mutable}
                    }}
                    trusted static func destroy(pos storage: &mut Self)
                    {{
                    }}
                    trusted static func release(pos storage: Self)
                    {{
                    }}
                }}
                func root(pos value: &box[Policy] i32) -> i32 executes(pure, total)
                {{
                    return match value
                    {{
                        case box(inner)
                        {{
                            yield inner;
                        }}
                    }};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn pure_projection_grants_mutation_authority_without_mutating() {
        for (contract, accepted) in [("executes(total)", true), ("executes(pure, total)", false)] {
            let source = format!(
                r#"
                module app;
                struct Item
                {{
                    mut value: i32;
                }}
                struct Outer
                {{
                    mut item: Item;
                }}
                func project(pos value: &mut Outer) -> &mut Item executes(pure, total)
                {{
                    return &mut value.item;
                }}
                func root(pos value: &mut Outer) {contract}
                {{
                    let projected = project(value);
                    projected.value = 1;
                }}
                "#,
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn checked_call_results_establish_caller_postconditions() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                executes(pure, total)
                ensures(result)
            {
                return true;
            }

            func middle() -> bool
                executes(pure, total)
                ensures(result)
            {
                return leaf();
            }

            func root() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return middle();
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn unchecked_callee_postconditions_cannot_certify_callers() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                executes(pure, total)
                ensures(result)
            {
                return false;
            }

            func root() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return leaf();
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics
                .iter()
                .filter(
                    |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
                )
                .count()
                >= 2,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn checked_call_results_refine_boolean_branches() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                executes(pure, total)
                ensures(result)
            {
                return true;
            }

            func root() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                if leaf()
                {
                    return true;
                }

                return false;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn ordinary_postconditions_can_supply_independent_body_evidence() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool)
                ensures(!ready)
            {
                ready = false;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let key = crate::test_support::source_callable_body_key(&compilation);

        let proof = compilation
            .callable_proofs_with_cancellation(key, &crate::fact::CancellationToken::new())
            .unwrap();

        assert_eq!(proof.result().value().len(), 1);

        assert!(matches!(
            proof.result().value().first(),
            Some(bray_bound_tree::CallableProofObligation::Postcondition(_))
        ));
    }

    #[test]
    fn scalar_argument_guards_use_evaluation_time_values() {
        let compilation = compilation(
            r#"
            module app;

            func leaf(pos ready: bool, pos other: bool) -> bool
                executes(pure)
                when(ready)
                {
                    ensures(result)
                }
            {
                return ready;
            }

            func root(pos mut ready: bool) -> bool
                when(!ready)
                {
                    ensures(result)
                }
            {
                return leaf(
                    ready,
                    {
                        ready = true;
                        yield true;
                    }
                );
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn guarded_call_proofs_use_parameter_order_and_preconditions() {
        for (arguments, accepted) in [
            ("other = false, ready = ready", true),
            ("other = ready, ready = false", false),
        ] {
            let source = format!(
                r#"
                module app;
                func leaf(ready: bool, other: bool) requires(ready) when(ready)
                {{
                    executes(pure, total)
                }}
                {{
                    if ready
                    {{
                        return;
                    }}
                    loop
                    {{
                    }}
                }}
                func root(pos ready: bool) when(ready)
                {{
                    executes(pure, total)
                }}
                {{
                    leaf({arguments});
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");
        }
    }

    #[test]
    fn execution_promises_do_not_certify_unverified_callee_bodies() {
        let compilation = compilation(
            r#"
            module app;

            func leaf(pos mut ready: bool)
                executes(pure)
            {
                ready = false;
            }

            func root()
                executes(pure)
            {
                leaf(true);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenExecutionGuarantee)
                .count()
                >= 2,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn recursive_purity_and_termination_have_different_proof_rules() {
        for (property, accepted) in [("pure", true), ("total", false)] {
            let source = format!(
                r#"
                module app;
                func first() executes({property})
                {{
                    second();
                }}
                func second() executes({property})
                {{
                    first();
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");
        }
    }

    #[test]
    fn dead_recursive_calls_do_not_create_termination_dependencies() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos ready: bool)
                when(ready)
                {
                    executes(pure, total)
                }
            {
                if ready
                {
                    return;
                }

                checked(false);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn pure_calls_preserve_entry_observations_for_later_control_flow() {
        let compilation = compilation(
            r#"
            module app;

            func leaf()
                executes(pure, total) {}

            func checked(pos ready: bool)
                when(ready)
                {
                    executes(pure, total)
                }
            {
                leaf();

                if ready
                {
                    return;
                }

                loop {}
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn mutation_invalidates_guarded_callee_evidence() {
        let compilation = compilation(
            r#"
            module app;

            func leaf(pos ready: bool)
                when(ready)
                {
                    executes(total)
                }
            {
                if ready
                {
                    return;
                }

                loop {}
            }

            func checked(pos mut ready: bool)
                when(ready)
                {
                    executes(total)
                }
            {
                ready = false;
                leaf(ready);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn execution_guarantees_are_checked_against_source_body_operations() {
        for body in ["value = false;", "unknown();"] {
            let source = format!(
                r#"
                module app;
                func unknown()
                {{
                }}
                func checked(pos mut value: bool) executes(pure)
                {{
                    {body}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn guarded_execution_checks_only_reachable_operations_in_its_entry_domain() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool)
                when(ready)
                {
                    executes(pure, total)
                }
            {
                if ready
                {
                    return;
                }

                ready = true;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn total_execution_rejects_reachable_unbounded_loops() {
        let compilation = compilation(
            r#"
            module app;

            func checked()
                executes(total)
            {
                loop {}
            }
        "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn execution_guarantees_include_owned_input_cleanup() {
        let compilation = compilation(
            r#"
            module app;

            func observe() {}

            struct Owner
            {
                destruct()
                {
                    observe();
                }
            }

            func checked(pos owner: Owner)
                executes(pure) {}
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn anonymous_execution_guarantees_use_their_own_input_scope() {
        let compilation = compilation(
            r#"
            module app;

            func outer(pos unrelated: bool)
            {
                let operation = lambda(pos mut ready: bool)
                    when(ready)
                    {
                        executes(pure, total)
                    }
                {
                    if ready
                    {
                        return;
                    }

                    ready = true;
                };
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn anonymous_mutation_requires_a_mutable_parameter_binding() {
        for (mode, allowed) in [("mut ", true), ("", false)] {
            let source = format!(
                r#"
                module app;
                func outer()
                {{
                    let operation = lambda(pos {mode}value: bool)
                    {{
                        value = true;
                    }};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingMissingMutationAuthority),
                !allowed,
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn total_execution_keeps_actual_assertion_and_cleanup_failures() {
        for source in [
            r#"
            module app;

            func checked()
                executes(total)
            {
                assert(false);
            }
            "#,
            r#"
            module app;

            struct Owner
            {
                destruct() {}
            }

            func checked(pos owner: Owner)
                executes(total) {}
            "#,
        ] {
            let compilation = compilation(source);
            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn total_execution_can_prove_assertions_from_entry_requirements() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos ready: bool)
                requires(ready)
                executes(total)
            {
                assert(ready);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn mutated_inputs_do_not_reuse_entry_observations() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool)
                when(ready)
                {
                    executes(total)
                }
            {
                ready = false;

                if ready
                {
                    return;
                }

                loop {}
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == DiagnosticKind::CheckingUnprovenExecutionGuarantee),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn guarded_postconditions_check_the_actual_boolean_return() {
        for (returned, accepted) in [("ready", true), ("false", false)] {
            let source = format!(
                r#"
                module app;
                func checked(pos ready: bool) -> bool when(ready)
                {{
                    executes(pure, total) ensures(result)
                }}
                {{
                    return {returned};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");

            assert_eq!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnprovenPostcondition),
                !accepted,
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn successful_completion_promises_distinguish_result_cases() {
        for (returned, accepted) in [("Ok(unit)", true), ("Error(unit)", false)] {
            let source = format!(
                r#"
                module app;
                func checked(pos ready: bool) -> Result<unit, unit> when(ready)
                {{
                    executes(pure, total) ensures(result matches Ok(_))
                }}
                {{
                    return {returned};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");

            assert_eq!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnprovenPostcondition),
                !accepted,
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn impossible_entry_domains_have_no_execution_or_completion_obligations() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool) -> bool
                when(false)
                {
                    executes(pure, total)
                    ensures(false)
                }
            {
                ready = true;
                loop {}
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn postconditions_observe_completion_state_and_preserve_entry_guard_activation() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool) -> bool
                when(ready)
                {
                    ensures(ready)
                }
            {
                ready = false;
                return true;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn assigned_scalar_postconditions_use_the_new_value() {
        for (condition, accepted) in [("!ready", true), ("ready", false)] {
            let source = format!(
                r#"
                module app;
                func checked(pos mut ready: bool) when(ready)
                {{
                    ensures({condition})
                }}
                {{
                    ready = false;
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");
        }
    }

    #[test]
    fn assigned_fields_establish_completion_predicates() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;
                predicate complete(value: &Self) = !value.pending;

                mut func close()
                    when(self.pending)
                    {
                        ensures(Self.complete(&self))
                    }
                {
                    self.pending = false;
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn assignments_preserve_only_disjoint_entry_observations() {
        for (assignment, accepted) in [
            ("other.ready = false", true),
            ("value.ready = false", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Flags
                {{
                    mut ready: bool;
                }}
                func root(pos mut value: Flags, mut other: Flags) requires(value.ready) when(true)
                {{
                    ensures(value.ready)
                }}
                {{
                    {assignment};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                !accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn assigned_checked_results_retain_scalar_snapshots() {
        for (assigned, accepted) in [("!ready()", true), ("ready()", false), ("!saved", true)] {
            let source = format!(
                r#"
                module app;
                func ready() -> bool executes(pure, total) ensures(result)
                {{
                    return true;
                }}
                func root(pos mut pending: bool) when(true)
                {{
                    ensures(!pending)
                }}
                {{
                    let saved = ready();
                    pending = {assigned};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                !accepted,
                "{source}: {diagnostics:?}"
            );

            if accepted {
                let key = crate::test_support::source_function_body_key(&compilation, "root");

                let proofs = compilation
                    .callable_proofs_with_cancellation(key, &compilation.state.cancellation)
                    .unwrap();

                assert!(
                    !proofs.result().diagnostics().has_errors(),
                    "{:?}",
                    proofs.result()
                );

                assert!(!proofs.result().value().is_empty());
            }
        }
    }

    #[test]
    fn pure_nullable_yields_preserve_the_declared_result_context() {
        for body in [
            "let code: i64? = if value != 0 { yield value; } else { yield none; }; return code;",
            "return if value != 0 { yield value; } else { yield none; };",
            "let code: i64? = match value { case 0 { yield none; } case _ { yield value; } }; return code;",
        ] {
            let source = format!(
                r#"
                module app;
                func root(pos value: i64) -> i64? executes(pure, total)
                {{
                    {body}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                lowered.value().is_some(),
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn matched_call_results_refine_delegated_postconditions() {
        for (failure_return, accepted) in [("Ok(unit)", false), ("Error(unit)", true)] {
            let source = format!(
                r#"
                module app;
                func status(pos ready: bool) -> Result<unit, unit> executes(pure, total) ensures(result matches Error(_) || ready)
                {{
                    if ready
                    {{
                        return Ok(unit);
                    }}
                    return Error(unit);
                }}
                func root(pos ready: bool) -> Result<unit, unit> executes(pure, total) ensures(result matches Error(_) || ready)
                {{
                    match status(ready)
                    {{
                        case Ok(_)
                        {{
                            return Ok(unit);
                        }}
                        case Error(_)
                        {{
                            return {failure_return};
                        }}
                    }}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                !accepted,
                "{source}: {diagnostics:?}"
            );

            if accepted {
                let key = crate::test_support::source_function_body_key(&compilation, "root");
                let lowered = compilation.lowered_unit(key).unwrap();

                assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());
            }
        }
    }

    #[test]
    fn structural_match_refinements_preserve_nested_failure_and_alternatives() {
        for (pattern, condition) in [
            ("Ok(true)", "value matches Ok(true)"),
            ("Ok(true) | Error(_)", "value matches Ok(true) | Error(_)"),
            ("Ok(_)", "value matches Ok(_)"),
        ] {
            for expected in [true, false] {
                let guarantee = if expected {
                    format!("!result || {condition}")
                } else {
                    format!("!result || !({condition})")
                };

                let source = format!(
                    r#"
                    module app;
                    func root(pos value: Result<bool, unit>) -> bool executes(pure, total) ensures({guarantee})
                    {{
                        match value
                        {{
                            case {pattern}
                            {{
                                return true;
                            }}
                            case _
                            {{
                                return false;
                            }}
                        }}
                    }}
                    "#
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert_eq!(
                    diagnostics.has_errors(),
                    !expected,
                    "{source}: {diagnostics:?}"
                );
            }
        }
    }

    #[test]
    fn trusted_wrappers_preserve_results_without_certifying_unproved_contracts() {
        for valid in [false, true] {
            let source = format!(
                r#"
                trusted module app;
                trusted func leaf() -> bool executes(pure, total) ensures(result)
                {{
                    return {valid};
                }}
                func root() -> bool when(true)
                {{
                    ensures(result)
                }}
                {{
                    return trusted leaf();
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");

            let proofs = compilation
                .callable_proofs_with_cancellation(key, &compilation.state.cancellation)
                .unwrap();

            assert_eq!(
                proofs.result().diagnostics().has_errors(),
                !valid,
                "{source}: {:?}",
                proofs.result()
            );

            assert_eq!(!proofs.result().value().is_empty(), valid);
        }
    }

    #[test]
    fn literal_defaults_preserve_proofs_without_hiding_default_effects() {
        for (default, argument, accepted) in [
            ("true", "", true),
            ("false", "", false),
            ("effect()", "", false),
            ("effect()", "value = true", true),
        ] {
            let source = format!(
                r#"
                module app;
                func effect() -> bool
                {{
                    panic("default effect");
                }}
                func leaf(value: bool = {default}) -> bool executes(pure, total) ensures(!value || result, value || !result)
                {{
                    return value;
                }}
                func root() -> bool executes(pure, total) ensures(result)
                {{
                    return leaf({argument});
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn target_sized_defaults_preserve_execution_guarantees() {
        for ty in ["usize", "isize", "u64", "i32"] {
            for (default, accepted) in [("0", true), ("effect()", false)] {
                let source = format!(
                    r#"
                    module app;
                    func effect() -> {ty}
                    {{
                        panic("default effect");
                    }}
                    func leaf(value: {ty} = {default}) -> {ty} executes(pure, total)
                    {{
                        return value;
                    }}
                    func root() -> Result<unit, {ty}> executes(pure, total) ensures(result matches Error(_))
                    {{
                        return Error(leaf());
                    }}
                    "#
                );

                let compilation = compilation(&source);
                let key = crate::test_support::source_function_body_key(&compilation, "root");
                let lowered = compilation.lowered_unit(key).unwrap();

                assert_eq!(
                    lowered.value().is_some(),
                    accepted,
                    "{source}: {:?}",
                    lowered.diagnostics()
                );
            }
        }
    }

    #[test]
    fn returned_union_construction_retains_checked_payload_results() {
        for (variant, accepted) in [("Error", true), ("Ok", false)] {
            let source = format!(
                r#"
                module app;
                struct Payload
                {{
                    value: bool;
                }}
                func payload() -> Payload executes(pure, total)
                {{
                    return
                    {{
                        value = true
                    }};
                }}
                func root() -> Result<Payload, Payload> executes(pure, total) ensures(result matches Error(_))
                {{
                    return {variant}(payload());
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn status_results_preserve_nested_owner_completion_proofs() {
        for preparation in [
            "let status = input_status;",
            "let status = unknown(input_status);",
        ] {
            for checked in ["checked", "forward_checked"] {
                for (assigned, accepted) in [("!closed(status)", true), ("true", false)] {
                    let source = format!(
                        r#"
                        module app;
                        @copy struct Status
                        {{
                            category: u32;
                            reserved: u32;
                        }}
                        struct Failure
                        {{
                            transferred: usize;
                        }}
                        struct State
                        {{
                            mut open: bool;
                        }}
                        struct Owner
                        {{
                            mut state: State;
                            predicate complete(value: &Self) = !value.state.open;
                        }}
                        func failure(transferred: usize = 0) -> Failure executes(pure, total)
                        {{
                            return
                            {{
                                transferred = transferred
                            }};
                        }}
                        func checked(pos status: Status) -> Result<unit, Failure> executes(pure, total) ensures((result matches Error(_)) || (status.category == 0 && status.reserved == 0))
                        {{
                            if status.reserved != 0
                            {{
                                return Error(failure());
                            }}
                            if status.category != 0
                            {{
                                return Error(failure());
                            }}
                            return Ok(unit);
                        }}
                        func forward_checked(pos status: Status) -> Result<unit, Failure> executes(pure, total) ensures((result matches Error(_)) || (status.category == 0 && status.reserved == 0))
                        {{
                            return checked(status);
                        }}
                        func closed(pos status: Status) -> bool executes(pure, total) ensures(status.category != 0 || result)
                        {{
                            return status.category == 0;
                        }}
                        func unknown(pos status: Status) -> Status
                        {{
                            return status;
                        }}
                        func close(pos value: &mut Owner, input_status: Status) -> Result<unit, Failure> requires(Owner.complete(&value) || blocking_execution()) when(Owner.complete(&value))
                        {{
                            executes(pure, total) ensures(result matches Ok(_))
                        }}
                        ensures((result matches Error(_)) || Owner.complete(&value))
                        {{
                            if !value.state.open
                            {{
                                return Ok(unit);
                            }}
                            {preparation} value.state.open = {assigned};
                            return {checked}(status);
                        }}
                        func root(pos value: &mut Owner, status: Status) -> Result<unit, Failure> requires(Owner.complete(&value) || blocking_execution()) when(Owner.complete(&value))
                        {{
                            executes(pure, total) ensures(result matches Ok(_))
                        }}
                        ensures((result matches Error(_)) || Owner.complete(&value))
                        {{
                            return close(&mut value, input_status = status);
                        }}
                        "#
                    );

                    let compilation = compilation(&source);
                    let mut proof_details = Vec::new();

                    if accepted {
                        for name in ["failure", "checked", "forward_checked", "closed", "close"] {
                            let key =
                                crate::test_support::source_function_body_key(&compilation, name);

                            let semantics = compilation
                                .body_semantics_with_cancellation(
                                    key.clone(),
                                    &compilation.state.cancellation,
                                )
                                .unwrap();

                            let proofs = compilation
                                .callable_proofs_with_cancellation(
                                    key,
                                    &compilation.state.cancellation,
                                )
                                .unwrap();

                            assert!(
                                !semantics.result().diagnostics().has_errors(),
                                "{name}: {:?}",
                                semantics.result().diagnostics()
                            );

                            assert!(
                                !proofs.result().diagnostics().has_errors(),
                                "{name}: {:?}",
                                proofs.result().diagnostics()
                            );

                            assert!(
                                !proofs.result().value().is_empty(),
                                "{name}: {:?}",
                                compilation
                                    .expression_semantics_with_cancellation(
                                        crate::test_support::source_function_body_key(
                                            &compilation,
                                            name
                                        ),
                                        &compilation.state.cancellation
                                    )
                                    .unwrap()
                                    .result()
                                    .diagnostics()
                            );

                            proof_details.push((
                                name,
                                format!(
                                    "candidates: {:?}, proven: {:?}",
                                    semantics.result().value().execution_proofs(),
                                    proofs.result().value()
                                ),
                            ));
                        }
                    }

                    let key = crate::test_support::source_function_body_key(&compilation, "root");
                    let lowered = compilation.lowered_unit(key).unwrap();

                    assert_eq!(
                        lowered.value().is_some(),
                        accepted,
                        "{source}: {:?}; {proof_details:#?}",
                        lowered.diagnostics()
                    );
                }
            }
        }
    }

    #[test]
    fn error_results_do_not_require_unrelated_invalidated_observations() {
        for (returned, accepted) in [("Error(unit)", true), ("Ok(unit)", false)] {
            let source = format!(
                r#"
                module app;
                struct Owner
                {{
                    mut ready: bool;
                }}
                func mutate(pos value: &mut Owner)
                {{
                    value.ready = false;
                }}
                func root(pos value: &mut Owner) -> Result<unit, unit> requires(value.ready) when(true)
                {{
                    ensures(result matches Error(_) || value.ready)
                }}
                {{
                    mutate(&mut value);
                    return {returned};
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn propagated_errors_preserve_the_result_case_and_success_conditions() {
        for (assigned, accepted) in [("false", true), ("true", false)] {
            let source = format!(
                r#"
                module app;
                struct Owner
                {{
                    mut pending: bool;
                }}
                func operation() -> Result<unit, unit>
                {{
                    return Error(unit);
                }}
                func root(pos value: &mut Owner) -> Result<unit, unit> when(true)
                {{
                    ensures(result matches Error(_) || !value.pending)
                }}
                {{
                    try operation();
                    value.pending = {assigned};
                    return Ok(unit);
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn propagated_errors_from_calls_without_contract_inputs_preserve_the_result_case() {
        for (assigned, accepted) in [("false", true), ("true", false)] {
            let source = format!(
                r#"
                module app;
                struct Owner
                {{
                    mut pending: bool;
                }}
                func default_flag() -> bool
                {{
                    return true;
                }}
                func operation(pos owner: &mut Owner, flag: bool = default_flag()) -> Result<unit, unit>
                {{
                    owner.pending = flag;
                    return Error(unit);
                }}
                func root(pos owner: &mut Owner) -> Result<unit, unit> when(true)
                {{
                    ensures(result matches Error(_) || !owner.pending)
                }}
                {{
                    try operation(&mut owner);
                    owner.pending = {assigned};
                    return Ok(unit);
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn resource_completion_survives_propagation_and_conditional_mutation() {
        for preparation in [
            "try operation();",
            "let handle: u64 = try acquire(); try operation();",
            "if flag { owner.pending = false; } try operation();",
        ] {
            for (assigned, accepted) in [("false", true), ("true", false)] {
                let source = format!(
                    r#"
                    module app;
                    @copy union Kind
                    {{
                        Other;
                    }}
                    @copy struct Failure
                    {{
                        kind: Kind;
                        code: i64?;
                    }}
                    struct Owner
                    {{
                        mut pending: bool;
                        predicate complete(value: &Self) = !value.pending;
                    }}
                    func operation() -> Result<unit, Failure>
                    {{
                        return Ok(unit);
                    }}
                    func acquire() -> Result<u64, Failure>
                    {{
                        return Ok(1);
                    }}
                    func root(pos owner: &mut Owner, flag: bool) -> Result<unit, Failure> when(true)
                    {{
                        ensures((result matches Error(_)) || Owner.complete(&owner))
                    }}
                    {{
                        if !owner.pending
                        {{
                            return Ok(unit);
                        }}
                        {preparation} owner.pending = {assigned};
                        return Ok(unit);
                    }}
                    "#
                );

                let compilation = compilation(&source);
                let key = crate::test_support::source_function_body_key(&compilation, "root");
                let lowered = compilation.lowered_unit(key).unwrap();

                assert_eq!(
                    lowered.value().is_some(),
                    accepted,
                    "{source}: {:?}",
                    lowered.diagnostics()
                );
            }
        }
    }

    #[test]
    fn propagation_retains_error_shape_across_cleanup_without_inventing_payload_facts() {
        for (condition, accepted) in [
            ("result matches Error(_)", true),
            ("result matches Error(true)", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Cleanup
                {{
                    value: bool;
                    destruct()
                    {{
                    }}
                }}
                struct Owner
                {{
                    mut pending: bool;
                }}
                func operation() -> Result<unit, bool>
                {{
                    return Error(false);
                }}
                func root(pos owner: &mut Owner, flag: bool) -> Result<unit, bool> when(true)
                {{
                    ensures({condition})
                }}
                {{
                    let cleanup: Cleanup =
                    {{
                        value = true
                    }};
                    if flag
                    {{
                        owner.pending = false;
                    }}
                    try operation();
                    return Error(true);
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn assigned_call_results_transfer_record_postconditions() {
        for (pending, accepted) in [("false", true), ("true", false)] {
            let source = format!(
                r#"
                module app;
                struct State
                {{
                    pending: bool;
                    handle: u64;
                }}
                struct Owner
                {{
                    mut state: State;
                    predicate complete(value: &Self) = !value.state.pending && value.state.handle == 0;
                }}
                func retired() -> State executes(pure, total) ensures(result.pending == {pending}, result.handle == 0)
                {{
                    return
                    {{
                        pending = {pending}, handle = 0
                    }};
                }}
                func effect()
                {{
                }}
                func root(pos owner: &mut Owner) when(true)
                {{
                    ensures(Owner.complete(&owner))
                }}
                {{
                    effect();
                    owner.state = retired();
                }}
                "#
            );

            let compilation = compilation(&source);
            let key = crate::test_support::source_function_body_key(&compilation, "root");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert_eq!(
                lowered.value().is_some(),
                accepted,
                "{source}: {:?}",
                lowered.diagnostics()
            );
        }
    }

    #[test]
    fn mutable_domain_calls_establish_caller_completion_state() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close()
                    ensures(!self.pending)
                {
                    self.pending = false;
                }

                mut func checked()
                    when(true)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.close();
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn local_bindings_retain_checked_call_results() {
        let compilation = compilation(
            r#"
            module app;

            func ready() -> bool
                executes(pure, total)
                ensures(result)
            {
                return true;
            }

            func checked() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                let value = ready();

                return value;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn local_owners_receive_domain_completion_state() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close()
                    ensures(!self.pending)
                {
                    self.pending = false;
                }
            }

            func checked() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                let mut buffer: Buffer =
                {
                    pending = true
                };

                buffer.close();
                return !buffer.pending;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn explicit_borrow_arguments_share_referent_contract_observations() {
        for (operation, contract, body) in [
            (
                r#"
                func close(pos value: &mut Buffer)
                    ensures(!value.pending)
                {
                    value.pending = false;
                }
                "#,
                "",
                "close(&mut value); return !value.pending;",
            ),
            (
                r#"
                func observe(pos value: &Buffer) -> bool
                    requires(!value.pending)
                    executes(pure, total)
                    ensures(result)
                {
                    return !value.pending;
                }
                "#,
                "requires(!value.pending)",
                "return observe(&value);",
            ),
        ] {
            let source = format!(
                r#"
                module app;
                struct Buffer
                {{
                    mut pending: bool;
                }}
                {operation} func root(pos mut value: Buffer) -> bool {contract} when(true)
                {{
                    ensures(result)
                }}
                {{
                    {body}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

            assert!(
                compilation
                    .lowered_unit(crate::test_support::source_function_body_key(
                        &compilation,
                        "root"
                    ))
                    .unwrap()
                    .value()
                    .is_some()
            );
        }
    }

    #[test]
    fn resource_completion_contracts_cover_borrowed_and_async_boundaries() {
        let cases = [
            (
                "explicit predicate reborrow",
                r#"
                struct Buffer
                {
                    mut pending: bool;
                    predicate complete(value: &Self) = !value.pending;
                }

                func close(pos value: &mut Buffer)
                    ensures(Buffer.complete(&value))
                {
                    value.pending = false;
                }
                "#,
            ),
            (
                "borrow reborrow in body",
                r#"
                struct Buffer
                {
                    mut pending: bool;
                }

                func close(pos value: &mut Buffer)
                    ensures(!value.pending)
                {
                    value.pending = false;
                }

                func root(pos value: &mut Buffer)
                    ensures(!value.pending)
                {
                    close(&mut value);
                    assert(!value.pending);
                }
                "#,
            ),
            (
                "async forwarding",
                r#"
                struct Buffer
                {
                    mut pending: bool;

                    mut func close() -> Result<unit, unit>
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        self.pending = false;
                        return Ok(unit);
                    }

                    mut async func close_async() -> Result<unit, unit>
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        return self.close();
                    }
                }
                "#,
            ),
            (
                "async resource forwarding",
                r#"
                struct Buffer
                {
                    mut pending: bool;

                    finalize() -> Result<unit, unit>
                        when(!self.pending)
                        {
                            executes(pure, total)
                            ensures(result matches Ok(_))
                        }
                    {
                        if !self.pending
                        {
                            return Ok(unit);
                        }

                        return Error(unit);
                    }

                    destruct() {}

                    mut func close() -> Result<unit, unit>
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        self.pending = false;
                        return Ok(unit);
                    }

                    mut async func close_async() -> Result<unit, unit>
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        return self.close();
                    }
                }
                "#,
            ),
            (
                "async blocking forwarding",
                r#"
                struct Buffer
                {
                    mut pending: bool;

                    mut func close() -> Result<unit, unit>
                        requires(blocking_execution())
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        self.pending = false;
                        return Ok(unit);
                    }

                    mut async func close_async() -> Result<unit, unit>
                        requires(blocking_execution())
                        ensures((result matches Error(_)) || !self.pending)
                    {
                        return self.close();
                    }
                }
                "#,
            ),
            (
                "comparison complement",
                r#"
                struct Status
                {
                    code: u32;
                }

                func closed(pos status: Status) -> bool
                    executes(pure, total)
                    ensures(status.code != 0 || result)
                {
                    return status.code == 0;
                }
                "#,
            ),
            (
                "completed transfer needs no blocking lane",
                r#"
                struct Buffer
                {
                    mut pending: bool;

                    finalize() -> Result<unit, unit>
                        requires(!self.pending || blocking_execution())
                        when(!self.pending)
                        {
                            executes(pure, total)
                            ensures(result matches Ok(_))
                        }
                    {
                        if !self.pending
                        {
                            return Ok(unit);
                        }

                        return Error(unit);
                    }

                    consume mut func transfer() -> Result<u64, unit>
                        ensures(!self.pending)
                    {
                        let result: Result<u64, unit> = Ok(1);

                        self.pending = false;
                        return result;
                    }
                }
                "#,
            ),
            (
                "returned owned error",
                r#"
                struct Failure
                {
                    code: u64;
                }

                struct Buffer
                {
                    mut pending: bool;

                    finalize() -> Result<unit, Failure>
                        when(!self.pending)
                        {
                            executes(pure, total)
                            ensures(result matches Ok(_))
                        }
                    {
                        if !self.pending
                        {
                            return Ok(unit);
                        }

                        return Error(
                            {
                                code = 1
                            }
                        );
                    }

                    consume mut func transfer() -> Result<u64, Failure>
                        ensures(!self.pending)
                    {
                        let result: Result<u64, Failure> = Ok(1);

                        self.pending = false;
                        return result;
                    }
                }
                "#,
            ),
            (
                "guarded borrowed call",
                r#"
                struct Buffer
                {
                    mut pending: bool;
                    predicate complete(value: &Self) = !value.pending;

                    finalize() -> Result<unit, unit>
                        when(Self.complete(&self))
                        {
                            executes(pure, total)
                            ensures(result matches Ok(_))
                        }
                    {
                        return close(&mut self);
                    }
                }

                func close(pos value: &mut Buffer) -> Result<unit, unit>
                    when(!value.pending)
                    {
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }
                {
                    if !value.pending
                    {
                        return Ok(unit);
                    }

                    value.pending = false;
                    return Ok(unit);
                }
                "#,
            ),
        ];

        let failures = cases
            .into_iter()
            .filter_map(|(name, body)| {
                let source = format!("module app; {body}");
                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                diagnostics
                    .has_errors()
                    .then(|| format!("{name}: {diagnostics:?}"))
            })
            .collect::<Vec<_>>();

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn resource_completion_contracts_compose_across_variants() {
        let mut failures = Vec::new();

        for asynchronous in [false, true] {
            for predicate in [false, true] {
                for owned_error in [false, true] {
                    for resource in [false, true] {
                        for trusted in [false, true] {
                            let execution = if asynchronous { "async" } else { "" };
                            let error = if owned_error { "Failure" } else { "unit" };

                            let completed = if predicate {
                                "Self.complete(&self)"
                            } else {
                                "!self.pending"
                            };

                            let lifecycle = if resource {
                                r#"
                                finalize() -> Result<unit, unit>
                                    when(!self.pending)
                                    {
                                        executes(pure, total)
                                        ensures(result matches Ok(_))
                                    }
                                {
                                    if !self.pending
                                    {
                                        return Ok(unit);
                                    }

                                    return Error(unit);
                                }

                                destruct() {}
                                "#
                            } else {
                                ""
                            };

                            let trust = if trusted { "trusted" } else { "" };

                            let source = format!(
                                r#"
                                {trust} module app;
                                struct Failure
                                {{
                                    code: u64;
                                }}
                                struct Buffer
                                {{
                                    mut pending: bool;
                                    predicate complete(value: &Self) = !value.pending;
                                    {lifecycle} mut func close() -> Result<unit, {error}> requires(blocking_execution()) ensures((result matches Error(_)) || {completed})
                                    {{
                                        self.pending = false;
                                        return Ok(unit);
                                    }}
                                    mut {execution} func forward() -> Result<unit, {error}> requires(blocking_execution()) ensures((result matches Error(_)) || {completed})
                                    {{
                                        return self.close();
                                    }}
                                }}
                                "#
                            );

                            let compilation = compilation(&source);
                            let diagnostics = compilation.check_diagnostics();

                            if diagnostics.has_errors() {
                                failures.push(format!("async={asynchronous} predicate={predicate} owned_error={owned_error} resource={resource} trusted={trusted}: {diagnostics:?}"));
                            }
                        }
                    }
                }
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn disjoint_field_assignments_preserve_completion_observations() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;
                mut dirty: bool;

                mut func close()
                    when(true)
                    {
                        ensures(!self.pending && !self.dirty)
                    }
                {
                    self.pending = false;
                    self.dirty = false;
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn overlapping_assignments_replace_completion_observations() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close()
                    when(true)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.pending = false;
                    self.pending = true;
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn mutable_call_guards_are_fixed_before_the_body_changes_state() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close()
                    when(self.pending)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.pending = false;
                }

                mut func checked()
                    when(self.pending)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.close();
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn tuple_projection_conditions_retain_exact_element_identity() {
        for (element, unproven) in [(0, false), (1, true)] {
            let source = format!(
                r#"
                module app;
                func root(pos value: (bool, bool)) -> bool requires(value.0) when(true)
                {{
                    ensures(result)
                }}
                {{
                    return value.{element};
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                unproven,
                "{element}: {diagnostics:?}"
            );

            assert_eq!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.kind()
                        == DiagnosticKind::CheckingUnprovenPostcondition),
                unproven,
                "{element}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn terminal_domain_errors_preserve_their_checked_completion_state() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close() -> Result<unit, unit>
                    ensures(!self.pending)
                {
                    self.pending = false;
                    return Error(unit);
                }

                mut func checked() -> Result<unit, unit>
                    when(true)
                    {
                        ensures(!self.pending)
                    }
                {
                    return self.close();
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn mutating_after_a_domain_call_invalidates_its_completion_state() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func close()
                    ensures(!self.pending)
                {
                    self.pending = false;
                }

                mut func checked()
                    when(true)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.close();
                    self.pending = true;
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn callee_owned_parameter_mutation_does_not_establish_caller_state() {
        let compilation = compilation(
            r#"
            module app;

            func close(pos mut ready: bool)
                ensures(!ready)
            {
                ready = false;
            }

            func checked(pos ready: bool)
                when(ready)
                {
                    ensures(!ready)
                }
            {
                close(ready);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn unknown_calls_invalidate_assigned_observations() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                mut pending: bool;

                mut func change()
                {
                    self.pending = true;
                }

                mut func close()
                    when(true)
                    {
                        ensures(!self.pending)
                    }
                {
                    self.pending = false;
                    self.change();
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(
                |diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUnprovenPostcondition
            ),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn assignment_joins_require_matching_completion_values() {
        for (other, accepted) in [("false", true), ("true", false)] {
            let source = format!(
                r#"
                module app;
                func checked(pos mut ready: bool, pos branch: bool) when(true)
                {{
                    ensures(!ready)
                }}
                {{
                    if branch
                    {{
                        ready = false;
                    }}
                    else
                    {{
                        ready = {other};
                    }}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.has_errors(), !accepted, "{diagnostics:?}");
        }
    }

    #[test]
    fn branches_and_return_values_observe_assignments() {
        let compilation = compilation(
            r#"
            module app;

            func checked(pos mut ready: bool) -> bool
                when(ready)
                {
                    ensures(!result)
                }
            {
                ready = false;

                if ready
                {
                    return true;
                }

                return ready;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn finalizer_completion_guarantees_use_the_declared_observation_predicate() {
        for declaration in ["Buffer", "Buffer<T>"] {
            let source = format!(
                r#"
                module app;
                struct {declaration}
                {{
                    mut pending: bool;
                    predicate complete(value: &Self) = !value.pending;
                    finalize() -> Result<unit, unit> when(Self.complete(&self))
                    {{
                        executes(pure, total) ensures(result matches Ok(_))
                    }}
                    {{
                        if !self.pending
                        {{
                            return Ok(unit);
                        }}
                        return Error(unit);
                    }}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{declaration}: {diagnostics:?}");
        }
    }

    #[test]
    fn ordinary_static_members_can_use_the_contextual_type_qualifier() {
        let compilation = compilation(
            r#"
            module app;

            struct Buffer
            {
                static func ready() -> bool
                {
                    return true;
                }

                static func checked() -> bool
                {
                    return Self.ready();
                }
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn contextual_type_qualifiers_do_not_produce_runtime_values() {
        for qualifier in ["Self", "Buffer"] {
            let source = format!(
                r#"
                module app;
                struct Buffer
                {{
                    static func checked()
                    {{
                        let value = {qualifier};
                    }}
                }}
                "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingTypeQualifierUsedAsValue),
                "{qualifier}: {diagnostics:?}"
            );
        }
    }
}
