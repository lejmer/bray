use bray_ir::{MirOperand, MirOperationKind, MirTerminatorKind};
use bray_runtime_interface::ExecutionLaneRequirement;
use bray_symbols::TypeData;

use super::support::lowered_mir;
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

const ASYNC_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "async func main() -> i32\n",
    "{\n",
    "    let retained: i32 = 1;\n",
    "    let pending = child();\n",
    "    let ignored: i32 = await pending;\n",
    "\n",
    "    return retained;\n",
    "}\n",
    "\n",
    "async func child() -> i32\n",
    "{\n",
    "    return 1;\n",
    "}\n",
);

#[test]
fn construction_inputs_survive_a_later_await() {
    for (input_type, initializer, result_type) in [
        (
            "i32",
            "Value { first = 17, second = await pending }",
            "Value",
        ),
        (
            "Guard",
            "Value { first = Guard {}, second = await pending }",
            "Value",
        ),
        ("i32", "(17, await pending)", "(i32, i32)"),
        ("Guard", "(Guard {}, await pending)", "(Guard, i32)"),
        ("i32", "[17, await pending]", "[i32; 2]"),
    ] {
        let source = format!(
            "module app; struct Guard {{ destruct() {{}} }} struct Value {{ first: {input_type}; second: i32; }} \
                 async func build(pos pending: Future<i32>) -> {result_type} {{ return {initializer}; }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "build"))
            .expect("pending construction must lower across suspension");

        let mir = lowered_mir(&result);

        let first = mir
            .operations()
            .iter()
            .find_map(|operation| {
                let first = match operation.kind() {
                    MirOperationKind::Construct(construction)
                        if construction.inputs().len() == 2 =>
                    {
                        construction.inputs()[0].value()
                    }
                    MirOperationKind::Aggregate(aggregate) if aggregate.operands().len() == 2 => {
                        &aggregate.operands()[0]
                    }
                    _ => return None,
                };

                let MirOperand::Move(first) = first else {
                    return None;
                };

                Some(first.storage())
            })
            .expect("the final construction must consume its retained input");

        let frame = mir
            .frame_descriptor()
            .expect("async construction has a frame");

        assert!(
            frame
                .states()
                .iter()
                .skip(1)
                .any(|state| state.initialized_storages().contains(&first)),
            "the initialized {input_type} input must survive the later await"
        );
    }
}

#[test]
fn async_callables_lower_to_protected_frames_and_explicit_suspension() {
    let compilation = compilation(ASYNC_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("async MIR must publish: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(matches!(
        mir.kind(),
        bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
    ));

    let Some(frame) = mir.frame_descriptor() else {
        panic!("async callable MIR must carry a protected-frame descriptor");
    };

    assert_eq!(frame.states().len(), 2);
    assert!(frame.completion_state().is_none());
    assert!(!frame.states()[1].initialized_storages().is_empty());

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
    )));

    assert!(
        mir.blocks()
            .iter()
            .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. }))
    );

    for block in mir.blocks() {
        let MirTerminatorKind::Suspend { resume_state, .. } = block.terminator().kind() else {
            continue;
        };

        for operation in block.operations() {
            if let MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                resume_state: admitted,
                ..
            }) = mir.operation(*operation).unwrap().kind()
            {
                assert_eq!(
                    admitted, resume_state,
                    "composition checks the suspended parent's continuation"
                );
            }
        }
    }

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
            state: bray_ir::MirTaskTerminalState::Cancelled(_),
            ..
        })
    )));
}

