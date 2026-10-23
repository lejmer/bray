use bray_binder::SymbolQueryProvider;
use bray_bound_tree::CompletedFinalizer;
use bray_checker::{
    ExecutionCallEvidence, ExecutionObligation, ExecutionProperty, FinalizerCompletion,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableContractsQuery, CallableInstanceData, SymbolQueryRequest};

use super::imported::ExecutionProofGraph;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation) type CompletedCleanup = std::collections::BTreeSet<CompletedFinalizer>;

impl Compilation {
    pub(in crate::compilation) fn completed_unit_cleanup(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<std::sync::Arc<crate::fact::PublishedUnitResult<CompletedCleanup>>, FactQueryError>
    {
        // The fact and publication each retain the immutable body identity.
        self.unit_query(
            &self.state.cleanup_completion,
            crate::fact::CompilationFactKey::CleanupCompletion(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                Ok((
                    self.compute_cleanup_completion(key, cancellation)?,
                    Box::new([]),
                ))
            },
        )
    }

    fn compute_cleanup_completion(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<bray_diagnostics::DiagnosticResult<CompletedCleanup>, FactQueryError> {
        let mut completed = CompletedCleanup::new();
        let prepared = self.prepared_body_analysis_with_cancellation(key.clone(), cancellation)?;
        let mut diagnostics = prepared.result().diagnostics().clone();

        if prepared
            .result()
            .value()
            .lifecycle_callees()
            .next()
            .is_none()
            || diagnostics.has_errors()
        {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                completed,
                diagnostics,
            ));
        }

        for (ty, finalizer) in prepared.result().value().finalizers() {
            if self.finalizer_is_complete(
                finalizer.callable,
                finalizer.result.completion_type(),
                None,
                cancellation,
            )? {
                completed.insert(CompletedFinalizer::Type(ty));
            }
        }

        // The query publication owns the immutable unit identity independently of its caller.
        let candidates = self.execution_candidates_with_cancellation(key.clone(), cancellation)?;

        diagnostics.add_range(candidates.result().diagnostics().iter().cloned());

        let Some(candidate) = candidates
            .result()
            .value()
            .get(&ExecutionObligation::Property(
                ExecutionProperty::Total,
                None,
            ))
        else {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                completed,
                diagnostics,
            ));
        };

        if candidates.result().diagnostics().has_errors() {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                completed,
                diagnostics,
            ));
        }

        let dependencies_verified = self.cleanup_dependencies_verified(candidate, cancellation)?;

        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(key, cancellation)?;

        let semantic_context =
            bray_binder::semantic_unit_context(context.symbols(), bound.result().value());

        let request =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context);

        let mut whole = std::collections::BTreeMap::new();
        let mut outcomes = Vec::new();

        for ((node, access, _, slot, normal), cleanup) in candidate.cleanup_candidates() {
            if slot == bray_symbols::TypeAssociatedLifecycleSlot::Destructor {
                outcomes.push((
                    node,
                    access,
                    normal,
                    slot,
                    cleanup,
                    FinalizerCompletion::Infallible,
                ));

                continue;
            }

            let evidence = dependencies_verified.then_some(&cleanup.evidence);

            let completion = self.finalizer_completion(
                cleanup.callable,
                cleanup.result,
                evidence,
                cancellation,
            )?;

            let complete = completion == FinalizerCompletion::Completed;

            // Represented children currently use the type-wide selected expansion.
            // Value-specific omission must not authorize an invocation that expansion retains.
            let completion = if complete
                && !cleanup.whole
                && !self.finalizer_is_complete(
                    cleanup.callable,
                    cleanup.result,
                    None,
                    cancellation,
                )? {
                FinalizerCompletion::Infallible
            } else {
                completion
            };

            outcomes.push((node, access, normal, slot, cleanup, completion));

            if cleanup.whole {
                whole
                    .entry((node, access))
                    .and_modify(|previous| *previous &= complete)
                    .or_insert(complete);
            }
        }

        completed.extend(whole.into_iter().filter_map(|((exit, access), complete)| {
            complete.then_some(CompletedFinalizer::Storage { exit, access })
        }));

        let asynchronous = matches!(
            bound.result().value().root(),
            bray_bound_tree::BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            } | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            }
        );

        let mut destruction = std::collections::BTreeMap::new();

        for (node, access, normal, slot, cleanup, completion) in outcomes {
            let completion = if completion == FinalizerCompletion::Completed
                && cleanup.whole
                && !completed.contains(&CompletedFinalizer::Storage { exit: node, access })
            {
                FinalizerCompletion::Infallible
            } else {
                completion
            };

            let ty = storage
                .result()
                .value()
                .access(access)
                .expect("selected cleanup retains its checked storage")
                .reached_type();

            let mut synchronous = cleanup.synchronous;

            if slot == bray_symbols::TypeAssociatedLifecycleSlot::Destructor
                && !asynchronous
                && !cleanup.caller_continuation
            {
                synchronous = if let Some(synchronous) = destruction.get(&ty) {
                    *synchronous
                } else {
                    let result = prepared
                        .result()
                        .value()
                        .destruction_is_synchronous(request, ty, &mut |call| {
                            self.finalizer_is_complete(
                                call.callable,
                                call.result.completion_type(),
                                None,
                                cancellation,
                            )
                            .map_err(crate::compilation::checker::checker_query_error)
                        })
                        .map_err(FactQueryError::from)?;

                    diagnostics.add_range(result.diagnostics().iter().cloned());
                    destruction.insert(ty, *result.value());

                    *result.value()
                };
            }

            bray_checker::check_implicit_finalization(
                request,
                node,
                ty,
                cleanup,
                synchronous,
                normal,
                completion,
                &mut diagnostics,
            )
            .map_err(FactQueryError::from)?;
        }

        Ok(bray_diagnostics::DiagnosticResult::new(
            completed,
            diagnostics,
        ))
    }

    fn cleanup_dependencies_verified(
        &self,
        candidate: &bray_checker::ExecutionCandidate,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        for dependency in candidate.completion_dependencies() {
            let bray_bound_tree::BoundCallableTarget::Declaration(callable) = dependency.target
            else {
                return Ok(false);
            };

            if self
                .verified_execution_obligation(
                    callable,
                    ExecutionObligation::Postcondition(dependency.source),
                    candidate.call_evidence(dependency.node.into()),
                    cancellation,
                )?
                .is_none()
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(in crate::compilation) fn finalizer_is_complete(
        &self,
        callable: CallableInstanceData,
        result: bray_symbols::TypeId,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        Ok(
            self.finalizer_completion(callable, result, evidence, cancellation)?
                == FinalizerCompletion::Completed,
        )
    }

    fn finalizer_completion(
        &self,
        callable: CallableInstanceData,
        result: bray_symbols::TypeId,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<FinalizerCompletion, FactQueryError> {
        let total = self.verified_execution_obligation(
            callable,
            ExecutionObligation::Property(ExecutionProperty::Total, None),
            evidence,
            cancellation,
        )?;

        let inferred_success = match total {
            Some(total) => {
                self.finalizer_returns_success(callable, result, total, evidence, cancellation)?
            }
            None => false,
        };

        if !inferred_success
            && !self.finalizer_promises_success(callable, evidence, cancellation)?
        {
            return Ok(FinalizerCompletion::Unresolved);
        }

        if total.is_some()
            && self
                .verified_execution_obligation(
                    callable,
                    ExecutionObligation::Property(ExecutionProperty::Pure, None),
                    evidence,
                    cancellation,
                )?
                .is_some()
        {
            Ok(FinalizerCompletion::Completed)
        } else {
            Ok(FinalizerCompletion::Infallible)
        }
    }

    fn finalizer_promises_success(
        &self,
        callable: CallableInstanceData,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let Some(result) = self
            .available_compiler_known_symbols()
            .result_representation()
        else {
            return Ok(false);
        };

        let expected = bray_checker::ExecutionCondition::VariantTest(
            result.success_variant(),
            std::sync::Arc::new(bray_checker::ExecutionCondition::Result),
        );

        let contracts = self.callable_completion_contracts(callable, cancellation)?;

        if contracts.diagnostics().has_errors() {
            return Ok(false);
        }

        let mut postconditions = Vec::new();

        for contract in contracts.into_parts().0 {
            if !evidence
                .unwrap_or(&ExecutionCallEvidence::default())
                .proves_trusted(&contract.entry)
            {
                continue;
            }

            for (condition, clause) in contract.postconditions {
                if self
                    .verified_execution_obligation(
                        callable,
                        ExecutionObligation::Postcondition(clause),
                        evidence,
                        cancellation,
                    )?
                    .is_some()
                {
                    postconditions.push(condition);
                }
            }
        }

        Ok(bray_checker::execution_condition_is_implied(
            &expected,
            &postconditions,
            &std::collections::BTreeMap::new(),
        ))
    }

    fn finalizer_returns_success(
        &self,
        callable: CallableInstanceData,
        result: bray_symbols::TypeId,
        total: ExecutionObligation,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        if self.cleanup_result_is_success(callable, result, total, cancellation)? {
            return Ok(true);
        }

        // A general total promise can return either Result case, while a more
        // specific applicable domain proves Ok. Every applicable guarantee holds.
        let alternatives = if let Some(body) = self.callable_body_key(callable.definition())? {
            let candidates = self.execution_candidates_with_cancellation(body, cancellation)?;

            candidates
                .result()
                .value()
                .keys()
                .copied()
                .collect::<Vec<_>>()
        } else {
            let mut diagnostics = DiagnosticBag::new();

            let contract =
                self.imported_execution_contract(callable, &mut diagnostics, cancellation)?;

            contract
                .domains
                .iter()
                .filter(|domain| domain.properties.contains(&ExecutionProperty::Total))
                .map(|domain| {
                    ExecutionObligation::Property(
                        ExecutionProperty::Total,
                        Some(bray_checker::ExecutionClauseId::Imported(domain.ordinal)),
                    )
                })
                .collect()
        };

        for required in alternatives {
            if required == total
                || !matches!(
                    required,
                    ExecutionObligation::Property(ExecutionProperty::Total, Some(_))
                )
            {
                continue;
            }

            if let Some(selected) =
                self.verified_execution_obligation(callable, required, evidence, cancellation)?
                && self.cleanup_result_is_success(callable, result, selected, cancellation)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn cleanup_result_is_success(
        &self,
        callable: CallableInstanceData,
        result: bray_symbols::TypeId,
        total: ExecutionObligation,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        use bray_compiler_known::RepresentationRole;
        use bray_symbols::{GenericArgument, TypeData};

        let values = self.semantic_value_store()?;

        let data = values.type_data(result);

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(false);
        };

        match super::super::foreign::compiler_known_representation(self, *definition) {
            Some(RepresentationRole::Unit) => return Ok(true),
            Some(RepresentationRole::Result) => {}
            _ => return Ok(false),
        }

        let substitution = values.generic_substitution_data(*substitution);

        let [success, _] = substitution.bindings() else {
            return Ok(false);
        };

        let GenericArgument::Type(success) = success.argument() else {
            return Ok(false);
        };

        let success = values.type_data(success);

        let TypeData::Named { definition, .. } = success.as_ref() else {
            return Ok(false);
        };

        if super::super::foreign::compiler_known_representation(self, *definition)
            != Some(RepresentationRole::Unit)
        {
            return Ok(false);
        }

        let Some(body) = self.callable_body_key(callable.definition())? else {
            return self.imported_cleanup_result_is_success(callable, total, cancellation);
        };

        let candidates = self.execution_candidates_with_cancellation(body, cancellation)?;

        let variant = candidates
            .result()
            .value()
            .get(&total)
            .and_then(|candidate| candidate.result_variant());

        Ok(variant.is_some_and(|variant| {
            self.available_compiler_known_symbols()
                .result_representation()
                .is_some_and(|representation| representation.success_variant() == variant)
        }))
    }

    fn imported_cleanup_result_is_success(
        &self,
        callable: CallableInstanceData,
        total: ExecutionObligation,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let ExecutionObligation::Property(
            ExecutionProperty::Total,
            Some(bray_checker::ExecutionClauseId::Imported(ordinal)),
        ) = total
        else {
            panic!("imported finalizer completion retains its certified total entry domain");
        };

        let mut diagnostics = DiagnosticBag::new();

        let contract =
            self.imported_execution_contract(callable, &mut diagnostics, cancellation)?;

        let domain = contract
            .domains
            .iter()
            .find(|domain| domain.ordinal == ordinal)
            .expect("certified imported execution domain must exist");

        let success = self
            .available_compiler_known_symbols()
            .result_representation()
            .expect("Result finalizer has the checked Result representation")
            .success_variant();

        let expected = bray_checker::ExecutionCondition::VariantTest(
            success,
            std::sync::Arc::new(bray_checker::ExecutionCondition::Result),
        );

        for (post, term) in &*domain.postconditions {
            let conditions = self.imported_execution_conditions(callable, [*term], cancellation)?;

            if conditions.as_slice() == std::slice::from_ref(&expected)
                && self
                    .verified_execution_obligation(
                        callable,
                        ExecutionObligation::Postcondition(
                            bray_checker::ExecutionClauseId::Imported(*post),
                        ),
                        None,
                        cancellation,
                    )?
                    .is_some()
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub(in crate::compilation) fn verified_execution_obligation(
        &self,
        callable: CallableInstanceData,
        required: ExecutionObligation,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        if let Some(body) = self.callable_body_key(callable.definition())? {
            let declaration = self.execution_declaration(body.source().syntax())?;

            if declaration.diagnostics().has_errors() {
                return Ok(None);
            }

            let selected = match required {
                ExecutionObligation::Property(property, _) => self
                    .applicable_execution_obligation(
                        &body,
                        callable,
                        declaration.value(),
                        required,
                        evidence,
                        cancellation,
                    )?
                    .map(|clause| ExecutionObligation::Property(property, clause.map(Into::into))),
                postcondition => Some(postcondition),
            };

            let Some(selected) = selected else {
                return Ok(None);
            };

            let certified = self.certified_execution_with_cancellation(body, cancellation)?;
            let proof = certified.result().value();

            return Ok((!certified.result().diagnostics().has_errors()
                && match selected {
                    ExecutionObligation::Property(property, None) => {
                        proof.properties.contains(&property)
                    }
                    ExecutionObligation::Property(
                        property,
                        Some(bray_checker::ExecutionClauseId::Source(source)),
                    ) => proof.guarded_properties.contains(&(source, property)),
                    ExecutionObligation::Postcondition(
                        bray_checker::ExecutionClauseId::Source(source),
                    ) => proof.postconditions.contains(&source),
                    _ => false,
                })
            .then_some(selected));
        }

        let mut diagnostics = DiagnosticBag::new();

        let Some(selected) = self.select_imported_execution_obligation(
            callable,
            required,
            evidence,
            &mut diagnostics,
            cancellation,
        )?
        else {
            return Ok(None);
        };

        self.verified_imported_execution_obligation(callable, selected, diagnostics, cancellation)
    }

    fn verified_imported_execution_obligation(
        &self,
        callable: CallableInstanceData,
        selected: ExecutionObligation,
        mut diagnostics: DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        let mut graph = ExecutionProofGraph::new();

        let valid = self.append_imported_execution_proof(
            callable,
            selected,
            &mut graph,
            &mut diagnostics,
            cancellation,
        )?;

        Ok((valid
            && !diagnostics.has_errors()
            && bray_symbols::check_execution_proof_dependencies(&graph).is_empty())
        .then_some(selected))
    }

    /// Certifies a property on every legal input, without authorizing any invocation.
    pub(in crate::compilation) fn certified_entry_property(
        &self,
        callable: CallableInstanceData,
        property: ExecutionProperty,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        if let Some(body) = self.callable_body_key(callable.definition())? {
            let certified = self.certified_execution_with_cancellation(body, cancellation)?;

            return Ok(!certified.result().diagnostics().has_errors()
                && certified.result().value().properties.contains(&property));
        }

        let context = self.binding_context(cancellation)?;

        let contract = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(crate::compilation::binder::binding_query_error)?;

        if contract.diagnostics().has_errors()
            || !contract
                .value()
                .phase_behaviors()
                .deferred_execution()
                .unwrap_or_else(|| contract.value().invocation_behavior())
                .execution_properties()
                .contains(&property)
        {
            return Ok(false);
        }

        let requirements = contract
            .value()
            .invocation_preconditions()
            .iter()
            .filter_map(|clause| {
                clause
                    .predicate()
                    .and_then(|predicate| predicate.condition())
            })
            .collect::<std::collections::BTreeSet<_>>();

        // Compare terms in the declaration's own domain. A guarded promise applies only
        // when its entire entry follows from the callable's required inputs.
        let Some(domain) = contract
            .value()
            .execution_contract()
            .domains
            .iter()
            .find(|domain| {
                domain.properties.contains(&property) && domain.requires_only(&requirements)
            })
        else {
            return Ok(false);
        };

        Ok(self
            .verified_imported_execution_obligation(
                callable,
                ExecutionObligation::Property(
                    property,
                    Some(bray_checker::ExecutionClauseId::Imported(domain.ordinal)),
                ),
                DiagnosticBag::new(),
                cancellation,
            )?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use crate::CancellationToken;
    use crate::test_support::{compilation, source_function_body_key};
    use bray_ir::MirOperationKind;

    #[test]
    fn destructor_guarantees_require_the_current_receiver() {
        for (requirements, body, valid) in [
            ("value.ready", "", true),
            ("!value.ready", "", false),
            ("value.ready", "value.ready = false;", false),
            ("!value.ready", "value.ready = true;", true),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value {{
                    mut ready: bool;
                    destruct() when(self.ready) {{ executes(total) }}
                    {{ if !self.ready {{ panic("unfinished"); }} }}
                }}
                func root(pos mut value: Value)
                    requires({requirements}) executes(total)
                {{ {body} }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "root"))
                .unwrap();

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {checked:#?}"
            );
        }
    }

    #[test]
    fn current_value_selects_cleanup_after_mutation() {
        for (requirement, operation, completed) in [
            ("value.ready", "", true),
            ("!value.ready", "", false),
            ("value.ready", "value.ready = false;", false),
            ("!value.ready", "value.ready = true;", true),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    finalize()
                        when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready
                        {{
                            panic("unfinished");
                        }}
                    }}
                }}
                func root(pos mut value: Value)
                    requires({requirement})
                {{
                    {operation}
                }}
            "#
            );

            assert_cleanup_selection(&source, completed);
        }
    }

    #[test]
    fn async_execution_certification_keeps_body_cancellation_and_suspension() {
        for (body, valid) in [
            ("", true),
            ("await child();", false),
            ("panic(\"failure\");", false),
        ] {
            let source = format!(
                r#"
                module app;
                async func child()
                {{
                }}
                async func root() executes(pure, total)
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "root"))
                .unwrap();

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {checked:#?}"
            );

            if !valid {
                assert!(checked.diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind()
                        == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven
                }));
            }
        }
    }

    #[test]
    fn completed_async_finalizers_do_not_construct_futures() {
        for (requirement, operation, completed) in [
            ("value.ready", "", true),
            ("!value.ready", "", false),
            ("value.ready", "value.ready = false;", false),
            ("!value.ready", "value.ready = true;", true),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    async finalize()
                        when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready
                        {{
                            panic("unfinished");
                        }}
                    }}
                    destruct()
                    {{
                        self.ready = false;
                    }}
                }}
                async func root(pos mut value: Value)
                    requires({requirement})
                {{
                    {operation}
                }}
            "#
            );

            let mir = assert_cleanup_selection(&source, completed);

            if completed {
                let entry = mir.block(mir.entry()).unwrap();

                let bray_ir::MirTerminatorKind::Branch { else_edge, .. } =
                    entry.terminator().kind()
                else {
                    panic!("async body entry checks cancellation before executing");
                };

                // Deferred entry requirements hold on the executing path. Abandonment
                // before entry must still resolve captures without those assumptions.
                let mut pending = vec![else_edge.target()];
                let mut visited = std::collections::BTreeSet::new();

                while let Some(id) = pending.pop() {
                    if !visited.insert(id) {
                        continue;
                    }

                    let block = mir.block(id).unwrap();

                    assert!(
                        block.operations().iter().all(|id| !matches!(
                            mir.operation(*id).unwrap().kind(),
                            MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
                        )),
                        "discharged finalizer must not construct a future on the executing path: {source}"
                    );

                    block
                        .terminator()
                        .kind()
                        .for_each_successor(|target| pending.push(target));
                }
            }
        }
    }

    #[test]
    fn replacement_uses_old_value_after_rhs_evaluation() {
        for (replacement, completed) in [
            ("Value { ready = false }", true),
            ("value.replace()", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    mut func replace() -> Value
                    {{
                        self.ready = false;
                        return Value
                        {{
                            ready = false
                        }};
                    }}
                    finalize()
                        when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready
                        {{
                            panic("unfinished");
                        }}
                    }}
                }}
                func root(pos mut value: Value)
                    requires(value.ready)
                {{
                    value = {replacement};
                }}
            "#
            );

            assert_cleanup_selection(&source, completed);
        }
    }

    #[test]
    fn successful_unit_result_selects_cleanup() {
        for execution in ["", "async "] {
            for (result, completed) in [("Ok(unit)", true), ("Error(true)", false)] {
                let source = format!(
                    r#"
                    module app;
                    struct Value
                    {{
                        {execution}finalize() -> Result<unit, bool>
                            executes(pure, total)
                        {{
                            return {result};
                        }}
                    }}
                    async func root(pos value: Value)
                    {{
                        panic("abnormal cleanup");
                    }}
                "#
                );

                assert_cleanup_selection(&source, completed);
            }
        }
    }

    #[test]
    fn a_guarded_success_proof_refines_general_total_finalization() {
        for execution in ["", "async "] {
            for (condition, completed) in [("value.ready", true), ("!value.ready", false)] {
                let source = format!(
                    r#"
                    module app;
                    struct Value
                    {{
                        ready: bool;
                        {execution}finalize() -> Result<unit, bool>
                            executes(pure, total)
                            when(self.ready) {{ executes(pure, total) }}
                        {{
                            if self.ready
                            {{
                                return Ok(unit);
                            }}
                            return Error(true);
                        }}
                    }}
                    async func root(pos value: Value) requires({condition})
                    {{
                        panic("abnormal cleanup");
                    }}
                "#
                );

                assert_cleanup_selection(&source, completed);
            }
        }
    }

    #[test]
    fn joins_require_completion_on_every_incoming_path() {
        for (alternative, completed) in [("true", true), ("false", false)] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    finalize()
                        when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready
                        {{
                            panic("unfinished");
                        }}
                    }}
                }}
                func root(pos mut value: Value, pos choose: bool)
                {{
                    if choose
                    {{
                        value.ready = true;
                    }}
                    else
                    {{
                        value.ready = {alternative};
                    }}
                }}
            "#
            );

            assert_cleanup_selection(&source, completed);
        }
    }

    #[test]
    fn returned_errors_preserve_receiver_completion_and_retry_obligations() {
        for (postcondition, state, completed) in [
            ("self.ready", "true", true),
            ("!self.ready", "false", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    mut func finish() -> Result<unit, bool>
                        ensures({postcondition})
                    {{
                        self.ready = {state};
                        return Error(true);
                    }}
                    finalize()
                        when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready
                        {{
                            panic("unfinished");
                        }}
                    }}
                }}
                func root(pos mut value: Value) -> Result<unit, bool>
                {{
                    return value.finish();
                }}
            "#
            );

            let mir = assert_cleanup_selection(&source, completed);

            assert!(
                mir.operations()
                    .iter()
                    .any(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
            );

            assert!(mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                bray_ir::MirTerminatorKind::Return(Some(_))
            )));
        }
    }

    #[test]
    fn normal_cleanup_rejects_unresolved_finalization() {
        for (execution, result, body, expected) in [
            (
                "",
                " -> Result<unit, bool>",
                "return Error(true);",
                bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
            ),
            (
                "async ",
                "",
                "",
                bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            ),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    {execution}finalize(){result} executes(total)
                    {{
                        {body}
                    }}
                }}
                func root(pos value: Value) {{}}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);

            let key = source_function_body_key(&compilation, "root");
            let cancellation = CancellationToken::new();

            let first = compilation
                .completed_unit_cleanup(&key, &cancellation)
                .unwrap();

            let second = compilation
                .completed_unit_cleanup(&key, &cancellation)
                .unwrap();

            assert!(std::sync::Arc::ptr_eq(&first, &second));
            assert!(compilation.lowered_unit(key).unwrap().value().is_none());

            let diagnostic = diagnostics
                .iter()
                .find(|item| item.kind() == expected)
                .unwrap();

            let rendered = bray_messages::DiagnosticRenderer::english().render(diagnostic);

            assert!(rendered.message().contains("Value"), "{rendered:?}");
            assert_eq!(rendered.labels()[0].message(), "ownership ends here");
        }
    }

    #[test]
    fn synchronous_cleanup_requires_live_async_completion_evidence() {
        for (operation, valid) in [("", true), ("value.ready = false;", false)] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    async finalize() when(self.ready) {{ executes(pure, total) }}
                    {{
                        if !self.ready {{ panic("unfinished"); }}
                    }}
                }}
                func root(pos mut value: Value) requires(value.ready)
                {{
                    {operation}
                }}
            "#
            );

            if valid {
                let mir = assert_cleanup_selection(&source, true);

                assert!(mir.operations().iter().all(|operation| !matches!(
                    operation.kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
                )));
            } else {
                let compilation = compilation(&source);

                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
                );
            }
        }
    }

    #[test]
    fn normal_cleanup_tracks_transferred_represented_and_discarded_owners() {
        for (body, valid) in [
            (
                r#"
                func root(pos value: Value) -> Value
                {
                    return value;
                }
            "#,
                true,
            ),
            (
                r#"
                func root(pos value: Value) -> Wrapper
                {
                    return Wrapper { item = value };
                }
            "#,
                true,
            ),
            (
                r#"
                func root(pos owner: Wrapper) -> Value
                {
                    return owner.item;
                }
            "#,
                true,
            ),
            (
                r#"
                func root(pos value: Value)
                {
                    let owner = Wrapper { item = value };
                }
            "#,
                false,
            ),
            (
                r#"
                func root()
                {
                    let value = Value {};
                }
            "#,
                false,
            ),
            (
                r#"
                func make() -> Value
                {
                    return Value {};
                }
                func root()
                {
                    make();
                }
            "#,
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    finalize() -> Result<unit, bool> executes(total)
                    {{
                        return Error(true);
                    }}
                }}
                struct Wrapper
                {{
                    item: Value;
                }}
                {body}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            if valid {
                assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
                );
            }
        }
    }

    #[test]
    fn completed_parent_keeps_its_represented_finalization_obligation() {
        let source = r#"
            module app;
            struct Value
            {
                finalize() -> Result<unit, bool> executes(total)
                {
                    return Error(true);
                }
            }
            struct Wrapper
            {
                item: Value;
                finalize() executes(pure, total) {}
            }
            func root(pos owner: Wrapper) {}
        "#;

        let compilation = compilation(source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
        );
    }

    #[test]
    fn normal_cleanup_uses_verified_postconditions_without_total() {
        for (guarded, mutation, valid) in [
            (false, "", true),
            (true, "", true),
            (true, "value.ready = false;", false),
        ] {
            let contract = if guarded {
                "when(self.ready) { ensures(result matches Ok(_)) }"
            } else {
                "ensures(result matches Ok(_))"
            };

            let failure = if guarded {
                "return Error(true);"
            } else {
                r#"panic("failed finalizer");"#
            };

            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    mut ready: bool;
                    finalize() -> Result<unit, bool> {contract}
                    {{
                        if self.ready
                        {{
                            return Ok(unit);
                        }}
                        {failure}
                    }}
                }}
                func root(pos mut value: Value) requires(value.ready)
                {{
                    {mutation}
                }}
            "#
            );

            if valid {
                assert_cleanup_selection(&source, false);
            } else {
                let compilation = compilation(&source);

                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
                );
            }
        }
    }

    #[test]
    fn normal_cleanup_accepts_effectful_proven_success_without_omitting_it() {
        let source = r#"
            module app;
            struct Value
            {
                mut ready: bool;
                finalize() -> Result<unit, bool> executes(total)
                {
                    self.ready = true;
                    return Ok(unit);
                }
            }
            func root(pos value: Value) {}
        "#;

        assert_cleanup_selection(source, false);
    }

    fn assert_cleanup_selection(source: &str, completed: bool) -> bray_ir::MirUnit {
        let compilation = compilation(source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "root");
        let cancellation = CancellationToken::new();

        let body = compilation
            .body_semantics_with_cancellation(key.clone(), &cancellation)
            .unwrap();

        let selected = compilation
            .completed_unit_cleanup(&key, &cancellation)
            .unwrap();

        assert_eq!(
            !selected.result().value().is_empty(),
            completed,
            "{source}: {selected:?}; invalidations: {:?}",
            body.result().value().refinements().invalidations(),
        );

        let symbols = compilation.symbol_graph().unwrap();

        let definition = symbols
            .structures()
            .iter()
            .find(|value| value.origin() == bray_symbols::SymbolOrigin::Source)
            .unwrap();

        let ty = crate::compilation::substitution::named_type(
            compilation.semantic_value_store().unwrap(),
            bray_symbols::NamedTypeSymbolId::Struct(definition.id()),
        )
        .unwrap();

        let lowered = compilation.lowered_unit(key).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let destroys_owner = mir.operations().iter().any(|operation| {
            matches!(operation.kind(), MirOperationKind::Destroy { place, .. } if place.ty() == ty)
        });

        let awaits_finalizer = mir.operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )
        });

        assert_eq!(destroys_owner, completed || awaits_finalizer, "{source}");

        mir.clone()
    }
}
