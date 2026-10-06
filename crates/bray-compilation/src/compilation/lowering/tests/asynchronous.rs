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

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::PublishTerminalState {
            state: bray_ir::MirTaskTerminalState::Cancelled,
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

    let guards = mir
        .storages_with_ids()
        .filter_map(|(id, storage)| {
            if storage.kind() != &bray_ir::MirStorageKind::Local {
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

    assert!(
        mir.blocks()
            .iter()
            .filter(|block| block.kind() != bray_ir::MirBlockKind::Ordinary)
            .all(|block| !matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. })),
        "cleanup array loops must not suspend with an unretained counter: {mir:?}"
    );
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