#[test]
fn partial_array_cleanup_flags_survive_suspension() {
    let compilation = compilation(
        r#"module app;
struct Guard { destruct() {} }
func take(pos value: Guard) {}
async func partial(pos values: [[Guard; 2]; 2], pos index: usize, pos pending: Future<i32>)
{
    take(values[index][0]);
    let _ = await pending;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "partial"))
        .unwrap();

    let mir = lowered_mir(&result);
    let frame = mir.frame_descriptor().unwrap();
    let values = compilation.semantic_value_store().unwrap();

    let boolean = compilation
        .available_compiler_known_symbols()
        .representation_symbol::<bray_symbols::StructSymbolId>(
            bray_compiler_known::RepresentationRole::ScalarBool,
        )
        .unwrap();

    let initialized_at_entry = mir
        .block(mir.entry())
        .unwrap()
        .operations()
        .iter()
        .filter_map(|id| match mir.operation(*id).unwrap().kind() {
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination,
                ..
            } => Some(destination.storage()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();

    let guards = mir
        .storages_with_ids()
        .filter_map(|(id, storage)| {
            if storage.kind() != &bray_ir::MirStorageKind::Local
                || !initialized_at_entry.contains(&id)
            {
                return None;
            }

            let mut ty = storage.ty();
            let mut dimensions = 0;

            loop {
                let data = values.type_data(ty);

                match data.as_ref() {
                    TypeData::Array { element, .. } => {
                        ty = *element;
                        dimensions += 1;
                    }
                    TypeData::Named {
                        definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                        ..
                    } if *definition == boolean => return Some((id, dimensions)),
                    _ => return None,
                }
            }
        })
        .collect::<Vec<_>>();

    assert!(
        guards.iter().any(|(_, dimensions)| *dimensions == 2),
        "nested array flags must remain represented: {mir:?}"
    );

    assert!(frame.states().len() > 1);

    for state in frame.states().iter().skip(1) {
        assert!(
            guards
                .iter()
                .all(|(guard, _)| state.initialized_storages().contains(guard)),
            "all entry-initialized flags must survive suspension: {frame:?}"
        );
    }

    for block in mir.blocks().iter().filter(|block| {
        block.kind() != bray_ir::MirBlockKind::Ordinary
            && matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. })
    }) {
        assert!(block.operations().iter().any(|id| matches!(mir.operation(*id).unwrap().kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                frame: MirOperand::Move(place), completion: None, ..
            }) if mir.storage(place.storage()).unwrap().kind() == &bray_ir::MirStorageKind::Parameter(2))),
            "only pending-future disposal may suspend, not the array cleanup loop: {mir:?}");
    }
}

#[test]
fn async_callable_frames_retain_declared_execution_lanes() {
    let compilation = compilation(concat!(
        "module app;\n",
        "async func wait()\n",
        "    requires(blocking_execution())\n",
        "{\n",
        "}\n",
    ));

    let result = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("blocking async MIR must publish: {error:?}"));

    let frame = lowered_mir(&result)
        .frame_descriptor()
        .unwrap_or_else(|| panic!("async callable must publish a frame descriptor"));

    assert!(
        frame
            .states()
            .iter()
            .all(|state| { state.lane_requirements() == [ExecutionLaneRequirement::Blocking] })
    );
}

#[test]
fn task_operations_retain_normal_completion_before_await() {
    for observation in ["join", "cancel"] {
        let source = format!(
            r#"
            module app;

            async func work() -> i32
            {{
                return 13;
            }}

            async func caller() -> RunResult<i32>
            {{
                let task = work().start();

                return await task.{observation}();
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{observation}: {diagnostics:?}");

        let result = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("task operations must lower");

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::StartTask { .. })
        )));

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )),
            "{observation} must remain reachable after task construction: {mir:?}"
        );

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::CommitAwaitedCompletion { .. })
            )),
            "{observation} must resume and transfer its result: {mir:?}"
        );
    }
}

