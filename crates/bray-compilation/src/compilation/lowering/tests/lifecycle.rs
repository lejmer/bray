use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_ir::{MirOperationKind, MirTerminatorKind};

use super::support::lowered_mir;
use crate::test_support::{
    compilation, source_function_body_key, source_type_callable_member_body_key,
};

#[test]
fn empty_construction_skips_admission_but_preserves_implicit_calls() {
    for (declaration, initializer, admission, outcome_check) in [
        (
            "struct Value { payload: u64; }",
            "Value { payload = 1 }",
            false,
            false,
        ),
        (
            "struct Value { payload: u64 = risky(); }",
            "Value {}",
            false,
            true,
        ),
        (
            "struct Value { construct() -> Self { risky(); return Value {}; } }",
            "Value()",
            false,
            true,
        ),
        (
            "struct Value { destruct() { risky(); } }",
            "Value {}",
            true,
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            {declaration}
            extern func risky() -> u64;
            func caller() -> Value {{ return {initializer}; }}
        "#
        );

        let compilation = compilation(&source);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{source}: {lowered:#?}"
        );

        let mir = lowered_mir(&lowered);

        assert_eq!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::AdmitOutgoing { .. }
            )),
            admission,
            "{source}"
        );

        assert_eq!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::CheckCallOutcome { .. }
            )),
            outcome_check,
            "{source}"
        );
    }
}

#[test]
fn empty_adapted_constructions_keep_checker_and_lowering_exit_plans_aligned() {
    for (result, body) in [
        (
            "Value?",
            "let value: Value = { payload = 1 }; return value;",
        ),
        ("Result<Value, u32>", "return Ok(Value { payload = 1 });"),
        ("Result<Value, u32>", "return Error(1);"),
        ("Value", "let other = Value { payload = 1 }; return other;"),
    ] {
        let compilation = compilation(&format!(
            "module app; struct Value {{ payload: u64; }} func caller() -> {result} {{ {body} }}"
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{result} {body}: {lowered:#?}"
        );
    }
}

#[test]
fn unit_valued_replacement_retains_old_owner_cleanup_exits() {
    let compilation = compilation(
        r#"
        module app;
        struct Resource { destruct() {} }
        struct Container { mut value: Resource?; }
        func replace(pos owner: &mut Container, pos value: Resource) { owner.value = value; }
    "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "replace"))
        .unwrap();

    assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

    assert!(lowered_mir(&lowered).blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::CheckCallOutcome { .. }
    )));
}

#[test]
fn scoped_invocations_retain_distinct_inputs_and_clear_activity_before_exit() {
    for body in ["yield lease;", "return lease;", "panic(\"body failed\");"] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Resource {{}}
            impl Resource
            {{
                enter() -> bool {{ return true; }}
                exit(pos lease: bool) {{}}
            }}
            func caller(pos resource: Resource) -> bool
            {{
                return with lease = resource {{ {body} }};
            }}
        "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");
        let selections = compilation.semantic_selections(key.clone()).unwrap();

        let scoped = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
                _ => None,
            })
            .unwrap();

        let result = compilation.lowered_unit(key).unwrap();
        let mir = lowered_mir(&result);
        let mut entered = false;
        let mut exited = false;

        for block in mir.blocks() {
            for (position, id) in block.operations().iter().enumerate() {
                let MirOperationKind::Call(call) = mir.operation(*id).unwrap().kind() else {
                    continue;
                };

                let bray_ir::MirCallTarget::Direct(callable) = call.target() else {
                    continue;
                };

                if callable.instance() == scoped.enter().0 {
                    entered = true;

                    assert!(matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::CheckCallOutcome { .. }
                    ));
                }

                if callable.instance() == scoped.exit().0 {
                    exited = true;

                    assert!(call.is_cleanup());

                    assert!(
                        block.operations()[..position].iter().any(|id| matches!(
                            mir.operation(*id).unwrap().kind(),
                            MirOperationKind::Store {
                                value: bray_ir::MirOperand::Immediate {
                                    value: bray_ir::MirImmediateValue::Boolean(false),
                                    ..
                                },
                                ..
                            }
                        )),
                        "exit clears its active guard before invoking user code"
                    );
                }
            }
        }

        assert!(
            entered && exited,
            "scoped use invokes both selected declarations"
        );
    }
}

