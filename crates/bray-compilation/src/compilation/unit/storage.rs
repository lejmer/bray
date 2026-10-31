use std::sync::Arc;

use bray_binder::{BindingQueryContext, semantic_unit_context};
use bray_bound_tree::{BoundUnitKey, CheckedBodySemantics, StoragePlan};
use bray_checker::{BodySemanticChecker, DefaultBodySemanticChecker};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::plan_storage;
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn static_dependency_roots(
        &self,
        key: &BoundUnitKey,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
    ) -> Result<
        std::collections::BTreeMap<
            bray_symbols::StaticSymbolId,
            bray_symbols::DependencySubjectRoot,
        >,
        FactQueryError,
    > {
        let binding = self.binding_context_for(key, cancellation)?;

        selections
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::StaticReference(selection) => {
                    Some(selection.template().declaration())
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|declaration| {
                super::super::binder::static_dependency_root(&binding, declaration)
                    .map(|root| (declaration, root))
                    .map_err(super::super::binder::binding_query_error)
            })
            .collect()
    }

    pub(in crate::compilation) fn storage_plan_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<StoragePlan>>, FactQueryError> {
        self.unit_query(
            &self.state.storage_plans,
            CompilationFactKey::StoragePlan(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let result = plan_storage(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    declared.result().value(),
                    expressions.result().value(),
                    patterns.result().value(),
                )?;

                let (plan, plan_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    declared.result().diagnostics(),
                    expressions.result().diagnostics(),
                    patterns.result().diagnostics(),
                    &plan_diagnostics,
                ]);

                Ok((DiagnosticResult::new(plan, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn prepared_body_analysis_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<bray_checker::PreparedBodyAnalysis>>, FactQueryError> {
        self.unit_query(
            &self.state.prepared_body_analysis,
            CompilationFactKey::PreparedBodyAnalysis(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;
                let semantic = semantic_unit_context(context.symbols(), bound.result().value());

                let request =
                    bray_checker::CheckerUnitView::new(bound.result().value(), &semantic, &context);

                let result = checker_result(bray_checker::prepare_body_analysis(
                    request,
                    expressions.result().value(),
                    patterns.result().value(),
                    storage.result().value(),
                ))?;

                let (prepared, diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    expressions.result().diagnostics(),
                    patterns.result().diagnostics(),
                    storage.result().diagnostics(),
                    &diagnostics,
                ]);

                Ok((DiagnosticResult::new(prepared, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn body_analysis_inputs(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<BodyAnalysisInputs, FactQueryError> {
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let expressions = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let behavior = self.selected_body_behavior_with_cancellation(key.clone(), cancellation)?;

        let prepared = self.prepared_body_analysis_with_cancellation(key.clone(), cancellation)?;

        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;

        let contracts = self.trusted_contract_inputs(
            &key,
            bound.result().value(),
            expressions.result().value(),
            cancellation,
        )?;

        let non_unwinding = self.checked_unwind(key.clone(), cancellation)?;

        let static_dependencies = self.static_dependency_roots(
            &key,
            expressions.result().value().selections(),
            cancellation,
        )?;

        Ok(BodyAnalysisInputs {
            key,
            bound,
            expressions,
            behavior,
            prepared,
            patterns,
            storage,
            memory,
            contracts,
            non_unwinding,
            static_dependencies,
        })
    }

    pub(in crate::compilation) fn body_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<Arc<CheckedBodySemantics>>>, FactQueryError> {
        self.unit_query(
            &self.state.body_semantics,
            CompilationFactKey::BodySemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let source_static = key.kind() == bray_bound_tree::BoundUnitKind::ConstantTemplate
                    && matches!(
                        self.binding_context_for(&key, cancellation)?
                            .symbols()
                            .symbol_for_key(key.declared_owner()),
                        Some(bray_symbols::AnySymbolId::Static(_))
                    );

                if source_static
                    || matches!(
                        key.kind(),
                        bray_bound_tree::BoundUnitKind::CallableBody
                            | bray_bound_tree::BoundUnitKind::AnonymousCallable
                            | bray_bound_tree::BoundUnitKind::RuntimeDefault
                    )
                {
                    let context = self.binding_context_for(&key, cancellation)?;

                    let result = crate::compilation::binder::source_body_semantics(&context, &key)
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    return Ok((result, Box::new([])));
                }

                let inputs = self.body_analysis_inputs(key.clone(), cancellation)?;
                let result = inputs.check(self, cancellation, None, &Default::default())?;

                Ok((result, Box::new([])))
            },
        )
    }
}

/// Immutable body prerequisites shared by every round of a demanded call component.
pub(in crate::compilation) struct BodyAnalysisInputs {
    key: BoundUnitKey,
    pub(in crate::compilation) bound: Arc<PublishedUnitResult<bray_bound_tree::BoundUnit>>,
    pub(in crate::compilation) expressions:
        Arc<PublishedUnitResult<bray_bound_tree::CheckedExpressionSemantics>>,
    pub(in crate::compilation) patterns: Arc<PublishedUnitResult<bray_bound_tree::CheckedPatterns>>,
    pub(in crate::compilation) behavior:
        Arc<PublishedUnitResult<bray_bound_tree::BodyBehaviorContributions>>,
    pub(in crate::compilation) prepared:
        Arc<PublishedUnitResult<bray_checker::PreparedBodyAnalysis>>,
    storage: Arc<PublishedUnitResult<StoragePlan>>,
    memory: Arc<PublishedUnitResult<bray_bound_tree::CheckedMemoryOperations>>,
    contracts: DiagnosticResult<bray_checker::TrustedContractInputs>,
    non_unwinding: bray_bound_tree::CheckedUnwind,
    static_dependencies: std::collections::BTreeMap<
        bray_symbols::StaticSymbolId,
        bray_symbols::DependencySubjectRoot,
    >,
}

impl BodyAnalysisInputs {
    pub(in crate::compilation) fn check(
        &self,
        compilation: &Compilation,
        cancellation: &CancellationToken,
        templates: Option<
            &std::collections::BTreeMap<
                bray_checker::ResultDependencyOwner,
                bray_symbols::DependencyContractTemplateId,
            >,
        >,
        recursive: &std::collections::BTreeSet<bray_symbols::CallableSymbolId>,
    ) -> Result<DiagnosticResult<Arc<CheckedBodySemantics>>, FactQueryError> {
        let context = compilation.checker_context_for(&self.key, cancellation)?;

        let semantic_context =
            semantic_unit_context(context.symbols(), self.bound.result().value());

        let mut unit = bray_checker::CheckerUnitView::new(
            self.bound.result().value(),
            &semantic_context,
            &context,
        )
        .with_trusted_contracts(self.contracts.value())
        .with_unwind(&self.non_unwinding)
        .with_static_dependencies(&self.static_dependencies);

        let settled;

        let templates = if let Some(templates) = templates {
            templates
        } else {
            settled = compilation
                .execution_default_inputs(self.behavior.result().value(), cancellation)?;

            &settled
        };

        unit = unit.with_result_dependencies(templates, recursive);

        let result = checker_result(DefaultBodySemanticChecker.check_body_semantics(
            unit,
            self.prepared.result().value(),
            self.behavior.result().value(),
            self.expressions.result().value(),
            self.patterns.result().value(),
            self.storage.result().value(),
            self.memory.result().value(),
        ))?;

        let (semantics, mut diagnostics) = result.into_parts();

        diagnostics.add_range(self.contracts.diagnostics().iter().cloned());
        diagnostics.add_range(self.prepared.result().diagnostics().iter().cloned());
        diagnostics.add_range(self.behavior.result().diagnostics().iter().cloned());

        Ok(DiagnosticResult::new(
            Arc::new(semantics.with_unwind(self.non_unwinding.clone())),
            diagnostics,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use crate::test_support::{compilation, source_function_body_key};

    #[test]
    #[ignore = "tier:proof"]
    fn dense_scalar_assignments_retain_only_current_storage() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    return input;
                }

                func caller() -> bool
                {
                    let mut value: bool = true;
                "#,
            );

            for _ in 0..count {
                writeln!(source, "value = identity(value);")
                    .expect("fixture source must be constructed");
            }

            source.push_str("\nreturn value;\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("dense scalar body must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
            assert_eq!(flow.value().replacements().len(), count);

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "expired scalar temporaries must not accumulate in exit state"
            );
        }
    }

    #[test]
    #[ignore = "tier:proof"]
    fn moved_cleanup_free_values_retain_only_current_storage() {
        for count in [64, 128, 256, 1024] {
            for partial in [false, true] {
                let mut source = String::from(
                    r#"
                    module example;

                    struct Item
                    {
                    }

                    struct Wrapper
                    {
                        value: Item;
                    }

                    func take(pos input: Item)
                    {
                    }

                    func caller()
                    {
                    "#,
                );

                for index in 0..count {
                    if partial {
                        writeln!(
                            source,
                            "let wrapper{index} = Wrapper {{ value = Item {{}}, }};"
                        )
                        .expect("fixture source must be constructed");

                        writeln!(source, "let moved{index} = wrapper{index}.value;")
                            .expect("fixture source must be constructed");
                    } else {
                        writeln!(source, "let item{index} = Item {{}};")
                            .expect("fixture source must be constructed");

                        writeln!(source, "let moved{index} = item{index};")
                            .expect("fixture source must be constructed");
                    }

                    writeln!(source, "take(moved{index});")
                        .expect("fixture source must be constructed");
                }

                source.push_str("\n}\n");

                let compilation = compilation(&source);

                assert!(
                    compilation.check_diagnostics().is_empty(),
                    "{:?}",
                    compilation.check_diagnostics()
                );

                let key = source_function_body_key(&compilation, "caller");

                let liveness = compilation
                    .liveness(key.clone())
                    .expect("fixture `liveness` call must succeed");

                assert!(!liveness.value().is_recovered());

                let flow = compilation
                    .storage_flow(key)
                    .expect("cleanup-free move-only body must check");

                assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

                assert!(
                    flow.value()
                        .operations()
                        .iter()
                        .filter(|operation| {
                            operation.purpose() == bray_bound_tree::StorageAccessPurpose::Move
                                && operation.status()
                                    == bray_bound_tree::StorageOperationStatus::Valid
                        })
                        .count()
                        >= count
                );

                assert!(
                    flow.value()
                        .exits()
                        .iter()
                        .all(|exit| exit.live().len() <= 8),
                    "expired move-only storage must not accumulate at exits"
                );

                assert!(
                    flow.value()
                        .exits()
                        .iter()
                        .all(|exit| exit.moved().len() <= 8),
                    "expired move history must not accumulate at exits"
                );
            }
        }
    }

    #[test]
    fn scalar_temporary_borrows_survive_until_their_parent_consumes_them() {
        let compilation = compilation(
            r#"
            module example;

            func identity(pos input: bool) -> bool
            {
                return input;
            }

            func read(pos input: &bool) -> bool
            {
                return input;
            }

            func caller() -> bool
            {
                let borrowed = &identity(true);

                return read(borrowed);
            }
        "#,
        );

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "caller"))
            .expect("borrowed scalar temporary must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
    }
    #[test]
    fn scalar_temporaries_expire_across_branches_and_loop_iterations() {
        let compilation = compilation(
            r#"
            module example;

            func identity(pos input: bool) -> bool
            {
                return input;
            }

            func caller(pos condition: bool) -> bool
            {
                let mut value: bool = true;

                if condition
                {
                    value = identity(false);
                }
                else
                {
                    value = identity(true);
                }

                while value
                {
                    value = identity(false);
                }

                return value;
            }
        "#,
        );

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "caller"))
            .expect("joined and looped scalar body must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
        assert_eq!(flow.value().replacements().len(), 3);

        assert!(
            flow.value()
                .exits()
                .iter()
                .all(|exit| exit.live().len() <= 8),
            "expired branch and loop temporaries must not accumulate"
        );
    }

    #[test]
    fn unused_temporary_destructors_remain_scope_exit_obligations() {
        let compilation = compilation(
            r#"
            module example;

            struct Guard
            {
                value: bool;
                destruct()
                {
                }
            }

            func create() -> Guard
            {
                return Guard
                {
                    value = true,
                };
            }

            func caller()
            {
                create();
            }
        "#,
        );

        let key = source_function_body_key(&compilation, "caller");

        let bound = compilation
            .bound_unit(key.clone())
            .expect("guard body must bind");

        let call = bound
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, bray_bound_tree::BoundExpression::Call(_)).then_some(id)
            })
            .expect("guard creation must retain its call");

        let storage = compilation
            .storage_plan(key.clone())
            .expect("guard storage must plan");

        let identity = storage.value().identity_entries().find_map(|(id, identity)| matches!(identity, bray_bound_tree::StorageIdentity::Temporary(expression) if expression == call).then_some(id)).expect("guard result must retain a temporary identity");

        let flow = compilation
            .storage_flow(key)
            .expect("guard cleanup must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

        assert!(
            flow.value()
                .exits()
                .iter()
                .any(|exit| exit.live().contains(&identity)
                    && exit.initialized().contains(&identity)),
            "unused cleanup-bearing temporary must survive to scope exit"
        );
    }

    #[test]
    #[ignore = "tier:proof"]
    fn unused_scalar_locals_retain_bounded_storage() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(source, "let value_{index}: bool = identity(true);")
                    .expect("fixture source must be constructed");
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("unused scalar locals must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "unused scalar locals must not accumulate in later flow states"
            );
        }
    }

    #[test]
    #[ignore = "tier:proof"]
    fn unused_scalar_parameters_do_not_enter_flow_state() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from("module example;\nfunc caller(\n");

            for index in 0..count {
                writeln!(source, "pos value_{index}: bool,")
                    .expect("fixture source must be constructed");
            }

            source.push_str(
                r#") -> bool
                {
                    return true;
                }
                "#,
            );

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "caller");

            let storage = compilation
                .storage_plan(key.clone())
                .expect("parameters must plan");

            let parameters = storage
                .value()
                .identity_entries()
                .filter_map(|(id, identity)| {
                    matches!(identity, bray_bound_tree::StorageIdentity::Parameter(_)).then_some(id)
                })
                .collect::<Vec<_>>();

            let flow = compilation
                .storage_flow(key)
                .expect("unused parameters must check");

            assert_eq!(parameters.len(), count);
            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| parameters.iter().all(|id| !exit.live().contains(id))),
                "unused cleanup-free parameters must not multiply flow availability"
            );
        }
    }

    #[test]
    fn recurrent_parameter_uses_without_a_final_use_remain_available() {
        for body in [
            "loop\n{\nidentity(input);\n}\n",
            "loop\n{\nidentity(input);\ncontinue;\n}\n",
        ] {
            let source = format!(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {{
                    return input;
                }}

                func caller(pos input: bool)
                {{
                    {body}
                }}
                "#,
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");
        }
    }

    #[test]
    fn returned_parameter_borrows_preserve_owner_provenance() {
        let compilation = compilation(
            r#"
            module example;

            func caller(pos input: &bool) -> &bool
            {
                let local = input;

                return local;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn unused_owned_parameters_and_locals_preserve_cleanup_obligations() {
        let compilation = compilation(
            r#"
            module example;

            struct Guard
            {
                value: bool;
                destruct()
                {
                }
            }

            func caller(pos input: Guard)
            {
                let local = Guard
                {
                    value = true,
                };
            }
            "#,
        );

        let key = source_function_body_key(&compilation, "caller");

        let storage = compilation
            .storage_plan(key.clone())
            .expect("owned values must plan");

        let obligations = storage
            .value()
            .identity_entries()
            .filter_map(|(id, identity)| {
                matches!(
                    identity,
                    bray_bound_tree::StorageIdentity::Parameter(_)
                        | bray_bound_tree::StorageIdentity::LocalOwned(_)
                )
                .then_some(id)
            })
            .collect::<Vec<_>>();

        let flow = compilation
            .storage_flow(key)
            .expect("owned cleanup must check");

        assert_eq!(obligations.len(), 2);
        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

        assert!(
            flow.value().exits().iter().any(|exit| obligations
                .iter()
                .all(|id| exit.live().contains(id) && exit.initialized().contains(id))),
            "unused owned parameters and locals must survive until cleanup"
        );
    }

    #[test]
    #[ignore = "tier:proof"]
    fn optionally_used_scalar_locals_expire_on_skipped_paths() {
        for count in [64, 128, 256, 1024] {
            for short_circuit in [false, true] {
                let mut source = String::from(
                    r#"
                    module example;

                    func identity(pos input: bool) -> bool
                    {
                        return input;
                    }

                    func caller(pos condition: bool)
                    {
                    "#,
                );

                for index in 0..count {
                    writeln!(source, "let value_{index}: bool = identity(true);")
                        .expect("fixture source must be constructed");

                    if short_circuit {
                        writeln!(
                            source,
                            "let discarded_{index}: bool = condition && identity(value_{index});"
                        )
                        .expect("fixture source must be constructed");
                    } else {
                        writeln!(source, "if condition\n{{\nidentity(value_{index});\n}}")
                            .expect("fixture source must be constructed");
                    }
                }

                source.push_str("\n}\n");

                let compilation = compilation(&source);

                let flow = compilation
                    .storage_flow(source_function_body_key(&compilation, "caller"))
                    .expect("optional uses must check");

                assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

                assert!(
                    flow.value()
                        .exits()
                        .iter()
                        .all(|exit| exit.live().len() <= 8),
                    "skipped branches and short-circuit operands must not retain dead locals"
                );
            }
        }
    }

    #[test]
    #[ignore = "tier:proof"]
    fn failed_call_inputs_expire_on_caught_paths() {
        for count in [64, 128, 256] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    if input
                    {
                        panic("failed input");
                    }

                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(
                    source,
                    "let value_{index}: bool = true;\ncatch\n{{\nidentity(value_{index});\n}};"
                )
                .expect("fixture source must be constructed");
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "caller");

            let storage = compilation
                .storage_plan(key.clone())
                .expect("caught inputs must plan");

            let inputs = storage
                .value()
                .identity_entries()
                .filter_map(|(id, identity)| {
                    matches!(identity, bray_bound_tree::StorageIdentity::LocalOwned(_))
                        .then_some(id)
                })
                .collect::<Vec<_>>();

            let flow = compilation
                .storage_flow(key)
                .expect("caught calls must check");

            assert_eq!(inputs.len(), count);

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value().exits().iter().all(|exit| exit
                    .live()
                    .iter()
                    .filter(|id| inputs.contains(id))
                    .count()
                    <= 1),
                "call failure must retire dead scalar inputs without discarding owned catch results"
            );
        }
    }

    #[test]
    #[ignore = "tier:proof"]
    fn discarded_borrow_results_release_their_scalar_owners() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func echo(pos input: &bool) -> &bool
                {
                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(
                    source,
                    "let value_{index}: bool = true;\necho(&value_{index});"
                )
                .expect("fixture source must be constructed");
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("discarded borrow results must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "an expired borrow result must not retain a dead scalar owner"
            );
        }
    }

    #[test]
    fn retained_borrow_results_keep_storage_until_consumption() {
        let compilation = compilation(
            r#"
            module example;

            func echo(pos input: &bool) -> &bool
            {
                return input;
            }

            func observe(pos input: &bool) -> bool
            {
                return true;
            }

            func caller() -> bool
            {
                let value: bool = true;
                let retained = echo(&value);
                observe(&false);

                return observe(retained);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn exact_lifetime_retirement_preserves_escaping_borrow_diagnostics() {
        let compilation = compilation(
            r#"
            module example;

            func echo(pos input: &bool) -> &bool
            {
                return input;
            }

            func caller() -> &bool
            {
                let value: bool = true;

                return echo(&value);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency),
            "returning a local borrow must remain rejected: {diagnostics:?}"
        );
    }
}
