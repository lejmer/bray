use bray_ir::{MirCallTarget, MirOperationKind, MirTerminatorKind};

use crate::test_support::{compilation, source_function_body_key};

use super::support::lowered_mir;

#[test]
fn unproved_requirements_execute_before_declared_calls() {
    for (callee, invocation, capabilities) in [
        (
            r#"
            func selected(pos value: i32)
                requires(value > 0, value < 100)
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
                requires(value > 0, value < 100)
                uses(foreign_call);
        "#,
            "selected(input);",
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
            bray_base::NonEmptySharedStr::try_new("native")
                .expect("fixture non empty shared str must be valid"),
            bray_symbols::NativeLinkKind::Dynamic,
        )]);

        let compilation = crate::Compilation::load(crate::CompilationRequest::with_options(
            bray_symbols::PackageIdentity::try_new("example.package")
                .expect("fixture package identity must be valid"),
            vec![crate::test_support::source_input(&source, 0)],
            options,
        ))
        .expect("fixture input must load");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .expect("fixture callable must lower successfully");

        assert!(
            !result.diagnostics().has_errors(),
            "{:#?}",
            result.diagnostics()
        );

        let mir = lowered_mir(&result);

        assert_runtime_requirements_guard_invocation(mir, 2..=2, |target|
            matches!(target, MirCallTarget::Direct(_) | MirCallTarget::Indirect { .. }));
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
        .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

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
        .expect("fixture callable must lower successfully");

    assert!(
        !result.diagnostics().has_errors(),
        "{:#?}",
        result.diagnostics()
    );

    let mir = lowered_mir(&result);

    assert_runtime_requirements_guard_invocation(mir, 1..=1, |target|
        matches!(target, MirCallTarget::Indirect { .. }));
}

fn assert_runtime_requirements_guard_invocation(
    mir: &bray_ir::MirUnit,
    expected: impl std::ops::RangeBounds<usize>,
    invokes: impl Fn(&MirCallTarget) -> bool,
) {
    let operations = |block: bray_ir::MirBlockId| {
        mir.block(block).expect("MIR block exists").operations().iter()
            .map(|id| mir.operation(*id).expect("MIR block retains its operations").kind())
    };

    let invoked = mir.blocks_with_ids().filter_map(|(id, _)| {
        operations(id).any(|operation| matches!(operation, MirOperationKind::Call(call)
            if invokes(call.target()))).then_some(id)
    }).collect::<std::collections::BTreeSet<_>>();

    assert!(!invoked.is_empty(), "the checked call must be emitted");

    let raises_panic = |id| operations(id).any(|operation| matches!(operation, MirOperationKind::PanicReport(_)));

    let reachable = |start, stop_on_panic| bray_base::transitive_dependencies([start], |id| {
        let mut children = Vec::new();

        if stop_on_panic && raises_panic(*id) { return children; }

        mir.block(*id).expect("successor belongs to the unit").terminator().kind()
            .for_each_successor(|next| children.push(next));

        children
    });

    let mut checks = 0;

    for (id, block) in mir.blocks_with_ids() {
        let predicates = operations(id).filter(|operation| matches!(operation, MirOperationKind::Call(call)
            if matches!(call.target(), MirCallTarget::Predicate(_)))).count();

        if predicates == 0 { continue; }

        assert_eq!(predicates, 1, "each requirement has its own checked branch");
        checks += 1;

        let branch = match block.terminator().kind() {
            MirTerminatorKind::TakeCallReport { completed, .. }
            | MirTerminatorKind::CheckCallOutcome { completed, .. } =>
                mir.block(completed.target()).expect("predicate completion has a block").terminator().kind(),
            branch => branch,
        };

        let MirTerminatorKind::Branch { then_edge, else_edge, .. } = branch else {
            panic!("a runtime requirement must branch on its result: {branch:?}");
        };

        let failure = reachable(else_edge.target(), true);

        assert!(reachable(then_edge.target(), true).iter().any(|id| invoked.contains(id)),
            "precondition success at {id:?} must reach one of {invoked:?}");

        assert!(!failure.iter().any(|id| invoked.contains(id)),
            "precondition failure at {id:?} reached an invocation before raising its panic");

        assert!(failure.iter().copied().any(raises_panic), "{mir:?}");
    }

    assert!(expected.contains(&checks), "found {checks} requirement checks");
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
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_)))));
    }
}

#[test]
fn scoped_requirements_check_entry_and_exit() {
    for requirements in [
        ("requires(self.ready)", ""),
        ("", "requires(lease)"),
    ] {
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
                    return self.ready;
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

        let key = source_function_body_key(&compilation, "caller");
        let selections = compilation.semantic_selections(key.clone()).expect("with selects entry and exit");

        let selected = selections.value().entries().iter().find_map(|entry| {
            match entry.selection() {
                bray_bound_tree::SemanticSelection::ScopedUse(selected) => Some(selected),
                _ => None,
            }
        }).expect("the caller contains one with expression");

        let checked = if entry.is_empty() { selected.exit().0 } else { selected.enter().0 };

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&result);

        assert_runtime_requirements_guard_invocation(mir, 1.., |target| {
            matches!(target, MirCallTarget::Direct(reference) if reference.instance() == checked)
        });
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

    let selections = compilation
        .semantic_selections(key.clone())
        .expect("fixture callable must have semantic selections");

    let selected = selections
        .value()
        .entries()
        .iter()
        .find_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::Call(call) => Some(call),
            _ => None,
        })
        .expect("fixture selection must find its expected value");

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
        .expect("fixture `nth` call must succeed");

    assert_eq!(
        selected
            .phase_behaviors()
            .deferred_execution()
            .expect("fixture `deferred_execution` call must succeed")
            .may_unwind(),
        baseline
            .phase_behaviors()
            .deferred_execution()
            .expect("fixture `deferred_execution` call must succeed")
            .may_unwind()
    );

    let result = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

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
        bray_base::NonEmptySharedStr::try_new("native")
            .expect("fixture non empty shared str must be valid"),
        bray_symbols::NativeLinkKind::Dynamic,
    )]);

    let compilation = crate::Compilation::load(crate::CompilationRequest::with_options(
        bray_symbols::PackageIdentity::try_new("example.package")
            .expect("fixture package identity must be valid"),
        vec![crate::test_support::source_input(source, 0)],
        options,
    ))
    .expect("fixture input must load");

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
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&result);

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Predicate(_))))
    );
}