#[test]
fn borrowed_scoped_capability_retains_its_receiver_storage_through_exit() {
    let compilation = compilation(
        r#"
        module app;
        struct Resource { ready: bool; }
        impl Resource
        {
            enter() -> &Self { return &self; }
            exit(pos lease: &Self) {}
        }
        func caller(pos resource: Resource) -> bool
        {
            return with lease = resource { yield lease.ready; };
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "caller");
    let storage = compilation.storage_plan(key.clone()).unwrap();

    let dependencies = compilation.dependency_contracts(key.clone()).unwrap();

    let (entry, _) = dependencies
        .value()
        .calls()
        .iter()
        .find(|(site, _)| matches!(site, bray_bound_tree::BoundExecutionSite::ScopedEnter(_)))
        .unwrap();

    let expression = entry.expression().unwrap();

    let (_, capability) = storage
        .value()
        .borrow_capability_entries()
        .find(|(_, loan)| loan.expression() == Some(expression))
        .expect("entry creates its receiver loan");

    let receiver = storage.value().root_identity(capability.access()).unwrap();

    assert!(matches!(
        storage.value().identity(receiver),
        Some(bray_bound_tree::StorageIdentity::Parameter(_))
    ));

    let liveness = compilation.liveness(key.clone()).unwrap();

    assert!(
        liveness.value().is_last_use(
            bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
            bray_bound_tree::BoundDependencySubject::Storage(receiver)
        ),
        "{:#?}",
        liveness.value()
    );

    let result = compilation.lowered_unit(key).unwrap();

    lowered_mir(&result);
}

#[test]
fn asynchronous_scoped_phases_drive_selected_frames_and_shield_exit() {
    for (enter, exit) in [("async ", ""), ("", "async "), ("async ", "async ")] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Resource {{}}
            impl Resource
            {{
                {enter}enter() -> bool {{ return true; }}
                {exit}exit(pos lease: bool) {{}}
            }}
            async func caller(pos resource: Resource) -> bool
            {{
                return with lease = resource {{ yield lease; }};
            }}
        "#,
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");
        let checked = compilation.async_analysis(key.clone()).unwrap();

        let phases = checked
            .value()
            .suspensions()
            .iter()
            .filter(|suspension| {
                suspension.kind() == bray_bound_tree::AsyncSuspensionKind::ScopedCall
            })
            .map(|suspension| suspension.occurrence())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            phases.len(),
            usize::from(!enter.is_empty()) + usize::from(!exit.is_empty())
        );

        let result = compilation.lowered_unit(key).unwrap();
        let mir = lowered_mir(&result);

        assert!(
            mir.is_valid(),
            "scoped cleanup must forward its values through MIR edges"
        );

        assert!(
            mir.blocks().iter().any(|block| matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. }
            ))
        );

        if !exit.is_empty() {
            assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Runtime(runtime)
                    if runtime.role() == bray_runtime_interface::RuntimeAbiRole::CleanupShieldEnter))));
        }
    }
}

#[test]
fn fallible_scoped_phases_separate_the_capability_and_body_result_types() {
    for (enter, exit) in [
        (
            "enter() -> Result<bool, i32> { return Ok(true); }",
            "exit(pos lease: bool) {}",
        ),
        (
            "enter() -> bool { return true; }",
            "exit(pos lease: bool) -> Result<unit, i32> { return Ok(unit); }",
        ),
        (
            "enter() -> Result<bool, i32> { return Error(7); }",
            "exit(pos lease: bool) -> Result<unit, i32> { return Error(8); }",
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Resource {{}}
            impl Resource {{ {enter} {exit} }}
            func caller(pos resource: Resource) -> Result<bool, i32>
            {{
                return with lease: bool = resource {{ yield lease; }};
            }}
        "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");
        let selections = compilation.semantic_selections(key.clone()).unwrap();

        let scoped = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
                _ => None,
            })
            .unwrap();

        let types = compilation.expression_types(key.clone()).unwrap();

        assert!(scoped.failure_type().is_some());

        assert_ne!(
            types.value().expression(scoped.expression()).unwrap().ty(),
            scoped.capability_type()
        );

        let result = compilation.lowered_unit(key).unwrap();

        assert!(
            lowered_mir(&result).is_valid(),
            "fallible scoped cleanup retains valid value ownership"
        );
    }
}

#[test]
fn fallible_scoped_exit_preserves_pending_control_outcomes() {
    for body in ["return Ok(lease);", "panic(\"pending panic\");"] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Resource {{}}
            impl Resource
            {{
                enter() -> Result<bool, i32> {{ return Ok(true); }}
                exit(pos lease: bool) -> Result<unit, i32> {{ return Error(9); }}
            }}
            func caller(pos resource: Resource) -> Result<bool, i32>
            {{
                return with lease = resource {{ {body} }};
            }}
            "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");
        let result = compilation.lowered_unit(key).unwrap();
        let mir = lowered_mir(&result);

        assert!(
            mir.is_valid(),
            "scoped cleanup must forward its values through MIR edges"
        );

        assert!(
            mir.blocks()
                .iter()
                .flat_map(|block| block.operations())
                .any(|id| matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::TransferCleanupIncident { .. }
                    )
                ))
        );
    }
}

