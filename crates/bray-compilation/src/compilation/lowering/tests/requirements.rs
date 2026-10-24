use bray_ir::{MirCallTarget, MirOperationKind, MirTerminatorKind};

use crate::test_support::{compilation, source_function_body_key};

use super::support::lowered_mir;

#[test]
fn unproved_requirements_execute_before_declared_calls() {
    for (callee, invocation, capabilities) in [
        (
            r#"
            func selected(pos value: i32)
                requires(value > 0)
                executes(total)
            {
            }
        "#,
            "selected(input);",
            "",
        ),
        (
            r#"
            @link(name = "native")
            @symbol(name = "selected")
            @abi(c)
            extern trusted func selected(pos value: i32)
                requires(value > 0)
                uses(foreign_call);
        "#,
            "trusted selected(input);",
            "uses(foreign_call)",
        ),
    ] {
        let source = format!(
            r#"
                trusted module app;

                {callee}

                trusted func caller(pos input: i32)
                    {capabilities}
                {{
                    {invocation}
                }}
            "#
        );

        let options = crate::CompilationOptions::new(
            crate::WorkerBudget::default(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("native").unwrap(),
            bray_symbols::NativeLinkKind::Dynamic,
        )]);

        let compilation = crate::Compilation::load(crate::CompilationRequest::with_options(
            bray_symbols::PackageIdentity::try_new("example.package").unwrap(),
            vec![crate::test_support::source_input(&source, 0)],
            options,
        ))
        .unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        assert!(
            !result.diagnostics().has_errors(),
            "{:#?}",
            result.diagnostics()
        );

        let mir = lowered_mir(&result);

        let predicate = mir
            .operations_with_ids()
            .find_map(|(id, operation)| match operation.kind() {
                MirOperationKind::Call(call)
                    if matches!(call.target(), MirCallTarget::Predicate(_)) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .expect("unknown source input needs its runtime requirement");

        let invocation = mir
            .operations_with_ids()
            .find_map(|(id, operation)| match operation.kind() {
                MirOperationKind::Call(call)
                    if matches!(
                        call.target(),
                        MirCallTarget::Direct(_) | MirCallTarget::Indirect { .. }
                    ) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .expect("successful requirement retains the original invocation");

        assert!(predicate.slot() < invocation.slot());

        for block in mir.blocks().iter().filter(|block| {
            block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::PanicReport(_)
                )
            })
        }) {
            assert!(
                !matches!(block.terminator().kind(), MirTerminatorKind::Unreachable),
                "ordinary precondition failure remains reachable for {callee}"
            );
        }

        assert!(
            mir.blocks()
                .iter()
                .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
        );
    }
}

#[test]
fn established_requirements_do_not_emit_runtime_predicates() {
    let compilation = compilation(
        r#"
        module app;

        func selected(pos value: i32)
            requires(value > 0) {}

        func caller(pos input: i32)
            requires(input > 0)
        {
            selected(input);
        }
    "#,
    );

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    let mir = lowered_mir(&result);

    assert!(!mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
}

#[test]
fn execution_requirements_remain_compile_time_obligations() {
    let compilation = compilation(
        r#"
        module app;

        func selected()
            requires(blocking_execution()) {}

        func caller()
            requires(blocking_execution())
        {
            selected();
        }
    "#,
    );

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    let mir = lowered_mir(&result);

    assert!(!mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
}

#[test]
fn represented_indirect_requirements_execute_before_invocation() {
    let compilation = compilation(
        r#"
        module app;

        callable Action = func(pos value: i32)
            requires(value > 0);

        func caller(pos action: Action, pos input: i32)
        {
            action(input);
        }
    "#,
    );

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    assert!(
        !result.diagnostics().has_errors(),
        "{:#?}",
        result.diagnostics()
    );

    let mir = lowered_mir(&result);

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(bray_ir::MirPredicateReference::Condition { .. })))));
    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Indirect { .. }))));
}

