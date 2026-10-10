use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{CheckedMemoryOperationKind, SemanticSelection};
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirBinaryOperator, MirNullableQueryKind, MirOperationKind};

#[test]
fn sequence_method_references_cannot_initialize_their_result_type() {
    let compilation = compilation(
        r#"module app;

func inspect(pos slice: &[u8]) -> usize
{
    let count: usize = slice.length;
    return count;
}
"#,
    );

    assert!(compilation.check_diagnostics().by_kind(
        bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType
    ).next().is_some());

    let key = source_callable_body_key(&compilation);
    let lowered = compilation.lowered_unit(key).expect("invalid source must retain diagnostics");
    assert!(lowered.diagnostics().has_errors());
    assert!(lowered.value().is_none());
}

#[test]
fn slices_and_fixed_arrays_select_and_lower_sequence_operations() {
    let compilation = compilation(
        r#"module app;

func inspect(pos slice: &[u8], pos array: [u8; 4]) -> (usize, bool, usize, bool)
    executes(pure)
{
    return (slice.length(), slice.is_empty(), array.length(), array.is_empty());
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("sequence operations must be selectable: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:#?}",
        selections.diagnostics()
    );

    let hooks = selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            SemanticSelection::Call(call) => call.implementation_hook(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        hooks,
        [
            ImplementationHook::SequenceLength,
            ImplementationHook::SequenceIsEmpty,
            ImplementationHook::SequenceLength,
            ImplementationHook::SequenceIsEmpty,
        ]
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("sequence operations must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let Some(mir) = lowered.value().as_ref().and_then(|unit| unit.mir()) else {
        panic!("sequence operations must produce MIR");
    };

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::Memory(memory)
                    if memory.kind() == CheckedMemoryOperationKind::SequenceLength
            ))
            .count(),
        4
    );

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::Binary {
                    operator: MirBinaryOperator::Equal,
                    ..
                }
            ))
            .count(),
        2
    );
}

#[test]
fn nullable_values_select_and_lower_state_queries() {
    let compilation = compilation(
        r#"module app;

func inspect<T>(pos value: T?, pos borrowed: &(T?)) -> (bool, bool, bool, bool)
{
    return (
        value.is_present(),
        value.is_absent(),
        borrowed.is_present(),
        borrowed.is_absent(),
    );
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("nullable queries must be selectable: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:#?}",
        selections.diagnostics()
    );

    let hooks = selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            SemanticSelection::Call(call) => call.implementation_hook(),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        hooks,
        [
            ImplementationHook::NullableIsPresent,
            ImplementationHook::NullableIsAbsent,
            ImplementationHook::NullableIsPresent,
            ImplementationHook::NullableIsAbsent,
        ]
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("nullable queries must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let Some(mir) = lowered.value().as_ref().and_then(|unit| unit.mir()) else {
        panic!("nullable queries must produce MIR");
    };

    let queries = mir
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::NullableQuery(query) => {
                Some((query.kind(), query.operand_type(), query.nullable_type()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(queries.len(), 4);
    assert_eq!(queries[0].0, MirNullableQueryKind::IsPresent);
    assert_eq!(queries[1].0, MirNullableQueryKind::IsAbsent);
    assert_eq!(queries[2].0, MirNullableQueryKind::IsPresent);
    assert_eq!(queries[3].0, MirNullableQueryKind::IsAbsent);

    assert!(
        queries
            .iter()
            .all(|(_, operand, nullable)| operand != nullable)
    );
}

#[test]
fn nullable_state_queries_are_not_available_on_non_nullable_values() {
    let compilation = compilation(
        r#"module app;

func inspect(pos value: i32) -> bool
{
    return value.is_present();
}
"#,
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("invalid nullable query must remain checkable: {error:?}"));

    assert!(lowered.value().is_none());
    assert!(lowered.diagnostics().has_errors());
}

#[test]
fn concrete_nullable_state_queries_lower_for_native_code_generation() {
    let compilation = compilation(
        r#"module app;

func main() -> bool
{
    let value: i32? = 1;

    return value.is_present();
}
"#,
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("concrete nullable query must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}