#[test]
fn destructor_receiver_move_admits_before_transfer() {
    let compilation = compilation(
        r#"
            module app;
            struct Value {
                mut again: bool;
                destruct() {
                    if self.again {
                        self.again = false;
                        take(self);
                        self = Value { again = false };
                        take(self);
                    }
                }
            }
            func take(pos value: Value) {}
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .unwrap();

    let result = compilation.lowered_unit(key).unwrap();
    let mir = lowered_mir(&result);

    let admission = mir
        .blocks()
        .iter()
        .find(|block| {
            block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::AdmitOutgoing { .. }
                )
            })
        })
        .expect("whole receiver transfer must be admitted");

    assert!(matches!(
        admission.terminator().kind(),
        MirTerminatorKind::CheckCallOutcome { .. }
    ));

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(operation.kind(), MirOperationKind::AdmitOutgoing { .. }))
            .count(),
        3
    );

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::DischargeOutgoing { .. }
            ))
            .count(),
        1
    );
}

#[test]
fn default_failure_keeps_initialized_inputs_in_cleanup() {
    for consumer in [
        r#"
                struct Value { guard: Guard; number: i32 = rejected_default(); }
                func main() { let value = Value { guard = Guard {} }; }
            "#,
        r#"
                func accept_guard(pos guard: Guard, number: i32 = rejected_default()) {}
                func main() { accept_guard(Guard {}); }
            "#,
        r#"
                async func accept_guard(pos guard: Guard, number: i32 = rejected_default()) {}
                func main() {
                    let result = catch { let pending = accept_guard(Guard {}); yield unit; };
                }
            "#,
    ] {
        let source = format!(
            r#"
                module app;
                struct Guard {{ destruct() {{}} }}
                func rejected_default() -> i32 {{ panic("default failed"); }}
                {consumer}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "main");

        let result = compilation
            .lowered_unit(key)
            .expect("construction failure must have an executable cleanup path");

        let mir = lowered_mir(&result);

        let (block, edge) = mir.blocks().iter().find_map(|block| {
        let operation = block.operations().last().and_then(|id| mir.operation(*id))?;

        if !matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::DefaultValue { .. })) { return None; }

        let MirTerminatorKind::CheckCallOutcome { panicked, .. } = block.terminator().kind() else { return None; };

        Some((block, panicked.clone()))
    }).expect("default must retain a failure edge before construction");

        assert!(!block.operations().iter().any(|id| matches!(mir.operation(*id).unwrap().kind(), MirOperationKind::Construct(construction) if construction.inputs().len() == 2)));

        let mut pending = vec![edge.target()];
        let mut seen = BTreeSet::new();
        let mut cleans_input = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            let block = mir.block(id).unwrap();

            cleans_input |= block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
                )
            });

            block
                .terminator()
                .kind()
                .for_each_successor(|target| pending.push(target));
        }

        assert!(
            cleans_input,
            "default failure must reach lifecycle cleanup for the initialized guard"
        );
    }
}

