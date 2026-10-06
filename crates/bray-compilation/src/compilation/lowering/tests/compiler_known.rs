use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundUnitKey, CheckedMemoryOperationKind, OperatorTarget, SelectedOperation, SemanticSelection,
};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBinaryOperator, MirCallIntrinsic, MirCallTarget, MirOperationKind, MirPanicCause,
    MirTerminatorKind, MirTextOperationKind,
};
use bray_symbols::BorrowKind;

use super::support::{lowered_mir, standard_text_compilation};
use crate::Compilation;
use crate::test_support::{
    compilation_with_target_operations, source_callable_body_key, source_function_body_key,
    source_trait_callable_fulfillment_body_key, source_type_callable_member_body_key,
};

#[test]
fn qualified_compiler_provided_memory_calls_lower_to_explicit_mir() {
    let compilation = compilation_with_target_operations(
        concat!(
            "trusted module app;\n",
            "trusted func main() uses(manual_alloc)\n",
            "{\n",
            "    let pointer = trusted core.memory.allocate(bytes = 0, align = 1);\n",
            "\n",
            "    trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
            "}\n",
        ),
        true,
        true,
    );

    let result = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("memory operation MIR must be available: {error:?}"));

    let kinds = lowered_mir(&result)
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Memory(memory) => Some(memory.kind()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        [
            CheckedMemoryOperationKind::RawAllocate,
            CheckedMemoryOperationKind::RawDeallocate,
        ]
    );
}

#[test]
fn standard_text_sources_bind_primitive_hooks_and_cursor_bodies() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../xtask/fixtures/native-execution/standard-string.bray"),
        include_str!("../../../../../../xtask/fixtures/native-execution/standard-character.bray"),
        include_str!("../../../../../../xtask/fixtures/native-execution/standard-text-cursor.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for name in ["exercise_text_operations", "exercise_character_operations"] {
        assert!(
            implementation_hooks(&compilation, name).is_empty(),
            "{name}"
        );
    }

    let text_selections = compilation
        .semantic_selections(source_function_body_key(
            &compilation,
            "exercise_text_operations",
        ))
        .unwrap_or_else(|error| panic!("text selections must be available: {error:?}"));

    let string_operator_targets = text_selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            SemanticSelection::Operation(SelectedOperation::Operator { target, .. }) => {
                Some(target)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(string_operator_targets.len(), 2);

    assert!(
        string_operator_targets
            .iter()
            .all(|target| matches!(target, OperatorTarget::BuiltIn(_)))
    );

    let text_operations = compilation
        .lowered_unit(source_function_body_key(
            &compilation,
            "exercise_text_operations",
        ))
        .unwrap_or_else(|error| panic!("text operations must lower: {error:?}"));

    assert_eq!(
        lowered_mir(&text_operations)
            .operations()
            .iter()
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Text(text)
                        if text.kind() == MirTextOperationKind::Equals
                )
            })
            .count(),
        2,
    );

    let string_hooks = [
        ("length", ImplementationHook::StringScalarCount),
        ("is_empty", ImplementationHook::StringIsEmpty),
        ("get", ImplementationHook::StringScalarAt),
        ("slice", ImplementationHook::StringScalarSlice),
        ("as_bytes", ImplementationHook::StringUtf8),
        ("from_utf8", ImplementationHook::StringFromUtf8),
    ];

    let character_hooks = [
        ("code_point", ImplementationHook::CharacterScalarValue),
        (
            "from_code_point",
            ImplementationHook::CharacterFromScalarValue,
        ),
        ("is_alphabetic", ImplementationHook::CharacterIsAlphabetic),
        ("is_numeric", ImplementationHook::CharacterIsNumeric),
        ("is_whitespace", ImplementationHook::CharacterIsWhitespace),
    ];

    let encode_utf8_hooks = implementation_hooks_for_key(
        &compilation,
        "encode_utf8",
        source_type_callable_member_body_key(&compilation, "encode_utf8"),
    );

    assert_eq!(
        encode_utf8_hooks,
        [
            ImplementationHook::CharacterUtf8Length,
            ImplementationHook::CharacterUtf8Byte,
        ]
    );

    let equals_hooks = implementation_hooks_for_key(
        &compilation,
        "equals",
        source_trait_callable_fulfillment_body_key(&compilation, "equals"),
    );

    assert_eq!(equals_hooks, [ImplementationHook::StringEquals]);

    let primitive_hooks = string_hooks
        .into_iter()
        .chain(character_hooks)
        .flat_map(|(name, expected)| {
            let key = source_type_callable_member_body_key(&compilation, name);
            let hooks = implementation_hooks_for_key(&compilation, name, key);

            assert_eq!(hooks, [expected], "{name}");

            hooks
        })
        .chain(encode_utf8_hooks)
        .chain(equals_hooks)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        primitive_hooks,
        BTreeSet::from([
            ImplementationHook::StringScalarCount,
            ImplementationHook::StringIsEmpty,
            ImplementationHook::StringEquals,
            ImplementationHook::StringScalarAt,
            ImplementationHook::StringScalarSlice,
            ImplementationHook::StringUtf8,
            ImplementationHook::StringFromUtf8,
            ImplementationHook::CharacterScalarValue,
            ImplementationHook::CharacterFromScalarValue,
            ImplementationHook::CharacterUtf8Length,
            ImplementationHook::CharacterUtf8Byte,
            ImplementationHook::CharacterIsAlphabetic,
            ImplementationHook::CharacterIsNumeric,
            ImplementationHook::CharacterIsWhitespace,
        ])
    );

    assert!(
        implementation_hooks_for_key(
            &compilation,
            "characters",
            source_type_callable_member_body_key(&compilation, "characters"),
        )
        .is_empty()
    );

    let next_key = source_trait_callable_fulfillment_body_key(&compilation, "next");

    assert!(implementation_hooks_for_key(&compilation, "next", next_key).is_empty());

    for (name, key, expected) in [
        (
            "length",
            source_type_callable_member_body_key(&compilation, "length"),
            MirTextOperationKind::ScalarCount,
        ),
        (
            "get",
            source_type_callable_member_body_key(&compilation, "get"),
            MirTextOperationKind::ScalarAt,
        ),
    ] {
        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("{name} must lower through its Bray body: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let operations = lowered_mir(&lowered)
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Text(text) => Some(text.kind()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(operations, [expected], "{name}");
    }
}

#[test]
fn standard_numeric_truncation_lowers_explicitly_with_an_inferred_source_type() {
    let compilation = standard_text_compilation(&[concat!(
        "module std.numeric;\n",
        "\n",
        "func truncate(pos value: u128) -> u8\n",
        "{\n",
        "    return std.truncate_to<u8>(value);\n",
        "}\n",
    )]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "truncate");

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("numeric truncation must lower: {error:?}"));

    assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
        matches!(
            operation.kind(),
            MirOperationKind::NumericConversion {
                kind: bray_ir::MirNumericConversionKind::Truncate,
                ..
            }
        )
    }));
}

