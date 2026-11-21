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
            .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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

        struct Resource
        {
            panic_on_cleanup: bool;

            destruct()
            {
                if self.panic_on_cleanup { panic("old owner cleanup"); }
            }
        }

        struct Container
        {
            mut value: Resource?;
        }

        func replace(pos owner: &mut Container, pos value: Resource)
        {
            owner.value = value;
        }
    "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "replace"))
        .expect("fixture callable must lower successfully");

    assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

    let mir = lowered_mir(&lowered);

    let collection = mir
        .operations()
        .iter()
        .find_map(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Cleanup {
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    ..
                }
            )
            .then(|| operation.kind().cleanup_collection())
            .flatten()
        })
        .expect("replacement cleanup borrows its enclosing failure collection");

    let cases = mir
        .blocks()
        .iter()
        .find_map(|block| match block.terminator().kind() {
            MirTerminatorKind::Switch {
                discriminant: bray_ir::MirOperand::Copy(place),
                cases,
                ..
            } if place == collection.state() => Some(cases),
            _ => None,
        })
        .expect("replacement dispatches its enclosing failure collection");

    let failures = cases.iter().map(|case| {
        mir.block(case.edge().target()).expect("fixture MIR block must exist")
    }).filter(|block| !matches!(block.terminator().kind(), MirTerminatorKind::Unreachable));

    assert!(failures.clone().any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Panic { .. })),
        "the old owner's possible panic has a failure continuation");

    for failure in failures {
        let mut transfers = 0;

        let mut count_transfer = |operand: &bray_ir::MirOperand| {
            if matches!(operand, bray_ir::MirOperand::Move(place) if place == collection.report()) {
                transfers += 1;
            }
        };

        for id in failure.operations() {
            mir.operation(*id)
                .expect("fixture MIR operation must exist")
                .kind()
                .for_each_operand(&mut count_transfer);
        }

        let terminator = failure.terminator().kind();

        terminator.for_each_input(&mut count_transfer);

        if let MirTerminatorKind::CancelCurrentRun { cleanup } = terminator {
            for argument in cleanup.edge().arguments() {
                count_transfer(argument);
            }
        }

        assert_eq!(transfers, 1, "the failure exit transfers the report exactly once: {failure:#?}");
    }
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

        let selections = compilation
            .semantic_selections(key.clone())
            .expect("fixture callable must have semantic selections");

        let scoped = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
                _ => None,
            })
            .expect("fixture selection must find its expected value");

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&result);
        let mut entered = false;
        let mut exited = false;

        for block in mir.blocks() {
            for (position, id) in block.operations().iter().enumerate() {
                let MirOperationKind::Call(call) = mir
                    .operation(*id)
                    .expect("fixture MIR operation must exist")
                    .kind()
                else {
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
                            mir.operation(*id)
                                .expect("fixture MIR operation must exist")
                                .kind(),
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

    let storage = compilation
        .storage_plan(key.clone())
        .expect("fixture callable must have a storage plan");

    let dependencies = compilation
        .dependency_contracts(key.clone())
        .expect("fixture `dependency_contracts` call must succeed");

    let (entry, _) = dependencies
        .value()
        .calls()
        .iter()
        .find(|(site, _)| matches!(site, bray_bound_tree::BoundExecutionSite::ScopedEnter(_)))
        .expect("fixture lookup must find its expected value");

    let expression = entry
        .expression()
        .expect("fixture expression must retain its checked type or storage");

    let (_, capability) = storage
        .value()
        .borrow_capability_entries()
        .find(|(_, loan)| loan.expression() == Some(expression))
        .expect("entry creates its receiver loan");

    let receiver = storage
        .value()
        .root_identity(capability.access())
        .expect("fixture `root_identity` call must succeed");

    assert!(matches!(
        storage.value().identity(receiver),
        Some(bray_bound_tree::StorageIdentity::Parameter(_))
    ));

    let liveness = compilation
        .liveness(key.clone())
        .expect("fixture `liveness` call must succeed");

    assert!(
        liveness.value().is_last_use(
            bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
            bray_bound_tree::BoundDependencySubject::Storage(receiver)
        ),
        "{:#?}",
        liveness.value()
    );

    let result = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

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

        let checked = compilation
            .async_analysis(key.clone())
            .expect("fixture callable must complete async analysis");

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

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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
        (
            "enter() -> Result<bool, i32> { return Ok(true); }",
            "exit(pos lease: bool) -> Result<unit, i32> { return Error(8); }",
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            module app;
            struct Resource {{}}
            impl Resource {{ {enter} {exit} }}
            func caller(pos resource: Resource) -> Result<u32, i32>
            {{
                return with lease: bool = resource {{
                    let body_value: u32 = if lease {{ yield 17; }} else {{ yield 23; }};

                    yield body_value;
                }};
            }}
        "#
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");

        let selections = compilation
            .semantic_selections(key.clone())
            .expect("fixture callable must have semantic selections");

        let scoped = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
                _ => None,
            })
            .expect("fixture selection must find its expected value");

        let types = compilation
            .expression_types(key.clone())
            .expect("fixture callable must complete expression typing");

        assert!(scoped.failure_type().is_some());

        assert_eq!(
            types
                .value()
                .expression(scoped.expression())
                .expect("fixture expression must retain its checked type or storage")
                .ty(),
            types.value().callable_result_type().expect("caller declares Result<u32, i32>")
        );

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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
                    mir.operation(*id)
                        .expect("fixture MIR operation must exist")
                        .kind(),
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
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .find(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .expect("fixture lookup must find its expected value");

    let result = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&result);

    let admission = mir
        .blocks()
        .iter()
        .find(|block| {
            block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id)
                        .expect("fixture `iter` call must succeed")
                        .kind(),
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

        assert!(!block.operations().iter().any(|id| matches!(mir.operation(*id).expect("fixture MIR operation must exist").kind(), MirOperationKind::Construct(construction) if construction.inputs().len() == 2)));

        let mut pending = vec![edge.target()];
        let mut seen = BTreeSet::new();
        let mut cleans_input = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            let block = mir.block(id).expect("fixture MIR block must exist");

            cleans_input |= block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id)
                        .expect("fixture `iter` call must succeed")
                        .kind(),
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

    let storage = compilation
        .storage_plan(key.clone())
        .expect("fixture callable must have a storage plan");

    let flow = compilation
        .storage_flow(key.clone())
        .expect("fixture callable must publish reached cleanup");

    let required = flow
        .value()
        .exits()
        .iter()
        .flat_map(|exit| {
            exit.cancellation_broadcast()
                .iter()
                .chain(exit.lifecycle_resolution())
        })
        .filter_map(|access| storage.value().root_identity(*access))
        .collect::<BTreeSet<_>>();

    let owned_locals = required.iter().copied().filter(|identity| {
        matches!(storage.value().identity(*identity), Some(bray_bound_tree::StorageIdentity::LocalOwned(_)))
    }).collect::<Vec<_>>();

    let [resource] = owned_locals.as_slice() else {
        panic!("the Resource must be the sole local owner selected for cleanup: {owned_locals:?}");
    };

    let resource_access = storage.value().root_access(*resource)
        .and_then(|access| storage.value().access(access))
        .expect("the selected resource retains its checked root access");

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
            lowered_mir(&lowered)
                .storage(*storage)
                .expect("fixture `storage` call must succeed")
                .kind()
                != &bray_ir::MirStorageKind::Return
        })
        .map(|(storage, _)| *storage)
        .collect::<BTreeSet<_>>();

    assert_eq!(cleanup_storages.len(), required.len(), "{cleanup_places:?}");

    assert!(cleanup_storages.iter().any(|&storage| {
        let owner = lowered_mir(&lowered).storage(storage).expect("cleanup names its MIR storage");

        owner.kind() == &bray_ir::MirStorageKind::Local
            && owner.ty() == resource_access.reached_type()
            && matches!(owner.source(), bray_ir::MirSourceAnchor::Source(origin)
                if origin.source_anchor() == resource_access.source())
    }), "MIR cleans up the Resource at its checked declaration source");
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
    let key = source_function_body_key(&compilation, "create");
    let selections = compilation.semantic_selections(key.clone()).expect("factory selection");

    let invocation = selections.value().entries().iter().find_map(|entry| match entry.selection() {
        bray_bound_tree::SemanticSelection::Call(call)
            if matches!(call.resolution().result(), bray_bound_tree::BoundCallResult::LazyFuture(_)) => Some(entry.expression()),
        _ => None,
    }).expect("fixture has a selected lazy factory");

    let flow = compilation.storage_flow(key.clone()).expect("factory has checked storage flow");
    let storage = compilation.storage_plan(key.clone()).expect("factory has a checked input slot");

    let (input, _) = storage.value().identity_entries().find(|(_, identity)| matches!(identity,
        bray_bound_tree::StorageIdentity::InvocationInput { expression, ordinal }
            if *expression == invocation && ordinal.raw() == 0
    )).expect("factory keeps its staged guard before admission");

    assert!(!flow.value().invocation_can_fail(invocation.into(), bray_symbols::DependencyFailureKind::Panic));

    assert!(flow.value().exits().iter().any(|exit| {
        exit.exit() == invocation.into()
            && exit.lifecycle_resolution().iter()
                .any(|access| storage.value().root_identity(*access) == Some(input))
    }), "reservation failure leaves the staged input with caller cleanup");

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    let admission = mir
        .blocks()
        .iter()
        .find(|block| {
            block.operations().last().is_some_and(|id| {
                matches!(
                    mir.operation(*id)
                        .expect("fixture sequence must contain its last item")
                        .kind(),
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
        .operation(
            *admission
                .operations()
                .last()
                .expect("fixture sequence must contain its last item"),
        )
        .expect("fixture MIR operation must exist")
        .kind()
    {
        let bray_ir::MirOperand::Move(factory) = callee else {
            panic!("indirect admission retains the evaluated factory");
        };

        assert!(matches!(
            mir.storage(factory.storage())
                .expect("fixture `storage` call must succeed")
                .kind(),
            bray_ir::MirStorageKind::Temporary
        ));

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Copy(source), .. }
                if destination == factory && matches!(mir.storage(source.storage()).expect("fixture `storage` call must succeed").kind(), bray_ir::MirStorageKind::Parameter(_))
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
        .expect("fixture MIR block must exist")
        .operations()
        .iter()
        .find_map(|id| {
            let MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(call),
                ..
            }) = mir
                .operation(*id)
                .expect("fixture MIR operation must exist")
                .kind()
            else {
                return None;
            };

            let bray_ir::MirOperand::Move(place) = call.arguments()[0].value() else {
                panic!("constructor moves the guarded input");
            };

            Some(place)
        })
        .expect("only admitted construction transfers captures");

    let reached = bray_ir::reachable_blocks(mir, [panicked.target()], &Default::default());

    let operations = reached.iter().filter_map(|id| mir.block(*id))
        .flat_map(|block| block.operations()).filter_map(|id| mir.operation(*id));

    assert!(!operations.clone().any(|operation| matches!(operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
    )), "failed admission never constructs a frame");

    assert!(operations.clone().any(|operation| matches!(operation.kind(),
        MirOperationKind::Cleanup {
            phase: bray_ir::MirCleanupPhase::LifecycleResolution, place, ..
        } if place == capture
    )), "failed admission resolves the exact retained input");
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
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    let entry = mir
        .block(
            mir.frame_descriptor()
                .expect("fixture `frame_descriptor` call must succeed")
                .states()[0]
                .entry(),
        )
        .expect("fixture MIR block must exist");

    let MirTerminatorKind::Branch {
        condition,
        then_edge,
        else_edge,
    } = entry.terminator().kind()
    else {
        panic!("initial frame state selects cancellation before source execution");
    };

    assert!(entry.operations().iter().any(|id| {
        let operation = mir
            .operation(*id)
            .expect("fixture MIR operation must exist");
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

        let block = mir.block(id).expect("fixture MIR block must exist");

        for id in block.operations() {
            let kind = mir
                .operation(*id)
                .expect("fixture MIR operation must exist")
                .kind();

            if let MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                place,
                ..
            } = kind
            {
                cleans_capture |= matches!(
                    mir.storage(place.storage())
                        .expect("fixture `storage` call must succeed")
                        .kind(),
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
fn scope_broadcast_visits_a_task_capability_consumed_by_scoped_exit() {
    let compilation = compilation(
        r#"
        module app;

        async func work() {}

        struct Lease { task: Task<unit>; }

        struct Resource
        {
            async enter() -> Lease
            {
                return Lease { task = work().start() };
            }

            async exit(pos lease: Lease) {}
        }

        async func caller(pos resource: Resource)
        {
            with _ = resource {};
        }
        "#,
    );

    let key = source_function_body_key(&compilation, "caller");
    let storage = compilation.storage_plan(key.clone()).expect("fixture storage plan");
    let storage = storage.value();

    let (capability, expression) = storage.identity_entries()
        .find_map(|(identity, record)| match record {
            bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. } => Some((identity, expression)),
            _ => None,
        })
        .expect("with binds the owned lease capability");

    let access = storage.root_access(capability).expect("lease capability has a root access");
    let ty = storage.storage_type(capability).expect("lease capability has a type");

    let checked = compilation.storage_flow(key.clone()).expect("fixture reached cleanup");

    assert!(!checked.diagnostics().has_errors(), "{:?}", checked.diagnostics());

    let plans = checked.value().exits().iter().filter(|plan| plan.scoped_exit() == Some(expression))
        .collect::<Vec<_>>();

    assert!(!plans.is_empty());

    let committed = plans.iter().find(|plan| plan.exit() == expression.into())
        .expect("cancellation after entry completion owns the acquired capability");

    assert_eq!(committed.cancellation_broadcast(), &[access]);

    for plan in plans {
        assert_eq!(plan.cancellation_broadcast(), &[access]);
        assert!(!plan.lifecycle_resolution().contains(&access), "exit consumes the capability");
    }

    let lowered = compilation.lowered_unit(key).expect("fixture lowers with its selected lifecycle");

    assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());

    let mir = lowered_mir(&lowered);

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Cleanup { phase: bray_ir::MirCleanupPhase::TaskCancellation, place, .. }
            if mir.storage(place.storage()).is_some_and(|storage| storage.ty() == ty)
    )));
}

#[test]
fn scoped_exit_panic_broadcasts_only_for_newly_exited_owners() {
    use bray_ir::{MirAsyncOperation, MirBlockKind, MirCallTarget, MirCleanupPhase, MirOutgoingOwner, MirStorageKind};

    for (outer, extra_broadcasts) in [("", 0), ("let outer = child().start();", 1)] {
        let compilation = compilation(&format!(r#"
            module app;
            struct Resource {{
                enter() -> bool {{ return true; }}
                exit(pos acquired: bool) -> Result<unit, bool> {{
                    if acquired {{ panic("exit"); }}
                    return Ok(unit);
                }}
            }}
            async func child() -> u8 {{ return 7; }}
            async func caller() {{
                {outer}
                let resource = Resource {{}};
                _ = with _ = resource {{
                    let task = child().start();
                    yield unit;
                }};
            }}
        "#));

        let key = source_function_body_key(&compilation, "caller");
        let lowered = compilation.lowered_unit(key).expect("fixture lowers checked cleanup");

        assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());

        let mir = lowered_mir(&lowered);

        let operations = |id| mir.block(id).into_iter().flat_map(bray_ir::MirBlock::operations)
            .filter_map(|operation| mir.operation(*operation));

        let started = mir.blocks().iter().rev().find(|block|
            block.operations().iter().filter_map(|id| mir.operation(*id)).any(|operation| matches!(
                operation.kind(), MirOperationKind::Async(MirAsyncOperation::StartTask { .. })
            )))
            .expect("fixture starts a child inside the scope");

        let MirTerminatorKind::CheckCallOutcome { completed, .. } = started.terminator().kind() else {
            panic!("the task is owned only after start succeeds");
        };

        let broadcasts = |id| operations(id).filter_map(|operation| match operation.kind() {
            MirOperationKind::Cleanup { phase: MirCleanupPhase::TaskCancellation, place, .. }
                if mir.storage(place.storage()).is_some_and(|storage| storage.kind() == &MirStorageKind::Local) =>
                    Some(place.storage()),
            _ => None,
        });

        let invokes_exit = |id| operations(id).any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(target)
                if target.instance().definition().callable_symbol().kind() == bray_symbols::SymbolKind::ScopeExit)
        ));

        let mut pending = std::collections::VecDeque::from([completed.target()]);
        let mut visited = BTreeSet::new();
        let mut issued = BTreeSet::new();
        let mut lifecycle = Vec::new();

        while let Some(id) = pending.pop_front() {
            if !visited.insert(id) { continue; }

            let block = mir.block(id).expect("cleanup successor belongs to its fixture");

            if block.kind() == MirBlockKind::LifecycleResolution {
                lifecycle.push(id);
                continue;
            }

            issued.extend(broadcasts(id));
            block.terminator().kind().for_each_successor(|target| pending.push_back(target));
        }

        assert_eq!(issued.len(), 1, "ordinary exit broadcasts to the inner child");
        assert_eq!(lifecycle.len(), 1, "broadcast completes before the first lifecycle action");

        pending.extend(lifecycle);
        visited.clear();

        let panicked = loop {
            let id = pending.pop_front().expect("ordinary cleanup reaches its scoped exit");

            if !visited.insert(id) { continue; }

            let block = mir.block(id).expect("cleanup successor belongs to its fixture");

            if invokes_exit(id) {
                let MirTerminatorKind::CheckCallOutcome { panicked: edge, .. } = block.terminator().kind() else {
                    panic!("the scoped exit publishes a panic edge");
                };

                break edge.target();
            }

            block.terminator().kind().for_each_successor(|target| pending.push_back(target));
        };

        pending.clear();
        pending.push_back(panicked);
        visited.clear();

        // A retained broadcast entry belongs to a separate frame command.
        while let Some(id) = pending.pop_front() {
            if visited.insert(id) {
                mir.block(id).expect("panic continuation stays in its unit")
                    .terminator().kind().for_each_successor(|target| pending.push_back(target));
            }
        }

        assert!(!visited.iter().copied().any(invokes_exit), "the panic cannot invoke the scoped exit again");

        let discharges = visited.iter().copied().flat_map(operations).filter(|operation| matches!(
            operation.kind(), MirOperationKind::DischargeOutgoing { owner: MirOutgoingOwner::Invocation { selection, .. }, .. }
                if selection.reference().instance().definition().callable_symbol().kind() == bray_symbols::SymbolKind::ScopeExit
        )).count();

        assert_eq!(discharges, 1, "the failed exit releases its admitted backing");

        let remaining = visited.into_iter().flat_map(broadcasts).collect::<BTreeSet<_>>();

        assert!(issued.is_disjoint(&remaining), "the panic cannot repeat an issued request: {issued:?} vs {remaining:?}");
        assert_eq!(remaining.len(), extra_broadcasts, "the panic broadcasts to the newly exited outer child: {remaining:?}");
    }
}

#[test]
fn scoped_backing_rejection_reaches_the_catch_before_entering_the_scope() {
    for exit in [
        "async exit(pos lease: bool) {}",
        "exit(pos lease: bool) -> Result<unit, i32> { return Ok(unit); }",
    ] {
        let source = format!(r#"
        module app;

        struct Resource {{}}
        impl Resource {{
            enter() -> bool {{ return true; }}
            {exit}
        }}

        async func caller(pos resource: Resource) -> Result<i32, PanicReport> {{
            return catch {{
                let outcome = with _ = resource {{ yield 7; }};
                yield 9;
            }};
        }}
    "#);

        let compilation = compilation(&source);

        let key = source_function_body_key(&compilation, "caller");
        let selections = compilation.semantic_selections(key.clone()).expect("scoped use selects its entry");

        let scoped = selections.value().entries().iter().find_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
            _ => None,
        }).expect("fixture has one scoped use");

        let expression = scoped.expression();
        let storage = compilation.storage_flow(key.clone()).expect("scope publishes its reached exits");

        assert!(!storage.value().invocation_can_fail(bray_bound_tree::BoundExecutionSite::ScopedEnter(expression), bray_symbols::DependencyFailureKind::Panic));
        assert!(storage.value().exits().iter().any(|exit| exit.exit() == expression.into()));

        let lowered = compilation.lowered_unit(key).expect("backing rejection has checked cleanup");
        let mir = lowered_mir(&lowered);

        let reservation = mir.blocks().iter().find(|block| {
            block.operations().iter().filter_map(|id| mir.operation(*id)).any(|operation| matches!(operation.kind(),
                MirOperationKind::AdmitOutgoing { owner: bray_ir::MirOutgoingOwner::Invocation {
                    selection, result,
                }, .. } if selection.reference().instance() == scoped.exit().0 && *result == scoped.exit_result()
            ))
        }).expect("the exit factory reserves backing before entry");

        let MirTerminatorKind::CheckCallOutcome { panicked, .. } = reservation.terminator().kind() else {
            panic!("reservation must route an allocation panic");
        };

        let result = compilation.available_compiler_known_symbols().result_representation().expect("fixture has the Result representation");
        let reached = bray_ir::reachable_blocks(mir, [panicked.target()], &Default::default());
        let operations = reached.iter().filter_map(|id| mir.block(*id)).flat_map(|block| block.operations()).filter_map(|id| mir.operation(*id));

        assert!(operations.clone().any(|operation| matches!(operation.kind(),
            MirOperationKind::Construct(construction)
                if construction.target() == bray_bound_tree::ConstructionTarget::UnionVariant(result.error_variant())
        )), "the enclosing catch owns the reservation panic");

        assert!(!operations.clone().any(|operation| matches!(operation.kind(),
            MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Direct(target)
                if matches!(target.instance().definition().callable_symbol(), bray_symbols::CallableSymbolId::ScopeEnter(_)))
        )), "reservation rejection precedes invocation of the entry");
    }
}

