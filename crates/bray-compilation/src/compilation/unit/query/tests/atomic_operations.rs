use crate::test_support::{
    compilation, compilation_with_target_operations, source_callable_body_key,
    source_function_body_key,
};
use bray_bound_tree::CheckedMemoryOperationKind;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn invalid_atomic_orders_publish_structured_diagnostics_before_lowering() {
    let compilation = compilation(
        r#"module app;

using core.atomic;
func main()
{
    let storage = core.atomic.initialize<u32>(1);
    let value = core.atomic.load<u32, 2>(&storage);
}
"#,
    );

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
    let compilation = compilation(
        r#"module app;

using core.atomic;
func initialize<T>(pos value: T) -> core.atomic.Atomic<T>
{
    return core.atomic.initialize<T>(value);
}
"#,
    );

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
        r#"module app;

using core.atomic;
func main()
{
    let storage = core.atomic.initialize<u128>(1);
}
"#,
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
        r#"module app;

using core.atomic;
func main()
{
    let storage = core.atomic.initialize<r32>(1.0);
}
"#,
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
        r#"module app;

using core.atomic;
@copy
@layout(transparent)
struct Counter
{
    value: u32;
}
func main()
{
    let storage = core.atomic.initialize<Counter>(Counter { value = 1 });
    let previous = core.atomic.fetch_add<Counter, 0>(
        &storage, Counter { value = 2 });
}
"#,
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
    let compilation = compilation(
        r#"module app;

using core.atomic;
func main()
{
    let storage = core.atomic.initialize<usize>(1);
    let previous = core.atomic.fetch_add<usize, 0>(&storage, 2);
}
"#,
    );

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
    let compilation = compilation(
        r#"module app;

using core.atomic;
@copy
@layout(stable)
struct Pair
{
    low: u16;
    high: u16;
}
func main()
{
    let storage = core.atomic.initialize<Pair>(Pair { low = 1, high = 2 });
    let value = core.atomic.load<Pair, 1>(&storage);
    let prior = core.atomic.exchange<Pair, 3>(
        &storage, Pair { low = 3, high = 4 });
}
"#,
    );

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
    let compilation = compilation(
        r#"module app;

using core.atomic;
@copy
@layout(stable)
struct Padded
{
    first: u8;
    second: u32;
}
func main()
{
    let storage = core.atomic.initialize<Padded>(
        Padded { first = 1, second = 2 });
}
"#,
    );

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
    let compilation = compilation(
        r#"module app;

using core.atomic;
@copy
@layout(stable)
struct Triple
{
    first: u8;
    second: u8;
    third: u8;
}
func main()
{
    let storage = core.atomic.initialize<Triple>(
        Triple { first = 1, second = 2, third = 3 });
}
"#,
    );

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("unsupported atomic storage must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}

#[test]
fn atomic_value_initialization_is_pure_but_observation_and_mutation_are_not() {
    for (operation, valid) in [
        ("return core.atomic.initialize<u32>(7);", true),
        (
            "_ = core.atomic.load<u32, 0>(existing); return core.atomic.initialize<u32>(7);",
            false,
        ),
        (
            "core.atomic.store<u32, 0>(existing, 7); return core.atomic.initialize<u32>(7);",
            false,
        ),
    ] {
        let application = compilation(&format!(
            r#"module app;

using core.atomic;

 func root(pos existing: &core.atomic.Atomic<u32>) -> core.atomic.Atomic<u32> executes(pure, total) {{ {operation} }}"#
        ));

        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{operation}: {diagnostics:#?}"
        );

        if !valid {
            assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}
