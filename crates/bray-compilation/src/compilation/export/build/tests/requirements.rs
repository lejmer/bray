use super::fixtures::{compilation, export, implementation_dependency};

#[test]
fn imported_unproved_requirements_reach_concrete_predicate_helpers() {
    let provider = compilation(r#"
        module api;
        predicate above(value: i32, limit: i32) = value > limit;
        func selected<T>(pos value: i32, pos limit: i32, marker: &T)
            requires(above(value, limit), limit >= 0)
        { marker; }
    "#);
    let consumer = crate::test_support::compilation_with_dependencies(r#"
        module app;
        using example.package.api;
        func caller(pos value: i32, pos limit: i32) {
            let marker: bool = true;
            example.package.api.selected<bool>(value, limit, marker = &marker);
        }
        func main() { caller(2, 1); }
    "#, [implementation_dependency(export(&provider))]);
    assert!(!consumer.check_diagnostics().has_errors(), "{:#?}", consumer.check_diagnostics());
    let count = consumer.codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_))).expect("imported requirement helpers must reach concrete code generation");
    assert_eq!(count, 3);
}

#[test]
fn requirement_templates_preserve_named_argument_mapping() {
    let provider = compilation(r#"
        module api;
        const func order(pos first: i32, second: i32) -> bool executes(pure, total) { return first > second; }
        func selected(pos value: i32, pos limit: i32) requires(order(second = limit, first = value)) {}
    "#);
    let consumer = crate::test_support::compilation_with_dependencies(r#"
        module app;
        using example.package.api;
        func caller(pos value: i32, pos limit: i32) { example.package.api.selected(value, limit); }
        func main() { caller(2, 1); }
    "#, [implementation_dependency(export(&provider))]);
    assert!(!consumer.check_diagnostics().has_errors(), "{:#?}", consumer.check_diagnostics());
    let count = consumer.codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::Predicate(_))).expect("imported requirement helpers must reach concrete code generation");
    assert_eq!(count, 1);
}

#[test]
fn imported_completion_observes_the_mutated_receiver() {
    let provider = compilation(r#"
        module api;

        struct Resource
        {
            mut open: bool;

            predicate complete(value: &Resource) = !value.open;

            mut func close(pos fail: bool) -> Result<unit, bool>
                executes(total)
                ensures(result matches Error(_) || Resource.complete(value = &self))
            {
                if fail
                {
                    return Error(false);
                }

                self.open = false;

                return Ok(unit);
            }

            finalize() -> Result<unit, bool>
                when(Resource.complete(value = &self))
                {
                    executes(pure, total)
                    ensures(result matches Ok(_))
                }
            {
                if self.open
                {
                    return Error(false);
                }

                return Ok(unit);
            }
        }

        func require_ok<T, E>(pos input: Result<T, E>) -> T
            when(input matches Error(_))
            {
                ensures(false)
            }
        {
            match consume input
            {
                case Ok(value) { return value; }
                case Error(error) { panic("failed"); }
            }
        }
    "#);
    let dependency = implementation_dependency(export(&provider));

    for valid in [true, false] {
        let mutation = if valid { "" } else { "value.open = true;" };
        let source = format!(r#"
            module app;
            using example.package.api;

            func caller(pos fail: bool)
            {{
                let mut value: example.package.api.Resource = {{ open = true }};
                example.package.api.require_ok<unit, bool>(value.close(fail));
                {mutation}
            }}

            func main()
            {{
                caller(false);
            }}
        "#);
        let consumer = crate::test_support::compilation_with_dependencies(&source, [dependency.clone()]);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{diagnostics:?}");
            let instances = consumer.codegen_instance_count_for_test(|key| matches!(key, bray_ir::MirUnitKey::ImportedExecutable(_)));
            assert!(instances.is_ok(), "{instances:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
            );
        }
    }
}