#[test]
fn cancelled_scoped_entry_cleans_the_committed_error_without_running_exit() {
    let compilation = compilation(r#"
        module app;
        async func work() {}
        struct Failure { child: Task<unit>; }
        struct Resource {}

        impl Resource {
            async enter() -> Result<bool, Failure> {
                return Error(Failure { child = work().start() });
            }

            async exit(pos lease: bool) {}
        }

        async func caller(pos resource: Resource) -> Result<i32, Failure> {
            return with lease = resource { yield 7; };
        }
    "#);

    let key = source_function_body_key(&compilation, "caller");
    let selected = compilation.semantic_selections(key.clone()).expect("scope has a selected entry");

    let expression = selected.value().entries().iter().find_map(|entry| match entry.selection() {
        bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped.expression()),
        _ => None,
    }).expect("fixture has one scoped use");

    let storage = compilation.storage_plan(key.clone()).expect("scoped error has a planned result");
    let region = storage.value().value_slot(expression).expect("scoped error has a slot");
    let result = storage.value().storage_type(region).expect("scoped result has a type");
    let lowered = compilation.lowered_unit(key).expect("committed scoped error lowers");
    let mir = lowered_mir(&lowered);

    let (owner, cancelled) = mir.blocks().iter().find_map(|block| {
        let MirTerminatorKind::Branch { then_edge, .. } = block.terminator().kind() else { return None; };

        let mut operations = block.operations().iter().filter_map(|id| mir.operation(*id));

        let owner = operations.find_map(|operation| match operation.kind() {
            MirOperationKind::Store { destination, .. } if destination.ty() == result => Some(destination),
            _ => None,
        })?;

        operations.any(|operation| matches!(operation.kind(), MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)))
            .then_some((owner, then_edge.target()))
    }).expect("the Error value takes ownership before the cancellation poll");

    let cancelled = mir.block(cancelled).expect("the cancellation poll has a rejection block");

    let MirTerminatorKind::CancelCurrentRun { cleanup } = cancelled.terminator().kind() else {
        panic!("cancellation after acquiring Error must leave the current run");
    };

    let reached = bray_ir::reachable_blocks(mir, [cleanup.edge().target()], &Default::default());
    let operations = reached.iter().filter_map(|id| mir.block(*id)).flat_map(|block| block.operations()).filter_map(|id| mir.operation(*id));

    for expected in [bray_ir::MirCleanupPhase::TaskCancellation, bray_ir::MirCleanupPhase::LifecycleResolution] {
        assert!(operations.clone().any(|operation| matches!(operation.kind(),
            MirOperationKind::Cleanup { phase, place, .. } if *phase == expected && place.storage() == owner.storage()
        )), "committed Error must own cleanup in {expected:?}");
    }

    assert!(!operations.clone().any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call)
        | MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
            initializer: bray_ir::MirFrameInitializer::Callable(call), ..
        }) if matches!(call.target(), bray_ir::MirCallTarget::Direct(target)
            if matches!(target.instance().definition().callable_symbol(), bray_symbols::CallableSymbolId::ScopeExit(_)))
    )), "an Error from entry cannot acquire a scope exit invocation");
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

    let checked = compilation
        .async_analysis(key.clone())
        .expect("fixture callable must complete async analysis");

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
        .expect("fixture lookup must find its expected value");

    assert!(!exit.cancellation_broadcast().is_empty());

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    assert!(
        mir.is_valid(),
        "broadcast callbacks must satisfy MIR contracts"
    );

    let frame = mir
        .frame_descriptor()
        .expect("fixture `frame_descriptor` call must succeed");

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
                .expect("fixture MIR block must exist")
                .terminator()
                .kind(),
            MirTerminatorKind::Unreachable
        ) {
            shielded = true;

            let entry = mir
                .block(
                    frame.states()[usize::try_from(resume_state.raw())
                        .expect("fixture value must fit the requested representation")]
                    .entry(),
                )
                .expect("fixture MIR block must exist");

            assert!(
                !entry.operations().iter().any(|operation| matches!(
                    mir.operation(*operation)
                        .expect("fixture MIR operation must exist")
                        .kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)
                )),
                "a shielded exit resumes its pending continuation even if cancellation started cleanup"
            );
        }
    }

    assert!(shielded);

    for state in frame.states() {
        let mut pending = vec![state.retention().broadcast()];
        let mut seen = BTreeSet::new();
        let mut broadcasts_task = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            assert_ne!(id, state.entry(), "broadcast does not resume the frame");

            let block = mir.block(id).expect("fixture MIR block must exist");

            assert!(!matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. } | MirTerminatorKind::Unreachable
            ));

            for id in block.operations() {
                if let MirOperationKind::Cleanup { phase, place, .. } = mir
                    .operation(*id)
                    .expect("fixture MIR operation must exist")
                    .kind()
                {
                    assert_eq!(*phase, bray_ir::MirCleanupPhase::TaskCancellation);

                    broadcasts_task |= matches!(
                        mir.storage(place.storage())
                            .expect("fixture `storage` call must succeed")
                            .kind(),
                        bray_ir::MirStorageKind::Parameter(0)
                    );
                }

                if let MirOperationKind::Store { destination, .. } = mir
                    .operation(*id)
                    .expect("fixture MIR operation must exist")
                    .kind()
                {
                    assert!(
                        matches!(
                            mir.storage(destination.storage())
                                .expect("fixture `storage` call must succeed")
                                .kind(),
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
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    assert!(mir.is_valid());

    for state in mir
        .frame_descriptor()
        .expect("fixture `frame_descriptor` call must succeed")
        .states()
    {
        let block = mir
            .block(state.retention().broadcast())
            .expect("fixture MIR block must exist");

        assert!(block.operations().is_empty());

        assert!(matches!(
            block.terminator().kind(),
            MirTerminatorKind::Return {
                value: None,
                report: None
            }
        ));
    }

    assert!(
        !mir.storages()
            .iter()
            .any(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::BroadcastLocal))
    );
}

#[test]
fn ordinary_finalizers_keep_selected_execution_in_the_enclosing_cleanup() {
    for (execution, awaiting) in [("", ""), ("async ", "await ")] {
        let source = format!(
            r#"
            module app;

            extern {execution}func pause();

            {execution}func caller(pos owner: Owner, pos fail: bool)
            {{
                if fail
                {{
                    panic("body");
                }}
            }}

            struct Owner
            {{
                mut touched: bool;

                {execution}finalize()
                {{
                    {awaiting}pause();
                    self.touched = true;
                }}
            }}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "caller");

        let checked = compilation
            .async_analysis(key.clone())
            .expect("fixture callable must complete async analysis");

        let selected = checked
            .value()
            .cleanup_types()
            .iter()
            .find_map(|ty| ty.finalizer())
            .expect("fixture selection must find its expected value");

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&lowered);

        assert!(mir.is_valid(), "{source}");

        let invocations = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Call(call)
                | MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                    initializer: bray_ir::MirFrameInitializer::Callable(call),
                    ..
                }) if matches!(call.target(), bray_ir::MirCallTarget::Direct(reference)
                if reference.instance() == selected.callable) =>
                {
                    Some(call.result())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(
            !invocations.is_empty(),
            "source cleanup invokes the checked finalizer directly"
        );

        assert!(
            invocations
                .iter()
                .all(|invocation| *invocation == selected.result)
        );

        assert_eq!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )),
            !execution.is_empty()
        );
    }
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

    let checked = compilation
        .async_analysis(key.clone())
        .expect("fixture callable must complete async analysis");

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

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    assert!(
        mir.is_valid(),
        "ordinary async cleanup must obey MIR ownership and suspension contracts"
    );

    let retained = mir
        .operations()
        .iter()
        .filter_map(|operation| {
            if !matches!(operation.kind(), MirOperationKind::Quiesce { .. }) {
                return None;
            }

            operation
                .kind()
                .cleanup_collection()
                .and_then(|collection| collection.retention())
        })
        .collect::<Vec<_>>();

    assert!(
        !retained.is_empty(),
        "failed finalizer quiescence preserves its checked continuation"
    );

    for retention in retained {
        assert!(
            mir.frame_descriptor()
                .expect("fixture `frame_descriptor` call must succeed")
                .states()
                .iter()
                .any(|state| state.retention() == retention.as_ref()),
            "generated quiescence reuses the selected source continuation without reproof"
        );
    }

    let (reconstructed, _) = bray_ir::reconstruct_reachable(mir)
        .expect("fixture `bray_ir::reconstruct_reachable` call must succeed");

    assert!(reconstructed.is_valid());

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
                mir.operation(*id)
                    .expect("fixture `iter` call must succeed")
                    .kind(),
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
                } = mir
                    .operation(*id)
                    .expect("fixture MIR operation must exist")
                    .kind()
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
            ..
        } = operation.kind()
        else {
            continue;
        };

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(),
            MirOperationKind::Quiesce { place: current, .. } if current == place)),
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
                .expect("fixture MIR block must exist")
                .terminator()
                .kind(),
            MirTerminatorKind::Unreachable
        ));

        assert!(block.operations().iter().all(|id| !matches!(mir.operation(*id).expect("fixture MIR operation must exist").kind(),
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Immediate { value: bray_ir::MirImmediateValue::Boolean(false), .. }, .. }
                if destruction_guards.contains(&destination.storage())
        )));

        let state = mir
            .frame_descriptor()
            .expect("fixture `frame_descriptor` call must succeed")
            .states()
            .iter()
            .find(|state| state.state() == *resume_state)
            .expect("fixture lookup must find its expected value");

        assert!(mir.block(state.retention().broadcast()).is_some());
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
            .expect("fixture callable must lower successfully");

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
            .expect("fixture callable must lower successfully");

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

            let block = mir.block(id).expect("fixture MIR block must exist");

            composed |= block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id)
                        .expect("fixture `iter` call must succeed")
                        .kind(),
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
        .expect("fixture callable must lower successfully");

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
            .expect("fixture `frame_descriptor` call must succeed")
            .states()
            .iter()
            .find(|state| state.state() == resume)
            .expect("fixture lookup must find its expected value");

        assert!(state.retention().is_completion_cleanup());

        let broadcast = mir
            .block(state.retention().broadcast())
            .expect("fixture MIR block must exist");

        assert!(
            broadcast.operations().iter().any(|id| matches!(
                mir.operation(*id)
                    .expect("fixture MIR operation must exist")
                    .kind(),
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
                .expect("fixture callable must lower successfully");

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

    // Select the destructor containing the local owner. The empty Child destructor has no such storage.
    let candidates = compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .filter(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .collect::<Vec<_>>();

    let units = candidates
        .into_iter()
        .map(|key| {
            compilation
                .lowered_unit(key)
                .expect("fixture callable must lower successfully")
        })
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

    let receiver = mir
        .storages_with_ids()
        .find_map(|(id, storage)| {
            matches!(
                storage.kind(),
                bray_ir::MirStorageKind::BorrowedParameter(0)
            )
            .then_some(id)
        })
        .expect("the consuming body borrows the caller's receiver storage");

    let tail = mir
        .cleanup_continuation()
        .expect("receiver cleanup is a caller-owned tail");

    let body = mir
        .synchronous_body()
        .expect("fixture `synchronous_body` call must succeed");

    assert!(mir.is_valid());
    assert!(body.is_valid());
    assert!(body.cleanup_continuation().is_none());
    assert!(tail.entries().iter().any(|entry| entry.is_initial()));

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Destroy { place, abandoned: Some(bray_ir::MirOperand::Immediate {
            value: bray_ir::MirImmediateValue::Boolean(true), .. }), .. } if place.storage() == receiver)));

    assert!(
        body.operations()
            .iter()
            .any(|operation| match operation.kind() {
                MirOperationKind::Destroy {
                    place,
                    abandoned: None | Some(bray_ir::MirOperand::Value(_)),
                    ..
                }
                | MirOperationKind::Cleanup { place, .. } => !matches!(
                    body.storage(place.storage())
                        .expect("fixture `storage` call must succeed")
                        .kind(),
                    bray_ir::MirStorageKind::BorrowedParameter(0)
                ),
                _ => false,
            }),
        "ordinary local cleanup stays in the synchronous source body"
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

        let checked = compilation
            .async_analysis(key.clone())
            .expect("fixture callable must complete async analysis");

        assert!(
            checked
                .value()
                .suspensions()
                .iter()
                .any(|point| point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup),
            "inactive ownership end: {body}"
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&lowered);

        assert!(mir.is_valid(), "inactive ownership end: {body}");

        let mut composed = 0;
        let mut committed = 0;

        for operation in mir.operations() {
            match operation.kind() {
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                    composition,
                    ..
                }) => {
                    assert!(
                        *composition == bray_ir::MirFrameComposition::Abandon,
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

#[test]
fn destructor_nested_callable_does_not_inherit_receiver_abandonment() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            child: Child;

            destruct()
            {
                let identity = lambda(pos value: bool) -> bool
                {
                    return value;
                };
                let value = identity(true);
            }
        }
        "#,
    );

    let key = compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
        .into_iter()
        .find(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .expect("fixture lookup must find its expected value");

    let bound = compilation
        .bound_unit(key.clone())
        .expect("fixture callable must bind successfully");

    let nested = bound
        .value()
        .nested_units()
        .first()
        .expect("fixture sequence must contain its first item")
        .clone();

    for (key, receiver_cleanup) in [(key, true), (nested, false)] {
        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&lowered);

        assert_eq!(
            mir.cleanup_continuation().is_some(),
            receiver_cleanup,
            "only the destructor publishes receiver cleanup entries"
        );
    }
}

#[test]
fn returned_error_owns_a_future_extracted_from_a_nullable_binding() {
    let application = compilation(
        r#"
        module app;

        extern async func pause();
        extern func take(pos source: &mut (Future<unit>?)) -> Future<unit>?;

        struct Failure
        {
            value: u64;
            pending: Future<unit>;

            finalize()
            {
                panic("retained error abandons finalization");
            }

            destruct()
            {
            }
        }

        struct Owner
        {
            mut pending: Future<unit>?;

            async finalize() -> Result<unit, Failure>
            {
                await pause();

                let stored: Future<unit>? = take(&mut self.pending);

                match consume stored
                {
                    case ?pending
                    {
                        return Error(
                            Failure
                            {
                                value = 17,
                                pending = pending
                            }
                        );
                    }
                    case none
                    {
                        panic("missing capture");
                    }
                }
            }

            destruct()
            {
            }
        }
        "#,
    );

    assert!(
        !application.check_diagnostics().has_errors(),
        "{:?}",
        application.check_diagnostics()
    );

    let keys = application
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units");

    let finalizers = keys
        .into_iter()
        .filter(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Finalizer
        })
        .collect::<Vec<_>>();

    assert_eq!(finalizers.len(), 2);

    for key in finalizers {
        let lowered = application
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        assert!(lowered_mir(&lowered).is_valid());
    }
}