#[test]
fn generic_union_returns_preserve_the_callable_result_through_cleanup() {
    let compilation = compilation(
        r#"module app;

union Choice<T>
{
    Value(pos value: T);
    Empty;
}

struct Guard
{
    destruct() {}
}

struct Receiver<T>
{
    async func receive(pos value: T) -> Choice<T>
    {
        let guard: Guard = Guard {};

        return Choice.Empty;
    }
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_type_callable_member_body_key(
            &compilation,
            "receive",
        ))
        .unwrap_or_else(|error| panic!("generic union return must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{lowered:#?}");
}

#[test]
fn cleanup_materializes_every_verified_lifecycle_storage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource\n",
        "{\n",
        "    destruct()\n",
        "    {\n",
        "    }\n",
        "}\n",
        "union OpenError\n",
        "{\n",
        "    Failed;\n",
        "}\n",
        "func open() -> Result<Resource, OpenError>\n",
        "{\n",
        "    return Ok(Resource {});\n",
        "}\n",
        "func read(pos text: &string) -> bool\n",
        "{\n",
        "    return true;\n",
        "}\n",
        "func main() -> Result<unit, OpenError>\n",
        "{\n",
        "    let resource: Resource = try open();\n",
        "    let observed: bool = read(&\"borrowed\");\n",
        "    return Ok(unit);\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "main");
    let storage = compilation.storage_plan(key.clone()).unwrap();
    let analysis = compilation.async_analysis(key.clone()).unwrap();

    let required = analysis
        .value()
        .scope_exits()
        .iter()
        .flat_map(|exit| {
            exit.cancellation_broadcast()
                .iter()
                .chain(exit.lifecycle_resolution())
        })
        .filter_map(|access| storage.value().root_identity(*access))
        .collect::<BTreeSet<_>>();

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("resource cleanup must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let cleanup_places = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Cleanup { place, .. } => Some((place.storage(), place.ty())),
            _ => None,
        })
        .collect::<Vec<_>>();

    let cleanup_storages = cleanup_places
        .iter()
        .filter(|(storage, _)| {
            lowered_mir(&lowered).storage(*storage).unwrap().kind()
                != &bray_ir::MirStorageKind::Return
        })
        .map(|(storage, _)| *storage)
        .collect::<BTreeSet<_>>();

    assert_eq!(cleanup_storages.len(), required.len(), "{cleanup_places:?}");
}

#[test]
fn frame_admission_failure_retains_initialized_capture_cleanup() {
    for creation in [
        "async func consume_guard(pos guard: Guard) {} func create(pos guard: Guard) { let outcome = catch { let pending = consume_guard(guard); yield unit; }; }",
        "callable Worker = async func(pos guard: Guard); func create(pos guard: Guard, pos worker: Worker) { let outcome = catch { let pending = worker(guard); yield unit; }; }",
    ] {
        assert_frame_admission_retains_capture(&format!(
            "module app; struct Guard {{ destruct() {{}} }} {creation}"
        ));
    }
}

fn assert_frame_admission_retains_capture(source: &str) {
    let compilation = compilation(source);

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "create"))
        .unwrap();

    let mir = lowered_mir(&lowered);

    let admission = mir
        .blocks()
        .iter()
        .find(|block| {
            block.operations().last().is_some_and(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::AdmitOutgoing {
                        owner: bray_ir::MirOutgoingOwner::Invocation { .. }
                            | bray_ir::MirOutgoingOwner::FrameFactory(_),
                        ..
                    }
                )
            })
        })
        .expect("frame backing is secured before capture transfer");

    if let MirOperationKind::AdmitOutgoing {
        owner: bray_ir::MirOutgoingOwner::FrameFactory(callee),
        ..
    } = mir
        .operation(*admission.operations().last().unwrap())
        .unwrap()
        .kind()
    {
        let bray_ir::MirOperand::Move(factory) = callee else {
            panic!("indirect admission retains the evaluated factory");
        };

        assert!(matches!(
            mir.storage(factory.storage()).unwrap().kind(),
            bray_ir::MirStorageKind::Temporary
        ));

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Copy(source), .. }
                if destination == factory && matches!(mir.storage(source.storage()).unwrap().kind(), bray_ir::MirStorageKind::Parameter(_))
        )), "factory selection is captured before admission can reenter");
    }

    let MirTerminatorKind::CheckCallOutcome {
        completed,
        panicked,
        ..
    } = admission.terminator().kind()
    else {
        panic!("admission requires a cleanup edge");
    };

    let capture = mir
        .block(completed.target())
        .unwrap()
        .operations()
        .iter()
        .find_map(|id| {
            let MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(call),
                ..
            }) = mir.operation(*id).unwrap().kind()
            else {
                return None;
            };

            let bray_ir::MirOperand::Move(place) = call.arguments()[0].value() else {
                panic!("constructor moves the guarded input");
            };

            Some(place)
        })
        .expect("only admitted construction transfers captures");

    let mut pending = vec![panicked.target()];
    let mut seen = BTreeSet::new();
    let mut resolves_capture = false;

    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }

        let block = mir.block(id).unwrap();

        for id in block.operations() {
            let kind = mir.operation(*id).unwrap().kind();

            assert!(
                !matches!(
                    kind,
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
                ),
                "failed admission never constructs a frame"
            );

            resolves_capture |= matches!(kind, MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution, place
            } if place == capture);
        }

        block
            .terminator()
            .kind()
            .for_each_successor(|target| pending.push(target));
    }

    assert!(
        resolves_capture,
        "failed admission resolves the exact retained input"
    );
}

