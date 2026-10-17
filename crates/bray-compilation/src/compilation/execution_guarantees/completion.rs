use bray_binder::SymbolQueryProvider;
use bray_checker::{ExecutionCallEvidence, ExecutionObligation, ExecutionProperty};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableContractsQuery, CallableInstanceData, SymbolQueryRequest};

use super::imported::ExecutionProofGraph;
use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn completed_unit_cleanup(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cleanup: &bray_bound_tree::CheckedAsync,
        cancellation: &CancellationToken,
    ) -> Result<
        std::collections::BTreeSet<(
            bray_bound_tree::AnyBoundNodeId,
            bray_bound_tree::StorageAccessId,
        )>,
        FactQueryError,
    > {
        let mut completed = std::collections::BTreeSet::new();

        if cleanup
            .scope_exits()
            .iter()
            .all(|plan| plan.lifecycle_resolution().is_empty())
            && cleanup.replacements().is_empty()
        {
            return Ok(completed);
        }

        // The cached storage publication retains this shared unit identity independently.
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;

        let types = cleanup
            .scope_exits()
            .iter()
            .flat_map(|plan| plan.lifecycle_resolution().iter().copied())
            .chain(cleanup.replacements().iter().map(|plan| plan.access()))
            .filter_map(|access| {
                storage
                    .result()
                    .value()
                    .access(access)
                    .map(|access| access.reached_type())
            })
            .collect::<std::collections::BTreeSet<_>>();

        let mut has_finalizer = false;

        for ty in types {
            if self
                .selected_lifecycle_signature(
                    ty,
                    bray_symbols::TypeAssociatedLifecycleSlot::Finalizer,
                    cancellation,
                )?
                .value()
                .is_some()
            {
                has_finalizer = true;
                break;
            }
        }

        if !has_finalizer {
            return Ok(completed);
        }

        // The query publication owns the immutable unit identity independently of its caller.
        let candidates = self.execution_candidates_with_cancellation(key.clone(), cancellation)?;

        let Some(candidate) = candidates
            .result()
            .value()
            .get(&ExecutionObligation::Property(
                ExecutionProperty::Total,
                None,
            ))
        else {
            return Ok(completed);
        };

        if candidates.result().diagnostics().has_errors() {
            return Ok(completed);
        }

        for dependency in candidate.completion_dependencies() {
            let bray_bound_tree::BoundCallableTarget::Declaration(callable) = dependency.target
            else {
                return Ok(completed);
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
                return Ok(completed);
            }
        }

        for (node, access, callable, result, evidence) in candidate.cleanup_candidates() {
            if self.finalizer_is_complete(callable, result, Some(evidence), cancellation)? {
                completed.insert((node, access));
            }
        }

        Ok(completed)
    }

    pub(in crate::compilation) fn finalizer_is_complete(
        &self,
        callable: CallableInstanceData,
        result: bray_symbols::TypeId,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let Some(total) = self.pure_total_execution(callable, evidence, cancellation)? else {
            return Ok(false);
        };

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

    pub(in crate::compilation) fn pure_total_execution(
        &self,
        callable: CallableInstanceData,
        evidence: Option<&ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        if self
            .verified_execution_obligation(
                callable,
                ExecutionObligation::Property(ExecutionProperty::Pure, None),
                evidence,
                cancellation,
            )?
            .is_none()
        {
            return Ok(None);
        }

        self.verified_execution_obligation(
            callable,
            ExecutionObligation::Property(ExecutionProperty::Total, None),
            evidence,
            cancellation,
        )
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
                assert!(
                    checked
                        .diagnostics()
                        .iter()
                        .any(|diagnostic| {
                            diagnostic.kind()
                    == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven
                        })
                );
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
                func root(pos mut value: Value)
                    requires({requirement})
                {{
                    {operation}
                }}
            "#
            );

            let mir = assert_cleanup_selection(&source, completed);

            if completed {
                assert!(
                    mir.operations().iter().all(|operation| !matches!(
                        operation.kind(),
                        MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
                    )),
                    "discharged finalizer must not construct a future: {mir:?}"
                );
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
                    func root(pos value: Value)
                    {{
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
                    func root(pos value: Value) requires({condition})
                    {{
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
            .completed_unit_cleanup(&key, body.result().value().asynchronous(), &cancellation)
            .unwrap();

        assert_eq!(
            !selected.is_empty(),
            completed,
            "{source}: {selected:?}; replacements: {:?}; candidates: {:?}",
            body.result().value().asynchronous().replacements(),
            compilation
                .execution_candidates_with_cancellation(key.clone(), &cancellation)
                .unwrap()
                .result()
                .value()
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

        assert_eq!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Destroy(place) if place.ty() == ty)), completed, "{source}");

        mir.clone()
    }
}