#[test]
fn constructors_and_receivers_check_requirements() {
    for (declaration, parameters, invocation) in [
        (
            r#"
            struct Value
            {
                construct(pos input: i32) -> Self
                    requires(input > 0)
                {
                    return Value {};
                }
            }
        "#,
            "pos input: i32",
            "let value: Value = Value(input);",
        ),
        (
            r#"
            struct Value
            {
                ready: bool;
            }

            impl Value
            {
                func selected()
                    requires(self.ready)
                {
                }
            }
        "#,
            "pos value: Value",
            "value.selected();",
        ),
        (
            r#"
            struct Value
            {
                ready: bool;
            }

            impl Value
            {
                mut func selected()
                    requires(self.ready)
                {
                }
            }
        "#,
            "pos mut value: Value",
            "value.selected();",
        ),
        (
            r#"
            struct Value
            {
                ready: bool;
            }

            impl Value
            {
                consume func selected()
                    requires(self.ready)
                {
                }
            }
        "#,
            "pos value: Value",
            "value.selected();",
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            module app;

            {declaration}

            func caller({parameters})
            {{
                {invocation}
            }}
        "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
    }
}

#[test]
fn scoped_requirements_check_entry_and_exit() {
    for requirements in [("requires(self.ready)", ""), ("", "requires(lease)")] {
        let (entry, exit) = requirements;

        let compilation = compilation(&format!(
            r#"
            module app;

            struct Resource
            {{
                ready: bool;
            }}

            impl Resource
            {{
                enter() -> bool
                    {entry}
                {{
                    return true;
                }}

                exit(pos lease: bool)
                    {exit}
                {{
                }}
            }}

            func caller(pos resource: Resource) -> bool
            {{
                return with lease = resource
                {{
                    yield lease;
                }};
            }}
        "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
    }
}

#[test]
fn asynchronous_requirements_fail_before_frame_creation() {
    let compilation = compilation(
        r#"
        module app;

        async func selected(pos input: i32)
            requires(input > 0) {}

        async func baseline(pos input: i32) {}

        func caller(pos input: i32)
        {
            _ = selected(input);
            _ = baseline(input);
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "caller");
    let selections = compilation.semantic_selections(key.clone()).unwrap();

    let selected = selections
        .value()
        .entries()
        .iter()
        .find_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::Call(call) => Some(call),
            _ => None,
        })
        .unwrap();

    assert!(
        selected
            .phase_behaviors()
            .invocation()
            .has_runtime_requirements()
    );

    let baseline = selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::Call(call) => Some(call),
            _ => None,
        })
        .nth(1)
        .unwrap();

    assert_eq!(
        selected
            .phase_behaviors()
            .deferred_execution()
            .unwrap()
            .may_unwind(),
        baseline
            .phase_behaviors()
            .deferred_execution()
            .unwrap()
            .may_unwind()
    );

    let result = compilation.lowered_unit(key).unwrap();
    let mir = lowered_mir(&result);

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
}

#[test]
fn nested_argument_calls_keep_the_declared_requirement_type() {
    let source = r#"
        @link(name = "native")
        trusted module app;

        @symbol(name = "selected")
        @abi(c)
        extern trusted func selected(pos value: i32) -> i32
            requires(value > 0)
            uses(foreign_call);

        func identity(pos value: i32) -> i32
        {
            return value;
        }

        internal callable Action = @abi(c) trusted func(pos value: i32) -> i32
            requires(value > 0)
            uses(foreign_call);

        trusted func caller()
            uses(foreign_call)
        {
            let action: Action = selected;
            let value: i32 = trusted selected(identity(0));

            assert(trusted selected(identity(7)) == 7);
        }
    "#;

    let options = crate::CompilationOptions::new(
        crate::WorkerBudget::default(),
        bray_symbols::ProductKind::Library,
        crate::SelectedTarget::baseline(),
    )
    .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
        bray_base::NonEmptySharedStr::try_new("native").unwrap(),
        bray_symbols::NativeLinkKind::Dynamic,
    )]);

    let compilation = crate::Compilation::load(crate::CompilationRequest::with_options(
        bray_symbols::PackageIdentity::try_new("example.package").unwrap(),
        vec![crate::test_support::source_input(source, 0)],
        options,
    ))
    .unwrap();

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn caught_scope_exit_requirements_preserve_lifecycle_control_flow() {
    let compilation = compilation(
        r#"
        module app;

        struct Resource
        {
            ready: bool;
        }

        impl Resource
        {
            enter() -> bool
            {
                return self.ready;
            }

            exit(pos lease: bool)
                requires(lease) {}
        }

        func caller(pos resource: Resource) -> Result<bool, PanicReport>
        {
            return catch
            {
                yield with lease = resource
                {
                    yield lease;
                };
            };
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    let mir = lowered_mir(&result);

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_))))
    );
}