#[test]
fn trait_qualified_standard_text_calls_lower_to_direct_calls() {
    let compilation = standard_text_compilation(&[include_str!(
        "../../../../../../xtask/fixtures/native-execution/standard-text-cursor.bray"
    )]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .unwrap_or_else(|error| panic!("standard text cursor must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let call_targets = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Call(call) if !matches!(call.target(), MirCallTarget::Runtime(_)) => {
                Some(call.target())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(call_targets.len(), 5);

    assert!(
        call_targets
            .iter()
            .all(|target| matches!(target, MirCallTarget::Direct(_)))
    );

    let mutable_receiver_borrows = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Mutable,
                    ..
                }
            )
        })
        .count();

    assert_eq!(mutable_receiver_borrows, 4);
}

#[test]
fn direct_compiler_known_trait_calls_retain_builtin_intrinsics() {
    let compilation = standard_text_compilation(&[concat!(
        "module std.direct_trait_call;\n",
        "func generic_equal<T>(pos left: &T, pos right: &T) -> bool\n",
        "    with(T: Equatable<T>)\n",
        "{\n",
        "    return left.equals(right);\n",
        "}\n",
    )]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "generic_equal"))
        .unwrap_or_else(|error| panic!("generic equality call must lower: {error:?}"));

    let calls = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Call(call) => Some(call),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(calls.len(), 1);
    assert!(calls[0].trait_dispatch().is_some());

    assert_eq!(
        calls[0].intrinsic(),
        Some(MirCallIntrinsic::Binary(MirBinaryOperator::Equal))
    );
}

#[test]
fn standard_run_utilities_lower_to_current_run_operations() {
    let compilation = standard_text_compilation(&[include_str!(
        "../../../../../../standard-library/std/src/run.bray"
    )]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    assert_eq!(
        implementation_hooks(&compilation, "cancellation_requested"),
        [ImplementationHook::CurrentRunCancellationObservation]
    );

    assert_eq!(
        implementation_hooks(&compilation, "checkpoint"),
        [
            ImplementationHook::CurrentRunCancellationObservation,
            ImplementationHook::CurrentRunCancellationPropagation,
        ]
    );

    let observation = compilation
        .lowered_unit(source_function_body_key(
            &compilation,
            "cancellation_requested",
        ))
        .unwrap_or_else(|error| panic!("cancellation observation must lower: {error:?}"));

    assert!(observation.diagnostics().is_empty());

    assert!(
        lowered_mir(&observation)
            .operations()
            .iter()
            .any(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::ObserveCurrentRunCancellation { .. }
                    )
                )
            })
    );

    let checkpoint = compilation
        .lowered_unit(source_function_body_key(&compilation, "checkpoint"))
        .unwrap_or_else(|error| panic!("cancellation checkpoint must lower: {error:?}"));

    assert!(checkpoint.diagnostics().is_empty());

    assert!(lowered_mir(&checkpoint).blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::PropagateCancellation { .. }
        )
    }));
}