#[test]
fn inactive_frame_cancellation_cleans_captures_without_entering_body() {
    let compilation = compilation(
        r#"
        module app;
        struct Guard { destruct() {} }
        func entered() {}
        async func consume_guard(pos guard: Guard) { entered(); }
    "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "consume_guard"))
        .unwrap();

    let mir = lowered_mir(&lowered);

    let entry = mir
        .block(mir.frame_descriptor().unwrap().states()[0].entry())
        .unwrap();

    let MirTerminatorKind::Branch {
        condition,
        then_edge,
        else_edge,
    } = entry.terminator().kind()
    else {
        panic!("initial frame state selects cancellation before source execution");
    };

    assert!(entry.operations().iter().any(|id| {
        let operation = mir.operation(*id).unwrap();
        matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)
        ) && operation.result().map(bray_ir::MirOperand::Value).as_ref() == Some(condition)
    }));

    let mut pending = vec![then_edge.target()];
    let mut seen = BTreeSet::new();
    let mut cleans_capture = false;

    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }

        assert_ne!(
            id,
            else_edge.target(),
            "cancellation never enters the source body"
        );

        let block = mir.block(id).unwrap();

        for id in block.operations() {
            let kind = mir.operation(*id).unwrap().kind();

            if let MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                place,
            } = kind
            {
                cleans_capture |= matches!(
                    mir.storage(place.storage()).unwrap().kind(),
                    bray_ir::MirStorageKind::Parameter(_)
                );
            }
        }

        block
            .terminator()
            .kind()
            .for_each_successor(|target| pending.push(target));
    }

    assert!(
        cleans_capture,
        "inactive cancellation resolves initialized captures"
    );
}

#[test]
fn frame_broadcast_visits_captures_during_shielded_scoped_exit() {
    let compilation = compilation(
        r#"
        module app;
        struct Resource {}
        impl Resource {
            enter() -> bool { return true; }
            async exit(pos lease: bool) {}
        }
        async func caller(pos task: Task<unit>, pos resource: Resource) -> bool {
            return with lease = resource { yield lease; };
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "caller");
    let checked = compilation.async_analysis(key.clone()).unwrap();

    let exit = checked
        .value()
        .suspensions()
        .iter()
        .find(|suspension| {
            matches!(
                suspension.occurrence(),
                bray_bound_tree::BoundExecutionSite::ScopedExit(_)
            )
        })
        .unwrap();

    assert!(!exit.cancellation_broadcast().is_empty());

    let lowered = compilation.lowered_unit(key).unwrap();
    let mir = lowered_mir(&lowered);

    assert!(
        mir.is_valid(),
        "broadcast callbacks must satisfy MIR contracts"
    );

    let frame = mir.frame_descriptor().unwrap();

    assert!(frame.states().len() > 1);

    let mut shielded = false;

    for block in mir.blocks() {
        let MirTerminatorKind::Suspend {
            resume_state,
            cancellation,
            ..
        } = block.terminator().kind()
        else {
            continue;
        };

        if matches!(
            mir.block(cancellation.edge().target())
                .unwrap()
                .terminator()
                .kind(),
            MirTerminatorKind::Unreachable
        ) {
            shielded = true;

            let entry = mir
                .block(frame.states()[usize::try_from(resume_state.raw()).unwrap()].entry())
                .unwrap();

            assert!(
                !entry.operations().iter().any(|operation| matches!(
                    mir.operation(*operation).unwrap().kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)
                )),
                "a shielded exit resumes its pending continuation even if cancellation started cleanup"
            );
        }
    }

    assert!(shielded);

    for state in frame.states() {
        let mut pending = vec![state.broadcast()];
        let mut seen = BTreeSet::new();
        let mut broadcasts_task = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            assert_ne!(id, state.entry(), "broadcast does not resume the frame");

            let block = mir.block(id).unwrap();

            assert!(!matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. } | MirTerminatorKind::Unreachable
            ));

            for id in block.operations() {
                if let MirOperationKind::Cleanup { phase, place } =
                    mir.operation(*id).unwrap().kind()
                {
                    assert_eq!(*phase, bray_ir::MirCleanupPhase::TaskCancellation);

                    broadcasts_task |= matches!(
                        mir.storage(place.storage()).unwrap().kind(),
                        bray_ir::MirStorageKind::Parameter(0)
                    );
                }

                if let MirOperationKind::Store { destination, .. } =
                    mir.operation(*id).unwrap().kind()
                {
                    assert!(
                        matches!(
                            mir.storage(destination.storage()).unwrap().kind(),
                            bray_ir::MirStorageKind::BroadcastLocal
                        ),
                        "broadcast preserves the suspended frame's pending state"
                    );
                }
            }

            block
                .terminator()
                .kind()
                .for_each_successor(|next| pending.push(next));
        }

        assert!(
            broadcasts_task,
            "state {} must broadcast its retained task",
            state.state().raw()
        );
    }
}