#[test]
fn destructor_receiver_cleanup_uses_the_consuming_context() {
    for (execution, completed, valid) in [
        ("async ", false, true),
        ("", false, false),
        ("", true, true),
    ] {
        let finalizer = if completed {
            "async finalize() executes(pure, total) {}"
        } else {
            "async finalize() { await pause(); }"
        };

        let application = compilation(&format!(
            r#"
            module app;
            extern async func pause();
            struct Child
            {{
                {finalizer}
            }}
            struct Owner
            {{
                child: Child;
                destruct()
                {{
                }}
            }}
            {execution}func root(pos owner: Owner)
            {{
            }}
        "#
        ));

        if valid {
            assert!(
                !application.check_diagnostics().has_errors(),
                "{:?}",
                application.check_diagnostics()
            );

            let lowered = application
                .lowered_unit(source_function_body_key(&application, "root"))
                .expect("fixture callable must lower successfully");

            assert!(lowered_mir(&lowered).is_valid());
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                application.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn destructor_receiver_cleanup_excludes_consumed_parts_and_preserves_local_context() {
    for (finalizer, valid) in [
        ("async finalize() executes(pure, total) {}", true),
        ("async finalize() { await pause(); }", false),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            extern async func pause();
            struct Child
            {{
                {finalizer}
            }}
            struct Owner
            {{
                child: Child;
                destruct()
                {{
                    let local = self.child;
                }}
            }}
            func root(pos owner: Owner)
            {{
            }}
        "#
        ));

        if valid {
            assert!(
                !application.check_diagnostics().has_errors(),
                "{:?}",
                application.check_diagnostics()
            );

            let key = application
                .declared_unit_keys_for_test()
                .expect("fixture declarations must resolve their units")
                .into_iter()
                .find(|key| {
                    key.kind() == BoundUnitKind::CallableBody
                        && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
                })
                .expect("fixture lookup must find its expected value");

            let body = application
                .body_semantics_with_cancellation(key, &crate::CancellationToken::new())
                .expect("fixture `body_semantics_with_cancellation` call must succeed");

            assert!(
                body.result()
                    .value()
                    .storage_flow()
                    .receiver_cleanup()
                    .is_empty(),
                "consumed fields have no implicit receiver tail"
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                application.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn retained_capture_cleanup_publishes_frame_outcomes() {
    let application = compilation(
        r#"
        module app;
        struct Capture
        {
            destruct()
            {
            }
        }
        async func pending(pos capture: Capture)
        {
            panic("inactive body");
        }
    "#,
    );

    assert!(
        !application.check_diagnostics().has_errors(),
        "{:?}",
        application.check_diagnostics()
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "pending"))
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);

    assert!(
        mir.frame_descriptor()
            .expect("fixture `frame_descriptor` call must succeed")
            .capture_cleanup()
            .is_some()
    );

    let mut cancellation_entries = 0;

    for block in mir.blocks() {
        if let MirTerminatorKind::CancelCurrentRun { cleanup } = block.terminator().kind() {
            let entry = mir
                .block(cleanup.edge().target())
                .expect("fixture MIR block must exist");

            assert!(entry.operations().iter().any(|operation| {
                matches!(mir.operation(*operation).expect("fixture MIR operation must exist").kind(),
                    MirOperationKind::Call(call)
                        if matches!(call.target(), bray_ir::MirCallTarget::Runtime(runtime)
                            if runtime.role() == bray_runtime_interface::RuntimeAbiRole::CleanupShieldEnter))
            }), "cancellation enters the shield before forwarding pending failure reports: {entry:#?}");

            cancellation_entries += 1;
        }

        if block.kind() != bray_ir::MirBlockKind::CleanupBroadcast {
            assert!(
                !matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::PropagatePanic { .. }
                        | MirTerminatorKind::PropagateCancellation { .. }
                ),
                "resumable capture cleanup returns through frame terminal publication: {:?}",
                block.terminator()
            );
        }
    }

    assert!(cancellation_entries > 0);
}

#[test]
fn destructor_receiver_tail_retains_normal_completion() {
    for (fields, body, reports) in [
        ("", "assert(true);", false),
        (
            "action: func() -> unit executes(total);",
            "self.action();",
            true,
        ),
    ] {
        let source = r#"
        module app;
        struct Owner
        {
            pending: Future<unit>;
            ExtraFields
            destruct()
            {
                Body
            }
        }
    "#
        .replace("ExtraFields", fields)
        .replace("Body", body);

        let application = compilation(&source);

        assert!(
            !application.check_diagnostics().has_errors(),
            "{:?}",
            application.check_diagnostics()
        );

        let key = application
            .declared_unit_keys_for_test()
            .expect("fixture declarations must resolve their units")
            .into_iter()
            .find(|key| {
                key.kind() == BoundUnitKind::CallableBody
                    && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
            })
            .expect("fixture lookup must find its expected value");

        let lowered = application
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&lowered);

        let tail = mir
            .cleanup_continuation()
            .expect("fixture `cleanup_continuation` call must succeed");

        assert!(mir.is_valid(), "{source}");

        assert_eq!(
            tail.entries().iter().any(|entry| matches!(
                mir.block(entry.handoff())
                    .expect("fixture MIR block must exist")
                    .terminator()
                    .kind(),
                MirTerminatorKind::Return {
                    report: Some(_),
                    ..
                }
            )),
            reports,
            "{source}"
        );

        for abandoned in [false, true] {
            let selected = |entry: &bray_ir::MirCleanupEntry| {
                if abandoned {
                    entry.abandoned()
                } else {
                    entry.ordinary()
                }
            };

            let handoffs = tail
                .entries()
                .iter()
                .map(|entry| (entry.handoff(), vec![selected(entry)]))
                .collect::<std::collections::BTreeMap<_, _>>();

            let roots = tail
                .entries()
                .iter()
                .filter(|entry| entry.is_initial())
                .map(selected);

            let reachable = bray_ir::reachable_blocks(mir, roots, &handoffs);

            assert!(
                reachable.into_iter().any(|id| {
                    !handoffs.contains_key(&id)
                        && matches!(
                            mir.block(id)
                                .expect("fixture MIR block must exist")
                                .terminator()
                                .kind(),
                            MirTerminatorKind::Return { value: None, .. }
                        )
                }),
                "normal receiver cleanup returns after its deferred handoff, abandoned={abandoned}"
            );
        }
    }
}

#[test]
fn scoped_exit_error_cancels_displaced_task_before_waiting() {
    let compilation = compilation(
        r#"
        module app;
        struct Resource {}
        impl Resource
        {
            enter() -> bool { return true; }
            exit(pos lease: bool) -> Result<unit, i32> { return Error(8); }
        }
        async func caller(pos resource: Resource, pos task: Task<unit>) -> Result<Task<unit>, i32>
        {
            return with lease = resource { yield task; };
        }
        "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .expect("the task-owning scope lowers");

    let mir = lowered_mir(&lowered);

    let error_variant = compilation.available_compiler_known_symbols().result_representation()
        .expect("fixture has the Result representation").error_variant();

    let (replacement, entry, displaced) = mir.blocks().iter().find_map(|block| {
        let MirTerminatorKind::BeginCleanup(entry) = block.terminator().kind() else {
            return None;
        };

        let mut operations = block.operations().iter().filter_map(|id| mir.operation(*id));

        let (pending, displaced) = operations.find_map(|operation| match operation.kind() {
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Move(pending), .. }
                if destination.ty() == pending.ty() => Some((pending, destination)),
            _ => None,
        })?;

        let error = operations.find_map(|operation| match operation.kind() {
            MirOperationKind::Construct(construction)
                if construction.target() == bray_bound_tree::ConstructionTarget::UnionVariant(error_variant) => operation.result(),
            _ => None,
        })?;

        operations.any(|operation| matches!(operation.kind(),
            MirOperationKind::Store { destination, value: bray_ir::MirOperand::Value(value), .. }
                if destination == pending && *value == error
        )).then_some((block, entry, displaced))
    }).expect("the Error arm replaces the pending owner and begins its cleanup");

    assert_eq!(entry.phase(), bray_ir::MirCleanupPhase::TaskCancellation);

    let broadcast = mir.block(entry.edge().target()).expect("the broadcast entry exists");

    let MirTerminatorKind::Branch { condition: bray_ir::MirOperand::Copy(guard), then_edge, .. } = broadcast.terminator().kind() else {
        panic!("broadcast tests the displaced owner's initialization guard");
    };

    assert!(replacement.operations().iter().filter_map(|id| mir.operation(*id)).any(|operation| matches!(operation.kind(),
        MirOperationKind::Store { destination, value: bray_ir::MirOperand::Immediate { value: bray_ir::MirImmediateValue::Boolean(true), .. }, .. }
            if destination == guard
    )), "replacement initializes the cleanup guard");

    let cancellation_block = mir.block(then_edge.target()).expect("the initialized owner enters broadcast");

    let collection_for = |block: &bray_ir::MirBlock, expected| {
        block.operations().iter().filter_map(|id| mir.operation(*id)).find_map(|operation| match operation.kind() {
            MirOperationKind::Cleanup { phase, place, collection: Some(collection) }
                if *phase == expected && place == displaced => Some(collection),
            _ => None,
        }).expect("cleanup retains the displaced owner and its report collection")
    };

    let cancellation = collection_for(cancellation_block, bray_ir::MirCleanupPhase::TaskCancellation);

    let resolution = std::iter::successors(Some(cancellation_block), |block| {
        match block.terminator().kind() {
            MirTerminatorKind::Goto(edge) | MirTerminatorKind::CheckCallOutcome { completed: edge, .. } => mir.block(edge.target()),
            _ => None,
        }
    }).take(mir.blocks().len()).find_map(|block| match block.terminator().kind() {
        MirTerminatorKind::ContinueCleanup(edge) if edge.phase() == bray_ir::MirCleanupPhase::LifecycleResolution => mir.block(edge.edge().target()),
        _ => None,
    }).expect("broadcast completion explicitly starts resolution");

    let MirTerminatorKind::Branch { condition: bray_ir::MirOperand::Copy(resolution_guard), then_edge, .. } = resolution.terminator().kind() else {
        panic!("resolution tests whether the displaced owner is still initialized");
    };

    assert_eq!(resolution_guard, guard);

    let resolution_block = mir.block(then_edge.target()).expect("the displaced owner enters resolution");
    let resolution = collection_for(resolution_block, bray_ir::MirCleanupPhase::LifecycleResolution);

    assert_eq!(cancellation.places(), resolution.places());
    assert!(resolution.retention().is_some());
}

#[test]
fn fallible_scoped_exit_retains_its_error_quiescence_continuation() {
    for execution in ["", "async "] {
        for body in ["return Ok(unit);", r#"panic("");"#] {
            let compilation = compilation(&format!(
                r#"
                module app;
                async func pending() {{}}
                struct Failure {{ child: Future<unit>; }}
                struct Resource {{}}
                impl Resource
                {{
                    enter() -> bool {{ return true; }}
                    {execution}exit(pos lease: bool) -> Result<unit, Failure>
                    {{
                        return Error(Failure {{ child = pending() }});
                    }}
                }}
                async func caller(pos resource: Resource) -> Result<unit, Failure>
                {{
                    _ = with lease = resource {{ {body} }};

                    return Ok(unit);
                }}
                "#,
            ));

            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.is_empty(),
                "{execution}, {body}: {diagnostics:?}"
            );

            let key = source_function_body_key(&compilation, "caller");

            let lowered = compilation
                .lowered_unit(key)
                .expect("fixture callable must lower successfully");

            let mir = lowered_mir(&lowered);
            let mut quiescence = 0;

            for operation in mir.operations() {
                if let MirOperationKind::Quiesce { collection, .. } = operation.kind() {
                    quiescence += 1;

                    assert!(
                        collection
                            .as_ref()
                            .expect("fixture optional value must be present")
                            .retention()
                            .is_some(),
                        "{execution}, {body}: scoped error quiescence retains its caller continuation"
                    );
                }
            }

            assert!(
                quiescence > 0,
                "fallible exit retains owned error quiescence"
            );
        }
    }
}