#[test]
fn standard_testing_failure_lowers_to_structured_failure_control() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../standard-library/std/src/testing.bray"),
        concat!(
            "module std.testing;\n",
            "\n",
            "func exercise(pos message: &string) -> never\n",
            "{\n",
            "    fail(message);\n",
            "}\n",
            "\n",
            "func catch_failure(pos message: &string) -> Result<never, PanicReport>\n",
            "{\n",
            "    return catch fail(message);\n",
            "}\n",
        ),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    assert_eq!(
        implementation_hooks(&compilation, "exercise"),
        [ImplementationHook::TestingFail]
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "exercise"))
        .unwrap_or_else(|error| panic!("testing failure must lower: {error:?}"));

    assert!(lowered.diagnostics().is_empty());

    assert!(lowered_mir(&lowered).operations().iter().any(|operation| {
        matches!(
            operation.kind(),
            MirOperationKind::PanicReport(MirPanicCause::ExplicitTestFailure(_))
        )
    }));

    assert!(lowered_mir(&lowered).blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::BeginCleanup(_) | MirTerminatorKind::Panic { .. }
        )
    }));

    let caught = compilation
        .lowered_unit(source_function_body_key(&compilation, "catch_failure"))
        .unwrap_or_else(|error| panic!("caught testing failure must lower: {error:?}"));

    assert!(caught.diagnostics().is_empty());

    assert!(
        lowered_mir(&caught)
            .blocks()
            .iter()
            .any(|block| { matches!(block.terminator().kind(), MirTerminatorKind::Panic { .. }) })
    );
}

#[test]
fn standard_task_yield_is_an_asynchronous_computation() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../standard-library/std/src/run.bray"),
        include_str!("../../../../../../standard-library/std/src/task.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "yield_now"))
        .unwrap_or_else(|error| panic!("task yield must lower: {error:?}"));

    assert!(lowered.diagnostics().is_empty());

    assert!(matches!(
        lowered_mir(&lowered).kind(),
        bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
    ));

    assert_eq!(
        implementation_hooks(&compilation, "yield_now"),
        [ImplementationHook::TaskYield]
    );

    assert!(lowered_mir(&lowered).blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::Suspend {
                kind: bray_ir::MirSuspensionKind::Yield,
                ..
            }
        )
    }));
}

#[test]
fn standard_task_events_lower_creation_and_waiting() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../standard-library/std/src/run.bray"),
        include_str!("../../../../../../standard-library/std/src/task.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let creation_key = source_function_body_key(&compilation, "event");

    let creation = compilation
        .lowered_unit(creation_key)
        .unwrap_or_else(|error| panic!("task event creation must lower: {error:?}"));

    assert!(creation.diagnostics().is_empty());

    let wait = compilation
        .lowered_unit(source_function_body_key(&compilation, "wait"))
        .unwrap_or_else(|error| panic!("task event wait must lower: {error:?}"));

    assert!(wait.diagnostics().is_empty());

    assert!(matches!(
        lowered_mir(&wait).kind(),
        bray_ir::MirUnitKind::ProtectedAsyncFrame(_)
    ));

    assert!(lowered_mir(&wait).blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::Suspend {
                kind: bray_ir::MirSuspensionKind::TaskEvent,
                payload: Some(_),
                ..
            }
        )
    }));
}

fn implementation_hooks(compilation: &Compilation, name: &str) -> Vec<ImplementationHook> {
    let key = source_function_body_key(compilation, name);

    implementation_hooks_for_key(compilation, name, key)
}

fn implementation_hooks_for_key(
    compilation: &Compilation,
    name: &str,
    key: BoundUnitKey,
) -> Vec<ImplementationHook> {
    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| {
            panic!("standard text selections for {name} must be available: {error:?}")
        });

    assert!(
        selections.diagnostics().is_empty(),
        "{name}: {:#?}",
        selections.diagnostics()
    );

    selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            SemanticSelection::Call(call) => call.implementation_hook(),
            _ => None,
        })
        .collect()
}
