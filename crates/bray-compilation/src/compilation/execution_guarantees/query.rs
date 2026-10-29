use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::BoundUnitKey;
use bray_checker::{
    ExecutionCertification, ExecutionDeclaration, ExecutionProperty, check_execution_candidate,
    declared_execution_properties,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::checker::checker_result;

use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn unit_requires_constant_evaluation(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        match key.kind() {
            bray_bound_tree::BoundUnitKind::CallableBody => {
                let definition = self
                    .symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .and_then(bray_symbols::CallableDefinitionId::try_new)
                    .expect("callable body retains its declaration identity");

                self.declares_constant_callable(definition, cancellation)
            }
            bray_bound_tree::BoundUnitKind::AnonymousCallable => {
                let syntax = self.execution_declaration_node(key.source().syntax())?;

                let lambda = syntax
                    .cast::<bray_syntax::LambdaExpressionSyntax>()
                    .expect("anonymous callable retains its lambda syntax");

                Ok(lambda.callable_modifiers().const_token().is_some())
            }
            bray_bound_tree::BoundUnitKind::RuntimeDefault => Ok(false),
            _ => Ok(true),
        }
    }

    pub(in crate::compilation) fn execution_unit_declaration(
        &self,
        key: &BoundUnitKey,
    ) -> Result<DiagnosticResult<ExecutionDeclaration>, FactQueryError> {
        match key.kind() {
            bray_bound_tree::BoundUnitKind::CallableBody
            | bray_bound_tree::BoundUnitKind::AnonymousCallable => {
                self.execution_declaration(key.source().syntax())
            }
            _ => Ok(DiagnosticResult::new(
                ExecutionDeclaration::default(),
                DiagnosticBag::new(),
            )),
        }
    }

    pub(in crate::compilation) fn execution_declaration(
        &self,
        anchor: SyntaxAnchor,
    ) -> Result<DiagnosticResult<ExecutionDeclaration>, FactQueryError> {
        Ok(declared_execution_properties(
            self.execution_declaration_node(anchor)?,
        ))
    }

    pub(in crate::compilation) fn execution_declaration_node(
        &self,
        anchor: SyntaxAnchor,
    ) -> Result<bray_syntax::SyntaxNodeView<'_>, FactQueryError> {
        self.syntax_tree()
            .find_node(
                anchor.source_id(),
                anchor.syntax_kind(),
                anchor.full_range(),
                anchor.is_recovered(),
            )
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Source(anchor.source_id()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                )
            })
            .map_err(Into::into)
    }

    /// Returns certified entry-domain properties and completion predicates after checking dependencies.
    pub fn execution_properties(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<ExecutionCertification>>, FactQueryError> {
        let published =
            self.certified_execution_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    pub(in crate::compilation) fn certified_execution_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<ExecutionCertification>>, FactQueryError> {
        // Fact keys and their immutable publications retain shared unit identities independently.
        self.unit_query(
            &self.state.certified_execution,
            CompilationFactKey::CertifiedExecution(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let result = self.compute_certified_execution(&key, cancellation)?;

                Ok((result, Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn execution_candidates_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<bray_checker::ExecutionCandidates>>, FactQueryError> {
        let predicate = !key.is_static_initializer()
            && (key.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault
                || self.unit_requires_constant_evaluation(&key, cancellation)?);

        self.unit_query(
            &self.state.execution_candidates,
            CompilationFactKey::ExecutionCandidates(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                Ok((
                    self.collect_execution_candidates(&key, predicate, cancellation)?,
                    Box::new([]),
                ))
            },
        )
    }

    fn collect_execution_candidates(
        &self,
        key: &BoundUnitKey,
        predicate: bool,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<bray_checker::ExecutionCandidates>, FactQueryError> {
        // Each queried publication owns this Arc-backed unit identity.
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let expressions = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let prepared = self.prepared_body_analysis_with_cancellation(key.clone(), cancellation)?;

        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let body = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
        let flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(key, cancellation)?;

        let semantic_context = semantic_unit_context(context.symbols(), bound.result().value());

        let trusted = self.trusted_contract_inputs(
            key,
            bound.result().value(),
            expressions.result().value(),
            cancellation,
        )?;

        let behavior = self.selected_body_behavior_with_cancellation(key.clone(), cancellation)?;

        let defaults = self.execution_default_inputs(behavior.result().value(), cancellation)?;

        let recursive = Default::default();

        let unit =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                .with_trusted_contracts(trusted.value())
                .with_result_dependencies(&defaults, &recursive)
                .with_unwind(body.result().value().unwind());

        let mut diagnostics = DiagnosticBag::merged_all([
            bound.result().diagnostics(),
            expressions.result().diagnostics(),
            prepared.result().diagnostics(),
            storage.result().diagnostics(),
            body.result().diagnostics(),
            memory.result().diagnostics(),
            flow.result().diagnostics(),
            trusted.diagnostics(),
        ]);

        let declaration = self.execution_unit_declaration(key)?;

        let requirements =
            self.execution_condition_inputs(key, declaration.value().requirements(), cancellation)?;

        diagnostics.add_range(requirements.diagnostics().iter().cloned());

        let mut requirements = requirements
            .into_parts()
            .0
            .into_iter()
            .map(|(condition, _)| condition)
            .collect::<Vec<_>>();

        if key.kind() == bray_bound_tree::BoundUnitKind::ContractClause {
            let entry = self.clause_entry_conditions(key, cancellation)?;

            diagnostics.add_range(entry.diagnostics().iter().cloned());
            requirements.extend(entry.into_parts().0);
        }

        let contracts = self.execution_call_contracts(
            bound.result().value(),
            expressions.result().value(),
            cancellation,
        )?;

        diagnostics.add_range(contracts.diagnostics().iter().cloned());

        let check =
            |property,
             assumptions: &[bray_checker::ExecutionCondition],
             postconditions: &[(bray_checker::ExecutionCondition, bray_source::SourceSpan)],
             diagnostics: &mut DiagnosticBag| {
                let candidate = checker_result(check_execution_candidate(
                    unit,
                    prepared.result().value(),
                    property,
                    predicate,
                    assumptions,
                    postconditions,
                    contracts.value(),
                    expressions.result().value(),
                    storage.result().value(),
                    body.result().value(),
                    memory.result().value(),
                ))?;

                diagnostics.add_range(candidate.diagnostics().iter().cloned());

                Ok::<_, FactQueryError>(Arc::new(candidate.into_parts().0))
            };

        let mut candidates = bray_checker::ExecutionCandidates::new();

        for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
            let candidate = check(
                Some(bray_checker::ExecutionObligation::Property(property, None)),
                &requirements,
                &[],
                &mut diagnostics,
            )?;

            candidates.insert(
                bray_checker::ExecutionObligation::Property(property, None),
                candidate,
            );
        }

        {
            let total = &candidates
                [&bray_checker::ExecutionObligation::Property(ExecutionProperty::Total, None)];

            let candidate = if total_covers_termination(
                predicate,
                total,
                trusted.value(),
                expressions.result().value().types(),
                storage.result().value(),
                self.semantic_value_store()?,
            ) {
                Arc::clone(total)
            } else {
                check(
                    Some(bray_checker::ExecutionObligation::Termination),
                    &requirements,
                    &[],
                    &mut diagnostics,
                )?
            };

            candidates.insert(bray_checker::ExecutionObligation::Termination, candidate);
        }

        for domain in declaration.value().domains() {
            let guards = self.execution_condition_inputs(key, &domain.guards, cancellation)?;

            let posts =
                self.predicate_condition_inputs(key, &domain.postconditions, cancellation)?;

            diagnostics.add_range(guards.diagnostics().iter().cloned());
            diagnostics.add_range(posts.diagnostics().iter().cloned());

            let trusted_producer = self.trusted_predicate_producer(key)?;

            // Trusted predicate guarantees are declared sources of authority. Execution
            // properties and ordinary postconditions still require their independent proofs.
            let posts = posts
                .into_parts()
                .0
                .into_iter()
                .map(|(condition, domain, span)| {
                    (
                        if domain == bray_symbols::PredicateDomain::Trusted && trusted_producer {
                            bray_checker::ExecutionCondition::Boolean(true)
                        } else {
                            condition
                        },
                        span,
                    )
                })
                .collect::<Vec<_>>();

            // Each domain owns its entry assumptions while sharing immutable condition operands.
            let mut assumptions = requirements.clone();

            assumptions.extend(
                guards
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition),
            );

            if !domain.guards.is_empty() {
                for property in &domain.properties {
                    let candidate = check(
                        Some(bray_checker::ExecutionObligation::Property(
                            property.property,
                            None,
                        )),
                        &assumptions,
                        &[],
                        &mut diagnostics,
                    )?;

                    candidates.insert(
                        bray_checker::ExecutionObligation::Property(
                            property.property,
                            Some(property.source.into()),
                        ),
                        candidate,
                    );
                }
            }

            for source in posts
                .iter()
                .map(|(_, source)| *source)
                .collect::<std::collections::BTreeSet<_>>()
            {
                // Each clause proof owns shared immutable condition terms from this normalized domain.
                let conditions = posts
                    .iter()
                    .filter(|(_, span)| *span == source)
                    .cloned()
                    .collect::<Vec<_>>();

                let candidate = check(None, &assumptions, &conditions, &mut diagnostics)?;

                candidates.insert(
                    bray_checker::ExecutionObligation::Postcondition(source.into()),
                    candidate,
                );
            }
        }

        Ok(DiagnosticResult::new(candidates, diagnostics))
    }
}

fn total_covers_termination(
    predicate: bool,
    total: &bray_checker::ExecutionCandidate,
    trusted: &bray_checker::TrustedContractInputs,
    types: &bray_bound_tree::CheckedExpressionTypes,
    storage: &bray_bound_tree::StoragePlan,
    values: &bray_symbols::SemanticValueStore,
) -> bool {
    let optional_step = types.callable_result_type().is_some_and(|ty| {
        matches!(
            values.type_data(ty).as_ref(),
            bray_symbols::TypeData::Nullable(_)
        )
    }) && storage.identity_entries().any(|(root, identity)| {
        matches!(identity, bray_bound_tree::StorageIdentity::Receiver(_))
            && storage.storage_type(root).is_some_and(|ty| {
                matches!(
                    values.type_data(ty).as_ref(),
                    bray_symbols::TypeData::Borrow {
                        kind: bray_symbols::BorrowKind::Mutable,
                        ..
                    }
                )
            })
    });

    // Runtime checks make admission and normal-completion evidence distinct.
    let same_entry = !trusted
        .calls
        .values()
        .any(|contract| contract.checks_preconditions);

    same_entry
        && (predicate || total.failure().is_none() && (!optional_step || total.finite_iteration()))
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};
    use bray_checker::ExecutionProperty::{Pure, Total};
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn certified_dependencies_preserve_selected_generic_arguments() {
        for required in ["bool", "u32"] {
            let compilation = compilation(&format!(
                r#"
            trusted module app;
            trusted predicate ready<T>();
            trusted func helper<T>() -> bool requires(trusted ready<T>()) executes(total) {{ return true; }}
            trusted func root() -> bool requires(trusted ready<{required}>()) executes(total) {{ return trusted helper<bool>(); }}
        "#
            ));

            if required != "bool" {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );

                continue;
            }

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );

            let proof = compilation
                .execution_properties(source_function_body_key(&compilation, "root"))
                .expect("fixture callable must resolve its execution properties");

            let values = compilation
                .semantic_value_store()
                .expect("fixture compilation must provide its semantic value store");

            assert!(
                proof
                    .value()
                    .dependencies
                    .iter()
                    .any(|(_, _, target, obligation)| {
                        let Some(bray_bound_tree::BoundCallableTarget::Declaration(instance)) =
                            target.callable()
                        else {
                            return false;
                        };
                        *obligation == bray_checker::ExecutionObligation::Property(Total, None)
                            && values
                                .generic_substitution_data(instance.substitution())
                                .bindings()
                                .len()
                                == 1
                    })
            );
        }
    }

    #[test]
    fn unconditional_execution_properties_check_source_bodies() {
        let compilation = compilation(
            r#"
                trusted module app;

                func identity(pos value: bool) -> bool
                    executes(pure, total)
                {
                    return value;
                }

                trusted func trusted_identity(pos value: bool) -> bool
                    executes(pure, total)
                {
                    return identity(value);
                }
            "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["identity", "trusted_identity"] {
            let proof = compilation
                .execution_properties(source_function_body_key(&compilation, name))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                &proof.value().properties,
                &std::collections::BTreeSet::from([Pure, Total])
            );
        }
    }

    #[test]
    fn declarations_do_not_certify_effects_or_recursive_termination() {
        for source in [
            r#"
                module app;

                func change(pos value: &mut bool)
                    executes(pure)
                {
                    value = false;
                }
            "#,
            r#"
                trusted module app;

                trusted func change(pos value: &mut bool)
                    executes(pure)
                {
                    value = false;
                }
            "#,
            r#"
                module app;

                func forever()
                    executes(total)
                {
                    loop {}
                }
            "#,
            r#"
                module app;

                func missing() {}

                func caller()
                    executes(total)
                {
                    missing();
                }
            "#,
        ] {
            let compilation = compilation(source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }

        for source in [
            r#"
                module app;

                func first()
                    executes(pure, total)
                {
                    second();
                }

                func second()
                    executes(pure, total)
                {
                    first();
                }
            "#,
            r#"
                module app;
                trait Readable { func read() -> bool executes(pure, total); }
                struct Value {}
                impl Value(Readable) {
                    func read() -> bool executes(pure, total) { return first(); }
                }
                func relay<T>(pos value: &T) -> bool
                    with(T: Readable)
                    executes(pure, total)
                {
                    return value.read();
                }
                func first() -> bool executes(pure, total) {
                    let value = Value {};
                    return relay<Value>(&value);
                }
            "#,
        ] {
            let compilation = compilation(source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingCircularExecutionGuarantee,
            );

            let proof = compilation
                .execution_properties(source_function_body_key(&compilation, "first"))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                &proof.value().properties,
                &std::collections::BTreeSet::from([Pure])
            );
        }
    }

    #[test]
    fn error_results_complete_normally() {
        let compilation = compilation(
            r#"
                module app;

                func failure() -> Result<bool, bool>
                    executes(pure, total)
                {
                    return Error(false);
                }

                func propagate() -> Result<bool, bool>
                    executes(pure, total)
                {
                    return Ok(try failure());
                }
            "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }
    #[test]
    fn owned_and_local_destructors_participate_in_certification() {
        let compilation = compilation(
            r#"
                module app;

                struct Value
                {
                    destruct()
                        executes(pure, total) {}
                }

                func dispose(pos value: Value)
                    executes(pure, total) {}

                func local()
                    executes(pure, total)
                {
                    let value = Value
                    {
                    };
                }
            "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn cleanup_effects_and_admission_prevent_certification() {
        for source in [
            r#"
                module app;

                struct Value
                {
                    destruct()
                    {
                        panic("cleanup");
                    }
                }

                func dispose(pos value: Value)
                    executes(total) {}
            "#,
            r#"
                module app;

                struct Value
                {
                    finalize() -> Result<unit, bool>
                        executes(total)
                    {
                        return Error(false);
                    }
                }

                func dispose(pos value: Value)
                    executes(total) {}
            "#,
            r#"
                module app;

                struct Value
                {
                    destruct()
                    {
                        panic("cleanup");
                    }
                }

                func make() -> Value
                    executes(pure, total)
                {
                    return Value
                    {
                    };
                }
            "#,
        ] {
            let compilation = compilation(source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }

    #[test]
    fn construction_admission_uses_local_total_actions() {
        for action in ["finalize", "destruct"] {
            let source = r#"
                module app;

                struct Value
                {
                    ACTION()
                        executes(total) {}
                }

                func make() -> Value
                    executes(pure, total)
                {
                    return Value {};
                }

                func make_array() -> [Value; 2]
                    executes(pure, total)
                {
                    return [each item in 0..2 { yield Value {}; }];
                }
            "#
            .replace("ACTION", action);

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn construction_transfers_child_admission() {
        let compilation = compilation(
            r#"
            module app;

            struct Child
            {
                destruct() { panic("later"); }
            }

            struct Parent { child: Child; }

            func wrap(pos child: Child) -> Parent
                executes(pure, total)
            {
                return Parent { child = child };
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn exceptional_cleanup_participates_in_pure_certification() {
        for operation in ["risky(value)", "1 / value", "Maker(value)"] {
            for (destructor, pure) in [
                (
                    r#"
                        destruct()
                        {
                            panic("cleanup");
                        }
                    "#,
                    false,
                ),
                (
                    r#"
                        destruct()
                            executes(pure, total) {}
                    "#,
                    true,
                ),
            ] {
                let source = r#"
                    module app;

                    struct Value
                    {
                        DESTRUCTOR
                    }

                    struct Maker
                    {
                        construct(pos value: i32) -> Self
                            executes(pure)
                        {
                            risky(value);

                            return Maker
                            {
                            };
                        }
                    }

                    func risky(pos value: i32) -> i32
                        executes(pure)
                    {
                        return 1 / value;
                    }

                    func relay(pos owner: Value, pos value: i32) -> Value
                        executes(pure)
                    {
                        OPERATION;
                        return owner;
                    }
                "#
                .replace("DESTRUCTOR", destructor)
                .replace("OPERATION", operation);

                let compilation = compilation(&source);

                let proof = compilation
                    .execution_properties(source_function_body_key(&compilation, "relay"))
                    .expect("fixture callable must resolve its execution properties");

                assert_eq!(
                    proof.value().properties.contains(&Pure),
                    pure,
                    "{source}: {proof:?}"
                );
            }
        }
    }

    #[test]
    fn returning_an_owner_transfers_its_later_cleanup() {
        let compilation = compilation(
            r#"
                module app;

                struct Value
                {
                    finalize()
                    {
                        panic("later");
                    }
                }

                func relay(pos value: Value) -> Value
                    executes(pure, total)
                {
                    return value;
                }
            "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn cleanup_cycles_cannot_certify_total_execution() {
        let compilation = compilation(
            r#"
                module app;

                struct Value
                {
                    destruct()
                        executes(total)
                    {
                        recurse();
                    }
                }

                func recurse()
                    executes(total)
                {
                    let value = Value
                    {
                    };
                }
            "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingCircularExecutionGuarantee,
        );
    }

    #[test]
    fn invalid_declarations_do_not_supply_execution_evidence() {
        for source in [
            r#"
                module app;

                func bad()
                    executes(total)
                {
                    missing();
                }
            "#,
            r#"
                module app;

                func bad()
                    executes(magic) {}
            "#,
            r#"
                module app;

                func bad()
                    executes(total)
                {
                    return ; ;
                }
            "#,
            r#"
                module app;

                func bad() -> bool
                    executes(total) {}
            "#,
        ] {
            let compilation = compilation(source);

            assert!(compilation.check_diagnostics().has_errors(), "{source}");

            if compilation.syntax_tree_result().diagnostics().is_empty() {
                let proof = compilation
                    .execution_properties(source_function_body_key(&compilation, "bad"))
                    .expect("fixture callable must resolve its execution properties");

                assert!(proof.value().properties.is_empty(), "{source}: {proof:?}");
            }
        }
    }

    #[test]
    fn opaque_foreign_assertions_retain_their_provenance() {
        for (requirement, argument, valid) in [
            ("", "0", true),
            ("requires(value > 0)", "1", true),
            ("requires(value > 0)", "0", false),
        ] {
            let options = crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                bray_symbols::ProductKind::Library,
                crate::SelectedTarget::baseline(),
            )
            .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new("c")
                    .expect("fixture non empty shared str must be valid"),
                bray_symbols::NativeLinkKind::Dynamic,
            )]);

            let source = r#"
                trusted module app;

                @link(name = "c")
                @symbol(name = "native_value")
                @abi(c)
                extern trusted func native_value(pos value: i32) -> i32
                    REQUIREMENT
                    uses(foreign_call)
                    executes(total);

                trusted func wrapper() -> i32
                    uses(foreign_call)
                    executes(total)
                {
                    return native_value(ARGUMENT);
                }
            "#
            .replace("REQUIREMENT", requirement)
            .replace("ARGUMENT", argument);

            let compilation = crate::test_support::compilation_with_options(&source, options);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{:?}",
                compilation.check_diagnostics()
            );

            if !valid {
                continue;
            }

            let evidence = compilation
                .execution_properties(source_function_body_key(&compilation, "wrapper"))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                evidence.value().properties,
                std::collections::BTreeSet::from([Total])
            );

            assert_eq!(evidence.value().foreign_assertions.len(), 1);
        }
    }

    #[test]
    fn unknown_execution_property_names_have_source_diagnostics() {
        let compilation = compilation(
            r#"
            module app;

            func bad()
                executes(magic) {}
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingUnknownExecutionProperty,
        );
    }

    #[test]
    fn certifications_are_immutable_and_invalidated_with_their_source() {
        let original = compilation(
            r#"
                module app;

                func identity(pos value: bool) -> bool
                    executes(total)
                {
                    return value;
                }
            "#,
        );

        let key = source_function_body_key(&original, "identity");

        let first = original
            .execution_properties(key.clone())
            .expect("fixture callable must resolve its execution properties");

        std::thread::scope(|scope| {
            let requests = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        original
                            .execution_properties(key.clone())
                            .expect("fixture callable must resolve its execution properties")
                    })
                })
                .collect::<Vec<_>>();

            for request in requests {
                assert!(std::sync::Arc::ptr_eq(
                    &first,
                    &request
                        .join()
                        .expect("fixture query worker must join successfully")
                ));
            }
        });

        let updated = original
            .updated_sources(vec![crate::test_support::source_input(
                r#"
                    module app;

                    func identity(pos value: bool) -> bool
                        executes(total)
                    {
                        loop {}
                    }
                "#,
                1,
            )])
            .expect("fixture `updated_sources` call must succeed");

        let proof = updated
            .execution_properties(source_function_body_key(&updated, "identity"))
            .expect("fixture callable must resolve its execution properties");

        assert!(proof.value().properties.is_empty());

        assert_eq!(
            first.value().properties,
            std::collections::BTreeSet::from([Total])
        );
    }
    #[test]
    fn callee_requirements_are_not_unconditional_caller_evidence() {
        let compilation = compilation(
            r#"
                module app;

                func guarded(pos value: bool)
                    requires(value)
                    executes(total) {}

                func caller(pos value: bool)
                    executes(total)
                {
                    guarded(value);
                }
            "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );

        let proof = compilation
            .execution_properties(source_function_body_key(&compilation, "guarded"))
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            proof.value().properties,
            std::collections::BTreeSet::from([Total])
        );
    }
    #[test]
    fn nullable_match_execution_proofs_respect_payload_conditions() {
        for (parameter, requirement, arms, valid) in [
            (
                "value: bool",
                "value",
                "case true { return; } case false { panic(\"false\"); }",
                true,
            ),
            (
                "value: u32?",
                "value matches ?_",
                "case ?held { return; } case none { panic(\"absent\"); }",
                true,
            ),
            (
                "value: &(u32?)",
                "value matches ?_",
                "case ?held { return; } case none { panic(\"absent\"); }",
                true,
            ),
            (
                "value: u32?",
                "value matches none",
                "case ?held { panic(\"present\"); } case none { return; }",
                true,
            ),
            (
                "value: u32?",
                "value matches ?_",
                "case ?1 { return; } case _ { panic(\"other payload\"); }",
                false,
            ),
            (
                "value: u32?",
                "true",
                "case ?held { return; } case none { panic(\"absent\"); }",
                false,
            ),
        ] {
            let source = format!(
                "module app; func root(pos {parameter}) requires({requirement}) executes(pure, total) {{ match value {{ {arms} }} }}"
            );

            let application = compilation(&source);

            let evidence = application
                .execution_properties(source_function_body_key(&application, "root"))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                !evidence.diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                evidence.diagnostics()
            );
        }
    }

    #[test]
    fn local_callable_targets_keep_checked_termination_dependencies() {
        for (setup, valid) in [
            (
                "let action = lambda() executes(pure, total) {}; action();",
                true,
            ),
            (
                "let action = lambda() executes(pure, total) {}; let forwarded = action; forwarded();",
                true,
            ),
            ("let action = lambda() { while true {} }; action();", false),
            (
                "let action = lambda() executes(total) { root(flag); }; action();",
                false,
            ),
            (
                "let mut action: func() -> unit = lambda() executes(total) {}; action = lambda() { while true {} }; action();",
                false,
            ),
            (
                "let mut action: func() -> unit = lambda() executes(total) {}; if flag { action = lambda() { while true {} }; } action();",
                false,
            ),
        ] {
            let source =
                format!("module app; func root(pos flag: bool) executes(total) {{ {setup} }}");

            let application = compilation(&source);

            let evidence = application
                .execution_properties(source_function_body_key(&application, "root"))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                !evidence.diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                evidence.diagnostics()
            );
        }
    }

    #[test]
    fn methods_constructors_and_lambda_bodies_establish_their_own_evidence() {
        let compilation = compilation(
            r#"
                module app;

                struct Value
                {
                    construct() -> Self
                        executes(pure, total)
                    {
                        return Value
                        {
                        };
                    }

                    func value() -> bool
                        executes(pure, total)
                    {
                        return true;
                    }

                    static func helper()
                        executes(pure, total) {}
                }

                func make() -> Value
                    executes(pure, total)
                {
                    return Value();
                }

                func outer()
                {
                    let action = lambda()
                        executes(pure, total) {};
                }
            "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn replacing_local_owners_requires_the_selected_destructor() {
        for (promise, valid) in [("executes(total)", true), ("", false)] {
            let source = r#"
                module app;

                struct Value
                {
                    destruct()
                        PROMISE {}
                }

                func replace()
                    executes(total)
                {
                    let mut value = Value
                    {
                    };

                    value = Value
                    {
                    };
                }
            "#
            .replace("PROMISE", promise);

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
    #[test]
    fn deferred_trait_dispatch_cannot_certify_from_a_default_body() {
        let compilation = compilation(
            r#"
                module app;

                trait Value
                {
                    func get() -> bool
                    {
                        return true;
                    }

                    func read() -> bool
                        executes(total)
                    {
                        return self.get();
                    }
                }
            "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );

        let proof = compilation
            .execution_properties(crate::test_support::source_trait_callable_member_body_key(
                &compilation,
                "read",
            ))
            .expect("fixture callable must resolve its execution properties");

        assert!(proof.value().properties.is_empty());
    }
}