#[test]
fn frames_without_cancellation_obligations_have_empty_broadcasts() {
    let compilation = compilation("module app; async func caller() -> bool { return true; }");

    assert!(!compilation.check_diagnostics().has_errors());

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .unwrap();

    let mir = lowered_mir(&lowered);

    assert!(mir.is_valid());

    for state in mir.frame_descriptor().unwrap().states() {
        let block = mir.block(state.broadcast()).unwrap();

        assert!(block.operations().is_empty());

        assert!(matches!(
            block.terminator().kind(),
            MirTerminatorKind::Return(None)
        ));
    }

    assert!(
        !mir.storages()
            .iter()
            .any(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::BroadcastLocal))
    );
}

#[test]
fn ordinary_async_finalizers_suspend_with_live_owner_guards() {
    let compilation = compilation(
        r#"
        module app;

        extern async func pause();

        async func caller(pos owner: Owner) -> u32
        {
            return 7;
        }

        struct Owner
        {
            mut pending: Task<unit>;

            async finalize()
            {
                self.pending = pause().start();
                await pause();
            }
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "caller");
    let checked = compilation.async_analysis(key.clone()).unwrap();

    assert!(
        checked
            .value()
            .suspensions()
            .iter()
            .any(|point| point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup)
    );

    let selected = checked
        .value()
        .cleanup_types()
        .iter()
        .filter_map(|ty| ty.finalizer())
        .find(|call| matches!(call.result, bray_bound_tree::BoundCallResult::LazyFuture(_)))
        .expect("cleanup checking retains the selected async finalizer");

    let lowered = compilation.lowered_unit(key).unwrap();
    let mir = lowered_mir(&lowered);

    assert!(
        mir.is_valid(),
        "ordinary async cleanup must obey MIR ownership and suspension contracts"
    );

    let invocation = mir
        .operations()
        .iter()
        .find_map(|operation| match operation.kind() {
            MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(call),
                ..
            }) if matches!(call.target(), bray_ir::MirCallTarget::Direct(reference)
            if reference.instance() == selected.callable) =>
            {
                Some(call.result())
            }
            _ => None,
        })
        .expect("lowering constructs the selected finalizer's inactive frame");

    assert_eq!(invocation, selected.result);

    let awaited = mir
        .operations()
        .iter()
        .filter(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )
        })
        .count();

    assert!(
        awaited > 0,
        "async finalization must drive its child in the enclosing frame"
    );

    assert_eq!(
        awaited,
        mir.blocks()
            .iter()
            .filter(|block| matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. }))
            .count()
    );

    let mut destruction_guards = BTreeSet::new();

    for block in mir.blocks() {
        if block.operations().iter().any(|id| {
            matches!(
                mir.operation(*id).unwrap().kind(),
                MirOperationKind::Destroy { .. }
            )
        }) {
            for id in block.operations() {
                if let MirOperationKind::Store {
                    destination,
                    value:
                        bray_ir::MirOperand::Immediate {
                            value: bray_ir::MirImmediateValue::Boolean(false),
                            ..
                        },
                    ..
                } = mir.operation(*id).unwrap().kind()
                {
                    destruction_guards.insert(destination.storage());
                }
            }
        }
    }

    assert!(!destruction_guards.is_empty());

    for operation in mir.operations() {
        let MirOperationKind::Destroy {
            place,
            abandoned: Some(_),
        } = operation.kind()
        else {
            continue;
        };

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(),
            MirOperationKind::Quiesce(current) if current == place)),
            "a failed mutating finalizer quiesces its current owner before destruction"
        );
    }

    for block in mir.blocks() {
        let MirTerminatorKind::Suspend {
            cancellation,
            resume_state,
            ..
        } = block.terminator().kind()
        else {
            continue;
        };

        assert!(matches!(
            mir.block(cancellation.edge().target())
                .unwrap()
                .terminator()
                .kind(),
            MirTerminatorKind::Unreachable
        ));

        assert!(block.operations().iter().all(|id| !matches!(mir.operation(*id).unwrap().kind(),
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Immediate { value: bray_ir::MirImmediateValue::Boolean(false), .. }, .. }
                if destruction_guards.contains(&destination.storage())
        )));

        let state = mir
            .frame_descriptor()
            .unwrap()
            .states()
            .iter()
            .find(|state| state.state() == *resume_state)
            .unwrap();

        assert!(mir.block(state.broadcast()).is_some());
    }
}

