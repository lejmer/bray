use crate::test_support::{source_function_body_key, source_input};
use crate::{Compilation, CompilationRequest};
use bray_bound_tree::CheckedMemoryOperationKind;
use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn bootstrap_pool_callbacks_reject_data_pointer_replacement() {
    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../../../runtime/bootstrap/src/records.bray"),
        include_str!("../../../../../../../runtime/bootstrap/src/panic.bray"),
        r#"module app;
        using internal bray.runtime.bootstrap;

        func poison(pos record: &mut bray.runtime.bootstrap.ReportRecord, pos pointer: RawPointer<u8>)
        {
            record.primary.release_message = pointer;
        }
        "#,
    ];

    let request = CompilationRequest::new(
        PackageIdentity::try_new("std").expect("standard library identity must be valid"),
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                source_input(
                    source,
                    u32::try_from(index).expect("fixture source index must fit u32"),
                )
            })
            .collect(),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).expect("bootstrap callback fixture must load");

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "poison"))
        .expect("invalid callback assignment must retain diagnostics");

    assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingIncompatibleExpressionType,
    );

    assert!(checked.value().is_none());
}

#[test]
fn callback_state_requires_the_exported_entry_context_parameter() {
    let compilation = standard_callback_compilation(
        r#"trusted module std.test;

@symbol(name = "valid_callback")
@abi(c)
trusted func valid_callback(pos context: RawPointer<i32>) -> bool uses(raw_memory)
{
    let state = trusted std.ffi.callback_state<i32>(context);
    return state == 0;
}

@abi(c)
trusted func unexported_callback(pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state;
}

@symbol(name = "misplaced_context")
@abi(c)
trusted func misplaced_context(pos value: i32, pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state + value;
}

@symbol(name = "bray_abi_context")
trusted func bray_abi_context(pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state;
}
"#,
    );

    let valid = compilation
        .memory_operations(source_function_body_key(&compilation, "valid_callback"))
        .unwrap_or_else(|error| panic!("valid callback memory analysis must publish: {error:?}"));

    assert!(valid.diagnostics().is_empty(), "{valid:#?}");

    assert!(matches!(
        valid.value().operations()[0].kind(),
        CheckedMemoryOperationKind::CallbackState { .. }
    ));

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "valid_callback"))
        .unwrap_or_else(|error| panic!("valid callback must lower: {error:#?}"));

    assert!(lowered.diagnostics().is_empty(), "{lowered:#?}");

    for name in [
        "unexported_callback",
        "misplaced_context",
        "bray_abi_context",
    ] {
        let invalid = compilation
            .memory_operations(source_function_body_key(&compilation, name))
            .unwrap_or_else(|error| {
                panic!("invalid callback memory analysis must publish: {error:?}")
            });

        assert!(
            invalid
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingInvalidCallbackStateContext)
                .next()
                .is_some(),
            "{name}: {invalid:#?}"
        );

        assert_goal_state_diagnostic_kind(
            invalid.diagnostics(),
            DiagnosticKind::CheckingInvalidCallbackStateContext,
        );
    }
}

fn standard_callback_compilation(source: &str) -> Compilation {
    let package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard library identity must be valid"));

    let request = CompilationRequest::new(
        package,
        vec![
            source_input(
                include_str!("../../../../../../../standard-library/std/src/std.bray"),
                0,
            ),
            source_input(
                include_str!("../../../../../../../standard-library/std/src/memory.bray"),
                1,
            ),
            source_input(
                include_str!("../../../../../../../standard-library/std/src/ffi/callback.bray"),
                2,
            ),
            source_input(source, 3),
        ],
    )
    .with_standard_library_source_authority();

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard callback compilation must load: {error:?}"))
}
