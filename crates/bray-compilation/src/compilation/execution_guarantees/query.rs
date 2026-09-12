use std::sync::Arc;

use bray_bound_tree::BoundUnitKey;
use bray_checker::{
    ExecutionCertification, ExecutionDeclaration, ExecutionProperty, check_execution_candidate,
    declared_execution_properties,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::checker::checker_result;
use crate::compilation::unit::{checker_unit_view, semantic_unit_context_for};
use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(super) fn execution_declaration(
        &self,
        anchor: SyntaxAnchor,
    ) -> Result<DiagnosticResult<ExecutionDeclaration>, FactQueryError> {
        Ok(declared_execution_properties(
            self.execution_declaration_node(anchor)?,
        ))
    }

    pub(super) fn execution_declaration_node(
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

    pub(super) fn execution_candidates_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<bray_checker::ExecutionCandidates>>, FactQueryError> {
        // Fact keys and their immutable publications retain shared unit identities independently.
        self.unit_query(
            &self.state.execution_candidates,
            CompilationFactKey::ExecutionCandidates(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                // Each queried publication owns this Arc-backed unit identity.
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let body = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit = checker_unit_view(bound.result().value(), &semantic_context, &context)?;

                let mut diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    expressions.result().diagnostics(),
                    storage.result().diagnostics(),
                    body.result().diagnostics(),
                    memory.result().diagnostics(),
                    flow.result().diagnostics(),
                ]);

                let declaration = self.execution_declaration(key.source().syntax())?;

                let requirements = self.execution_condition_inputs(
                    &key,
                    declaration.value().requirements(),
                    cancellation,
                )?;

                diagnostics.add_range(requirements.diagnostics().iter().cloned());

                let requirements = requirements
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition)
                    .collect::<Vec<_>>();

                let contracts = self.execution_call_contracts(
                    bound.result().value(),
                    expressions.result().value(),
                    cancellation,
                )?;

                diagnostics.add_range(contracts.diagnostics().iter().cloned());

                let check = |property,
                             assumptions: &[bray_checker::ExecutionCondition],
                             postconditions: &[(
                    bray_checker::ExecutionCondition,
                    bray_source::SourceSpan,
                )]| {
                    checker_result(check_execution_candidate(
                        unit,
                        property,
                        assumptions,
                        postconditions,
                        contracts.value(),
                        expressions.result().value(),
                        storage.result().value(),
                        body.result().value(),
                        memory.result().value(),
                    ))
                };

                let mut candidates = bray_checker::ExecutionCandidates::new();

                for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
                    let candidate = check(Some(property), &requirements, &[])?;
                    diagnostics.add_range(candidate.diagnostics().iter().cloned());

                    candidates.insert(
                        bray_checker::ExecutionObligation::Property(property, None),
                        candidate.into_parts().0,
                    );
                }

                for domain in declaration.value().domains() {
                    let guards =
                        self.execution_condition_inputs(&key, &domain.guards, cancellation)?;

                    let posts = self.execution_condition_inputs(
                        &key,
                        &domain.postconditions,
                        cancellation,
                    )?;

                    diagnostics.add_range(guards.diagnostics().iter().cloned());
                    diagnostics.add_range(posts.diagnostics().iter().cloned());

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
                            let candidate = check(Some(property.property), &assumptions, &[])?;
                            diagnostics.add_range(candidate.diagnostics().iter().cloned());

                            candidates.insert(
                                bray_checker::ExecutionObligation::Property(
                                    property.property,
                                    Some(property.source),
                                ),
                                candidate.into_parts().0,
                            );
                        }
                    }

                    for source in posts
                        .value()
                        .iter()
                        .map(|(_, source)| *source)
                        .collect::<std::collections::BTreeSet<_>>()
                    {
                        // Each clause proof owns shared immutable condition terms from this normalized domain.
                        let conditions = posts
                            .value()
                            .iter()
                            .filter(|(_, span)| *span == source)
                            .cloned()
                            .collect::<Vec<_>>();

                        let candidate = check(None, &assumptions, &conditions)?;
                        diagnostics.add_range(candidate.diagnostics().iter().cloned());

                        candidates.insert(
                            bray_checker::ExecutionObligation::Postcondition(source),
                            candidate.into_parts().0,
                        );
                    }
                }

                Ok((DiagnosticResult::new(candidates, diagnostics), Box::new([])))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};
    use bray_checker::ExecutionProperty::{Pure, Total};
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn certified_dependencies_preserve_selected_generic_arguments() {
        let compilation = compilation(
            r#"
            module app;
            func helper<T>() -> bool executes(total) { return true; }
            func root() -> bool executes(total) { return helper<bool>(); }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let proof = compilation
            .execution_properties(source_function_body_key(&compilation, "root"))
            .unwrap();

        let values = compilation.semantic_value_store().unwrap();

        assert!(
            proof
                .value()
                .dependencies
                .iter()
                .any(|(_, target, obligation)| {
                    let bray_bound_tree::BoundCallableTarget::Declaration(instance) = target else {
                        return false;
                    };
                    *obligation == bray_checker::ExecutionObligation::Property(Total, None)
                        && values
                            .generic_substitution_data(instance.substitution())
                            .unwrap()
                            .bindings()
                            .len()
                            == 1
                })
        );
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
                .unwrap();

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

                func change(pos mut value: bool)
                    executes(pure)
                {
                    value = false;
                }
            "#,
            r#"
                trusted module app;

                trusted func change(pos mut value: bool)
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

        let compilation = compilation(
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
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingCircularExecutionGuarantee,
        );

        let proof = compilation
            .execution_properties(source_function_body_key(&compilation, "first"))
            .unwrap();

        assert_eq!(
            &proof.value().properties,
            &std::collections::BTreeSet::from([Pure])
        );
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
                    finalize()
                        executes(total) {}
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
                    .unwrap();

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
                    .unwrap();

                assert!(proof.value().properties.is_empty(), "{source}: {proof:?}");
            }
        }
    }

    #[test]
    fn opaque_foreign_assertions_retain_their_provenance() {
        let options = crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c").unwrap(),
            bray_symbols::NativeLinkKind::Dynamic,
        )]);

        let compilation = crate::test_support::compilation_with_options(
            r#"
                trusted module app;

                @link(name = "c")
                @symbol(name = "native_value")
                @abi(c)
                extern trusted func native_value() -> i32
                    uses(foreign_call)
                    executes(total);

                trusted func wrapper() -> i32
                    uses(foreign_call)
                    executes(total)
                {
                    return native_value();
                }
            "#,
            options,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let evidence = compilation
            .execution_properties(source_function_body_key(&compilation, "wrapper"))
            .unwrap();

        assert_eq!(
            evidence.value().properties,
            std::collections::BTreeSet::from([Total])
        );

        assert_eq!(evidence.value().foreign_assertions.len(), 1);
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
        let first = original.execution_properties(key.clone()).unwrap();

        std::thread::scope(|scope| {
            let requests = (0..4)
                .map(|_| scope.spawn(|| original.execution_properties(key.clone()).unwrap()))
                .collect::<Vec<_>>();

            for request in requests {
                assert!(std::sync::Arc::ptr_eq(&first, &request.join().unwrap()));
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
            .unwrap();

        let proof = updated
            .execution_properties(source_function_body_key(&updated, "identity"))
            .unwrap();

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
            .unwrap();

        assert_eq!(
            proof.value().properties,
            std::collections::BTreeSet::from([Total])
        );
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
                        executes(total)
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
            .unwrap();

        assert!(proof.value().properties.is_empty());
    }
}