#[test]
fn async_finalization_composes_with_local_owner_transitions() {
    for body in [
        "let owner = Owner { value = 1 }; return 7;",
        "Owner { value = 1 }; return 7;",
        "let owner = Owner { value = 1 }; if choice { return 7; } return 8;",
        "let first = Owner { value = 1 }; let second = first; return 7;",
        "let mut owner = Owner { value = 1 }; owner = Owner { value = 2 }; return 7;",
    ] {
        let compilation = compilation(&format!(
            r#"
            module app;
            extern async func pause();
            async func caller(pos choice: bool) -> u32
            {{
                {body}
            }}
            struct Owner
            {{
                value: u32;
                async finalize()
                {{
                    await pause();
                }}
            }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .unwrap();

        let mir = lowered_mir(&lowered);

        assert!(
            mir.is_valid(),
            "{body}: cleanup composition must retain valid MIR"
        );

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )),
            "{body}: selected async finalizers must compose in the enclosing frame"
        );
    }
}

#[test]
fn failed_later_inputs_await_initialized_temporary_finalizers() {
    for invocation in [
        "await accept(Owner { value = 1 });",
        "await accept(Owner { value = 1 }, number = rejected());",
        "await accept(Owner { value = 1 }, number = { let input = rejected(); yield input; });",
    ] {
        let source = r#"
        module app;
        extern async func pause();
        func rejected() -> u32 { panic("later input"); }
        struct Owner
        {
            value: u32;
            async finalize() { await pause(); }
        }
        async func accept(pos owner: Owner, number: u32 = rejected()) {}
        async func caller()
        {
            INVOCATION
        }
    "#
        .replace("INVOCATION", invocation);

        let application = compilation(&source);

        assert!(
            !application.check_diagnostics().has_errors(),
            "{:?}",
            application.check_diagnostics()
        );

        let lowered = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .unwrap();

        let mir = lowered_mir(&lowered);

        assert!(mir.is_valid());

        let failed = mir
            .blocks()
            .iter()
            .find_map(|block| {
                let operation = block
                    .operations()
                    .last()
                    .and_then(|id| mir.operation(*id))?;

                if !matches!(operation.kind(), MirOperationKind::Call(call)
            if matches!(call.result(), bray_bound_tree::BoundCallResult::Immediate(_)))
                {
                    return None;
                }

                match block.terminator().kind() {
                    MirTerminatorKind::CheckCallOutcome { panicked, .. } => Some(panicked.target()),
                    _ => None,
                }
            })
            .expect("evaluated later input retains its panic edge");

        let mut pending = vec![failed];
        let mut seen = BTreeSet::new();
        let mut composed = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            let block = mir.block(id).unwrap();

            composed |= block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
                )
            });

            block
                .terminator()
                .kind()
                .for_each_successor(|target| pending.push(target));
        }

        assert!(
            composed,
            "failed later input must compose the initialized input's async finalizer"
        );
    }
}

