use bray_checker::{ExecutionCallEvidence, ExecutionObligation, ExecutionProperty};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::CallableInstanceData;

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
                    candidate.call_evidence(dependency.node),
                    cancellation,
                )?
                .is_none()
            {
                return Ok(completed);
            }
        }

        for (node, access, callable, result, evidence) in candidate.cleanup_candidates() {
            if let Some(total) =
                self.pure_total_execution(callable, Some(evidence), cancellation)?
                && self.cleanup_result_is_success(callable, result, total, cancellation)?
            {
                completed.insert((node, access));
            }
        }

        Ok(completed)
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
            return Ok(false);
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
                        declaration.value(),
                        property,
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
        for (result, completed) in [("Ok(unit)", true), ("Error(true)", false)] {
            let source = format!(
                r#"
                module app;
                struct Value
                {{
                    finalize() -> Result<unit, bool>
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