#[test]
fn scoped_error_quiescence_requires_an_async_caller_even_for_sync_exit() {
    let compilation = compilation(
        r#"
        module app;
        async func pending() {}
        struct Failure { child: Future<unit>; }
        struct Resource {}
        impl Resource
        {
            enter() -> bool { return true; }
            exit(pos lease: bool) -> Result<unit, Failure>
            {
                return Error(Failure { child = pending() });
            }
        }
        func caller(pos resource: Resource) -> Result<unit, Failure>
        {
            _ = with lease = resource { return Ok(unit); };

            return Ok(unit);
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
    );
}

#[test]
fn scoped_structural_cleanup_retires_the_source_initialization_guard() {
    let compilation = compilation(
        r#"
        module app;

        struct Resource
        {
            enter() -> bool
            {
                return true;
            }

            exit(pos capability: bool) {}
        }

        struct Local
        {
            marker: usize;

            destruct()
            {
                _ = self.marker;
            }
        }

        async func caller(pos resource: Resource)
        {
            with capability = resource
            {
                let local = Local
                {
                    marker = 619
                };
            };
        }
        "#,
    );

    let key = source_function_body_key(&compilation, "caller");

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered_mir(&lowered);
    let mut checked = 0;

    assert!(mir.is_valid());

    for block in mir.blocks() {
        for (index, id) in block.operations().iter().enumerate() {
            let operation = mir
                .operation(*id)
                .expect("fixture MIR operation must exist");

            let bray_ir::MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                place,
                ..
            } = operation.kind()
            else {
                continue;
            };

            if !matches!(
                mir.storage(place.storage())
                    .expect("fixture `storage` call must succeed")
                    .kind(),
                bray_ir::MirStorageKind::Local
            ) {
                continue;
            }

            assert!(
                block.operations()[..index]
                    .last()
                    .is_some_and(|id| matches!(
                        mir.operation(*id)
                            .expect("fixture MIR operation must exist")
                            .kind(),
                        bray_ir::MirOperationKind::Store {
                            value: bray_ir::MirOperand::Immediate {
                                value: bray_ir::MirImmediateValue::Boolean(false),
                                ..
                            },
                            ..
                        }
                    )),
                "source ownership must retire before structural cleanup can fail or suspend"
            );

            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "the scoped body owns a local requiring cleanup"
    );
}

#[test]
fn scoped_result_completion_reaches_the_following_return() {
    for execution in ["", "async "] {
        for fallible in [false, true] {
            let result = if fallible {
                "Result<bool, unit>"
            } else {
                "bool"
            };

            let entered = if fallible { "Ok(true)" } else { "true" };

            let source = format!(
                r#"
                module app;

                struct Resource
                {{
                    {execution}enter() -> {result}
                    {{
                        return {entered};
                    }}

                    {execution}exit(pos capability: bool) {{}}
                }}

                {execution}func caller() -> usize
                {{
                    _ = with capability = Resource {{}}
                    {{
                        yield unit;
                    }};

                    return 619;
                }}
                "#,
            );

            let compilation = compilation(&source);

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "caller"))
                .expect("fixture callable must lower successfully");

            let mir = lowered_mir(&lowered);

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            assert!(mir.is_valid());

            let returns = if execution.is_empty() {
                mir.blocks().iter().any(|block| {
                    matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::Return { value: Some(_), .. }
                    )
                })
            } else {
                mir.operations().iter().any(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
                            state: bray_ir::MirTaskTerminalState::Completed { .. },
                            ..
                        })
                    )
                })
            };

            assert!(
                returns,
                "the checked with result must continue to its following return: {source}"
            );
        }
    }
}

