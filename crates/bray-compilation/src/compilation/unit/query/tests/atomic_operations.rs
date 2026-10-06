use crate::test_support::{
    compilation, compilation_with_target_operations, source_callable_body_key,
    source_function_body_key,
};
use bray_bound_tree::CheckedMemoryOperationKind;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn invalid_atomic_orders_publish_structured_diagnostics_before_lowering() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let storage = core.atomic.initialize<u32>(1);\n",
        "    let value = core.atomic.load<u32, 2>(&storage);\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

    assert_eq!(operations.value().operations().len(), 1);

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingInvalidAtomicMemoryOrder,
    );
}

#[test]
fn open_generic_atomic_operations_defer_representation_validation() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func initialize<T>(pos value: T) -> core.atomic.Atomic<T>\n",
        "{\n",
        "    return core.atomic.initialize<T>(value);\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_function_body_key(&compilation, "initialize"))
        .unwrap_or_else(|error| panic!("open generic atomic operation must publish: {error:?}"));

    assert!(
        operations.diagnostics().is_empty(),
        "{:#?}",
        operations.diagnostics()
    );

    assert_eq!(operations.value().operations().len(), 1);

    assert!(matches!(
        operations.value().operations()[0].kind(),
        CheckedMemoryOperationKind::AtomicInitialize { .. }
    ));
}

#[test]
fn unavailable_atomic_representations_fail_before_lowering() {
    let compilation = compilation_with_target_operations(
        concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<u128>(1);\n",
            "}\n",
        ),
        true,
        true,
    );

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}

#[test]
fn non_atomic_scalar_representations_fail_before_lowering() {
    let compilation = compilation_with_target_operations(
        concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<r32>(1.0);\n",
            "}\n",
        ),
        true,
        true,
    );

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}

#[test]
fn transparent_integer_representations_support_atomic_fetch_operations() {
    let compilation = compilation_with_target_operations(
        concat!(
            "module app;\n",
            "@copy\n",
            "@layout(transparent)\n",
            "struct Counter\n",
            "{\n",
            "    value: u32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<Counter>(Counter { value = 1 });\n",
            "    let previous = core.atomic.fetch_add<Counter, 0>(\n",
            "        &storage, Counter { value = 2 });\n",
            "}\n",
        ),
        true,
        true,
    );

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("transparent atomic operations must publish: {error:?}"));

    assert!(
        operations.diagnostics().is_empty(),
        "{:#?}",
        operations.diagnostics()
    );

    assert_eq!(operations.value().operations().len(), 2);

    assert!(matches!(
        operations.value().operations()[1].kind(),
        CheckedMemoryOperationKind::AtomicFetch {
            kind: bray_bound_tree::AtomicFetchKind::Add,
            ..
        }
    ));
}

#[test]
fn target_sized_integer_fetch_uses_integer_atomic_representation() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let storage = core.atomic.initialize<usize>(1);\n",
        "    let previous = core.atomic.fetch_add<usize, 0>(&storage, 2);\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("target-sized atomic operations must publish: {error:?}"));

    assert!(
        operations.diagnostics().is_empty(),
        "{:#?}",
        operations.diagnostics()
    );

    assert_eq!(operations.value().operations().len(), 2);

    assert!(matches!(
        operations.value().operations()[1].kind(),
        CheckedMemoryOperationKind::AtomicFetch {
            kind: bray_bound_tree::AtomicFetchKind::Add,
            ..
        }
    ));
}

#[test]
fn plain_storage_representations_support_non_fetch_atomic_operations() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@copy\n",
        "@layout(stable)\n",
        "struct Pair\n",
        "{\n",
        "    low: u16;\n",
        "    high: u16;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let storage = core.atomic.initialize<Pair>(Pair { low = 1, high = 2 });\n",
        "    let value = core.atomic.load<Pair, 1>(&storage);\n",
        "    let prior = core.atomic.exchange<Pair, 3>(\n",
        "        &storage, Pair { low = 3, high = 4 });\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("plain-storage atomic operations must publish: {error:?}"));

    assert!(
        operations.diagnostics().is_empty(),
        "{:#?}",
        operations.diagnostics()
    );

    assert_eq!(operations.value().operations().len(), 3);
}

#[test]
fn padded_plain_storage_atomic_representations_fail_before_lowering() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@copy\n",
        "@layout(stable)\n",
        "struct Padded\n",
        "{\n",
        "    first: u8;\n",
        "    second: u32;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let storage = core.atomic.initialize<Padded>(\n",
        "        Padded { first = 1, second = 2 });\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("padded atomic storage must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}

#[test]
fn unsupported_atomic_plain_storage_sizes_fail_before_lowering() {
    let compilation = compilation(concat!(
        "module app;\n",
        "@copy\n",
        "@layout(stable)\n",
        "struct Triple\n",
        "{\n",
        "    first: u8;\n",
        "    second: u8;\n",
        "    third: u8;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let storage = core.atomic.initialize<Triple>(\n",
        "        Triple { first = 1, second = 2, third = 3 });\n",
        "}\n",
    ));

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("unsupported atomic storage must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}