#[test]
fn await_takes_the_child_outcome_before_cancellation_and_cleans_its_result() {
    let application = compilation(
        r#"
        module app;
        struct Guard
        {
            destruct() {}
        }
        async func child() -> Guard
        {
            return Guard {};
        }
        async func caller() -> Guard
        {
            return await child();
        }
        "#,
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .unwrap();

    let mir = lowered_mir(&lowered);

    let resumed = mir
        .block(mir.frame_descriptor().unwrap().states()[1].entry())
        .unwrap();

    assert!(resumed.operations().iter().any(|id| matches!(
        mir.operation(*id).unwrap().kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::CommitAwaitedCompletion { .. })
    )));

    assert!(!resumed.operations().iter().any(|id| matches!(
        mir.operation(*id).unwrap().kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)
    )));

    let MirTerminatorKind::CheckCallOutcome { completed, .. } = resumed.terminator().kind() else {
        panic!("child panic and cancellation must transfer before checking a new request");
    };

    let completed = mir.block(completed.target()).unwrap();

    let result = completed
        .operations()
        .iter()
        .find_map(|id| match mir.operation(*id).unwrap().kind() {
            MirOperationKind::Store {
                destination,
                value: MirOperand::Value(_),
                ..
            } => Some(destination.storage()),
            _ => None,
        })
        .expect("the completed result must become an initialized cleanup owner");

    assert!(completed.operations().iter().any(|id| matches!(
        mir.operation(*id).unwrap().kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::ObserveFrameCancellation)
    )));

    let MirTerminatorKind::Branch { then_edge, .. } = completed.terminator().kind() else {
        panic!("completed await observes cancellation before source continuation");
    };

    let mut pending = vec![then_edge.target()];
    let mut seen = std::collections::BTreeSet::new();
    let mut disposes_result = false;

    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }

        let block = mir.block(id).unwrap();

        disposes_result |= block.operations().iter().any(|id| matches!(
            mir.operation(*id).unwrap().kind(),
            MirOperationKind::Cleanup { phase: bray_ir::MirCleanupPhase::LifecycleResolution, place }
                if place.storage() == result
        ));

        block
            .terminator()
            .kind()
            .for_each_successor(|target| pending.push(target));
    }

    assert!(
        disposes_result,
        "cancelled await must dispose its transferred result"
    );
}

#[test]
fn completed_frame_result_has_its_own_cleanup_state() {
    let application = compilation(
        r#"
        module app;

        struct Guard
        {
            destruct() {}
        }

        async func produce() -> Guard
        {
            return Guard {};
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let result = application
        .lowered_unit(source_function_body_key(&application, "produce"))
        .expect("owned completion must lower");

    let mir = lowered_mir(&result);

    let frame = mir
        .frame_descriptor()
        .expect("producer has a protected frame");

    let state = frame
        .completion_state()
        .expect("owned result needs abandonment cleanup");

    let completion = frame
        .states()
        .iter()
        .find(|entry| entry.state() == state)
        .unwrap();

    let [storage] = completion.initialized_storages() else {
        panic!("completion owns just its result");
    };

    assert!(matches!(
        mir.storage(*storage).unwrap().kind(),
        bray_ir::MirStorageKind::FrameCompletion
    ));

    assert!(!frame.states()[0].initialized_storages().contains(storage));

    assert!(
        matches!(mir.block(completion.entry()).unwrap().terminator().kind(),
        MirTerminatorKind::ContinueCleanup(edge) if edge.phase() == bray_ir::MirCleanupPhase::LifecycleResolution)
    );

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::Cleanup { phase: bray_ir::MirCleanupPhase::LifecycleResolution, place }
            if place.storage() == *storage))
    );
}

#[test]
fn completed_result_cleanup_retains_its_separate_execution_phase() {
    let application = compilation(
        r#"
        module app;

        struct Guard
        {
            destruct()
                requires(blocking_execution())
            {
            }
        }

        async func produce() -> Guard
        {
            return Guard {};
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let result = application
        .lowered_unit(source_function_body_key(&application, "produce"))
        .expect("owned completion must lower");

    let frame = lowered_mir(&result).frame_descriptor().unwrap();
    let completion = frame.completion_state().unwrap();

    assert!(frame.states()[0].lane_requirements().is_empty());

    let cleanup = frame
        .states()
        .iter()
        .find(|state| state.state() == completion)
        .unwrap();

    assert!(cleanup.is_completion_cleanup());

    assert!(
        cleanup.lane_requirements().is_empty(),
        "concrete lifecycle realization owns this requirement"
    );
}