#[test]
fn unconsumed_frame_results_await_their_finalizers() {
    let application = compilation(
        r#"
        module app;
        extern async func pause();
        struct Owner
        {
            pending: Task<unit>;
            async finalize() { await pause(); }
        }
        async func caller(pos pending: Task<unit>) -> Owner
        {
            return Owner { pending = pending };
        }
    "#,
    );

    assert!(
        !application.check_diagnostics().has_errors(),
        "{:?}",
        application.check_diagnostics()
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    let mir = lowered_mir(&lowered);

    assert!(mir.is_valid());

    let suspended = mir
        .blocks()
        .iter()
        .filter_map(|block| match block.terminator().kind() {
            MirTerminatorKind::Suspend { resume_state, .. } => Some(*resume_state),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        !suspended.is_empty(),
        "unconsumed completion cleanup must drive the result's finalizer"
    );

    for resume in suspended {
        let state = mir
            .frame_descriptor()
            .unwrap()
            .states()
            .iter()
            .find(|state| state.state() == resume)
            .unwrap();

        assert!(state.is_completion_cleanup());

        let broadcast = mir.block(state.broadcast()).unwrap();

        assert!(
            broadcast.operations().iter().any(|id| matches!(
                mir.operation(*id).unwrap().kind(),
                MirOperationKind::Cleanup {
                    phase: bray_ir::MirCleanupPhase::TaskCancellation,
                    ..
                }
            )),
            "suspended completion cleanup must still broadcast to its owned tasks"
        );
    }
}

#[test]
fn temporary_and_unconsumed_results_reuse_unconditional_completion() {
    for (finalizer, completed) in [
        ("async finalize() executes(pure, total) {}", true),
        (
            "async finalize() when(self.ready) { executes(pure, total) } { if !self.ready { panic(\"unfinished\"); } }",
            false,
        ),
        (
            "async finalize() executes(total) { self.ready = false; }",
            false,
        ),
        (
            "async finalize() -> Result<unit, bool> executes(pure, total) { return Error(true); }",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Owner
            {{
                mut ready: bool;
                {finalizer}
            }}
            func rejected() -> u32 {{ panic("rejected"); }}
            async func accept(pos owner: Owner, pos number: u32 = rejected()) {{ panic("abnormal exit"); }}
            async func input() {{ await accept(Owner {{ ready = true }}); }}
            async func result() -> Owner {{ return Owner {{ ready = true }}; }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.check_diagnostics().has_errors(),
            "{source}: {:?}",
            application.check_diagnostics()
        );

        for name in ["input", "result"] {
            let lowered = application
                .lowered_unit(source_function_body_key(&application, name))
                .unwrap();

            let mir = lowered_mir(&lowered);

            assert!(mir.is_valid());

            let creates_finalizer = mir.operations().iter().any(|operation| {
                let MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                    initializer: bray_ir::MirFrameInitializer::Callable(call), ..
                }) = operation.kind() else {
                    return false;
                };

                matches!(call.target(), bray_ir::MirCallTarget::Direct(callable)
                    if matches!(callable.instance().definition().symbol(), bray_symbols::AnySymbolId::Finalizer(_)))
            });

            assert_eq!(creates_finalizer, !completed, "{name}: {source}");
        }
    }
}

#[test]
fn destructor_abandonment_applies_only_to_implicit_receiver_parts() {
    let source = r#"
        module app;

        struct Child
        {
            finalize() {}
            destruct() {}
        }

        struct Owner
        {
            child: Child;

            destruct()
            {
                let local = Child {};
            }
        }
    "#;

    let compilation = compilation(source);

    // Select the destructor containing the local owner; the empty Child destructor has no such storage.
    let candidates = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .filter(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .collect::<Vec<_>>();

    let units = candidates
        .into_iter()
        .map(|key| compilation.lowered_unit(key).unwrap())
        .collect::<Vec<_>>();

    let mir = units
        .iter()
        .map(|unit| lowered_mir(unit))
        .find(|mir| {
            mir.operations().iter().any(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Destroy {
                        abandoned: Some(_),
                        ..
                    }
                )
            })
        })
        .expect("the represented receiver tail retains abandonment");

    let inherited = mir
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Destroy {
                place,
                abandoned: Some(_),
            } => Some(place.storage()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(
        inherited.len(),
        1,
        "only receiver storage inherits abandonment"
    );

    assert!(
        mir.operations()
            .iter()
            .any(|operation| match operation.kind() {
                MirOperationKind::Destroy {
                    place,
                    abandoned: None,
                }
                | MirOperationKind::Cleanup { place, .. } => !inherited.contains(&place.storage()),
                _ => false,
            }),
        "ordinary local cleanup remains independent"
    );
}

#[test]
fn inactive_future_cleanup_composes_without_result_storage() {
    for body in [
        "let pending = child();",
        "child();",
        "let mut pending = child(); pending = child();",
        "let pending = child(); if choice { return; }",
    ] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Large
            {{
                values: [u64; 1024];
            }}

            extern async func child() -> Large;

            async func caller(pos choice: bool)
            {{
                {body}
            }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");
        let checked = compilation.async_analysis(key.clone()).unwrap();

        assert!(
            checked
                .value()
                .suspensions()
                .iter()
                .any(|point| point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup),
            "inactive ownership end: {body}"
        );

        let lowered = compilation.lowered_unit(key).unwrap();
        let mir = lowered_mir(&lowered);

        assert!(mir.is_valid(), "inactive ownership end: {body}");

        let mut composed = 0;
        let mut committed = 0;

        for operation in mir.operations() {
            match operation.kind() {
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                    completion,
                    ..
                }) => {
                    assert!(
                        completion.is_none(),
                        "discarded future has no result destination"
                    );

                    composed += 1;
                }
                MirOperationKind::Async(bray_ir::MirAsyncOperation::CommitAwaitedCompletion {
                    completion,
                    ..
                }) => {
                    assert!(completion.is_none());
                    assert!(operation.result().is_none());
                    committed += 1;
                }
                _ => {}
            }
        }

        assert!(composed > 0, "inactive ownership end: {body}");
        assert_eq!(composed, committed);
    }
}