#[test]
fn discard_spellings_preserve_owned_storage_and_lifecycle_requirements() {
    for prefix in ["let _", "_"] {
        for (declaration, expected) in [
            ("struct Value { destruct() {} }", None),
            (
                "struct Value { finalize() -> Result<unit, bool> executes(total) { return Error(true); } }",
                Some(bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization),
            ),
            (
                "struct Value { async finalize() executes(total) {} }",
                Some(
                    bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
                ),
            ),
        ] {
            let source = format!(
                r#"
                module app;
                {declaration}
                func root(pos value: Value)
                {{
                    {prefix} = value;
                }}
            "#
            );

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "root");

            let lowered = compilation
                .lowered_unit(key.clone())
                .expect("fixture callable must lower successfully");

            match expected {
                Some(kind) => {
                    bray_testing::assert_goal_state_diagnostic_kind(lowered.diagnostics(), kind);
                    assert!(lowered.value().is_none());
                }
                None => {
                    assert!(lowered.diagnostics().is_empty(), "{source}: {lowered:#?}");
                    assert!(lowered.value().is_some());

                    let storage = compilation
                        .storage_plan(key)
                        .expect("fixture callable must have a storage plan");

                    assert!(
                        storage
                            .value()
                            .bindings()
                            .iter()
                            .any(|(target, _)| matches!(
                                target,
                                bray_bound_tree::StorageBindingTarget::PatternDiscard(_)
                            ))
                    );
                }
            }
        }
    }
}

#[test]
fn discard_spellings_preserve_copy_borrow_and_trusted_expression_rules() {
    for prefix in ["let _", "_"] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate admitted(value: i32);

            trusted func number(pos value: i32) -> i32
                requires(trusted admitted(value))
            {{
                return value;
            }}

            func root(pos value: i32)
                requires(trusted admitted(value))
            {{
                {prefix} = value;
                {prefix} = &value;
                {prefix} = trusted number(value);
                let copy = value;
            }}
        "#
        );

        let compilation = compilation(&source);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "root"))
            .expect("fixture callable must lower successfully");

        assert!(lowered.diagnostics().is_empty(), "{source}: {lowered:#?}");

        let number = crate::test_support::source_function(&compilation, "number").into();
        let mir = lowered_mir(&lowered);

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Direct(reference)
                if reference.instance().definition().callable_symbol() == number)
        )), "{source}: {mir:?}");
    }
}
