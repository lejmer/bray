use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_ir::{MirOperationKind, MirTerminatorKind};

use super::support::lowered_mir;
use crate::test_support::{
    compilation, source_function_body_key, source_type_callable_member_body_key,
};

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
fn borrowed_scoped_capability_retains_its_entry_receiver_through_exit() {
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

    let capability = storage
        .value()
        .access_entries()
        .find_map(|(_, access)| match access.root() {
            bray_bound_tree::StorageAccessRoot::BorrowedStorage {
                capability,
                storage: identity,
            } if matches!(
                storage.value().identity(identity),
                Some(bray_bound_tree::StorageIdentity::ScopedCapability { .. })
            ) =>
            {
                Some(storage.value().borrow_capability(capability).unwrap())
            }
            _ => None,
        })
        .expect("borrowed scoped result retains both its loan and its physical capability storage");

    let receiver = storage.value().root_identity(capability.access()).unwrap();

    assert!(matches!(
        storage.value().identity(receiver),
        Some(bray_bound_tree::StorageIdentity::Parameter(_))
    ));

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

        lowered_mir(&result);
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
