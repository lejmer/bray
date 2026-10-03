use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::{BoundUnitKey, CheckedBodySemantics, StoragePlan};
use bray_checker::{BodySemanticChecker, DefaultBodySemanticChecker};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::plan_storage;
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
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
                    expressions.result().value().types(),
                    patterns.result().value(),
                    expressions.result().value().selections(),
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

    pub(in crate::compilation) fn body_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedBodySemantics>>, FactQueryError> {
        self.unit_query(
            &self.state.body_semantics,
            CompilationFactKey::BodySemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let control_flow =
                    self.control_flow_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let unit = bray_checker::CheckerUnitView::new(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                );

                let result = checker_result(DefaultBodySemanticChecker.check_body_semantics(
                    unit,
                    control_flow.result().value(),
                    expressions.result().value(),
                    patterns.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                ))?;

                let (semantics, semantic_diagnostics) = result.into_parts();

                Ok((
                    DiagnosticResult::new(semantics, semantic_diagnostics),
                    Box::new([]),
                ))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use crate::test_support::{compilation, source_function_body_key};

    #[test]
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
                writeln!(source, "value = identity(value);").unwrap();
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
}
