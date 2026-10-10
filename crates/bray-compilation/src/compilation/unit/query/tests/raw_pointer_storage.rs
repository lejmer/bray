use crate::test_support::{source_function_body_key, source_input};
use crate::{Compilation, CompilationRequest};
use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;

#[test]
fn in_place_write_returns_initialized_borrowed_storage() {
    let compilation = standard_memory_compilation(
        r#"trusted module std.test;
        func main()
        {
            let mut storage: Uninit<u32> = std.memory.uninit<u32>();
            {
                let mut placement = std.memory.in_place<u32>(&mut storage);
                let placed: &mut u32 = placement.write(42);

                assert(placed == 42);
            };
        }
        "#,
    );
    let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("in-place initialization must lower");

    assert!(lowered.diagnostics().is_empty(), "{:?}", lowered.diagnostics());
    assert!(lowered.value().is_some());
}

#[test]
fn raw_pointer_use_does_not_extend_the_source_borrow() {
    assert_standard_memory_body_has_no_conflicting_borrow(concat!(
        "module std.test;\n",
        "func main()\n",
        "{\n",
        "    let mut value: i32 = 1;\n",
        "    let pointer: RawPointer<i32> = std.memory.address_of<i32>(&value);\n",
        "    std.memory.is_null<i32>(pointer);\n",
        "    value = 2;\n",
        "}\n",
    ));
}

#[test]
fn raw_pointer_assignment_preserves_addressed_storage_initialization() {
    assert_standard_memory_body_has_no_diagnostic(
        concat!(
            "trusted module std.test;\n",
            "trusted func main(pos value: &usize) -> usize uses(raw_memory)\n",
            "{\n",
            "    let pointer: RawPointer<usize> = std.memory.address_of<usize>(value);\n",
            "\n",
            "    return trusted std.memory.read<usize>(pointer);\n",
            "}\n",
        ),
        DiagnosticKind::CheckingUninitializedRawStorage,
    );
}

#[test]
fn borrow_of_recovered_storage_preserves_the_original_diagnostic() {
    assert_standard_memory_body_has_no_conflicting_borrow(concat!(
        "trusted module std.test;\n",
        "trusted func main() -> RawPointer<u8> uses(layout_reinterpret) {\n",
        "    let mut report: Missing = {};\n",
        "    return trusted std.memory.reinterpret<u8, Missing>(std.memory.address_of_mut(&mut report));\n",
        "}\n",
    ));
}

#[test]
fn mutable_owned_aggregate_can_be_addressed_inside_pointer_reinterpretation() {
    assert_standard_memory_body_has_no_conflicting_borrow(concat!(
        "trusted module std.test;\n",
        "struct Report { mut cause: u32; }\n",
        "trusted func main() -> RawPointer<u8> uses(layout_reinterpret) {\n",
        "    let mut report: Report = empty_report();\n",
        "    if report.cause == 0 { report.cause = 1; }\n",
        "    let result: RawPointer<u8> = trusted std.memory.reinterpret<u8, Report>(std.memory.address_of_mut(&mut report));\n",
        "    return result;\n",
        "}\n",
        "func empty_report() -> Report { return { cause = 0 }; }\n",
    ));
}

#[test]
fn mutable_slice_element_can_be_reborrowed_for_a_raw_pointer() {
    assert_standard_memory_body_has_no_conflicting_borrow(concat!(
        "module std.test;\n",
        "func main(pos bytes: & mut [u8]) -> RawPointer<u8>\n",
        "{\n",
        "    if bytes.is_empty()\n",
        "    {\n",
        "        return std.memory.null<u8>();\n",
        "    }\n",
        "\n",
        "    return std.memory.address_of_mut<u8>(& mut bytes[0]);\n",
        "}\n",
    ));
}

fn assert_standard_memory_body_has_no_conflicting_borrow(source: &str) {
    assert_standard_memory_body_has_no_diagnostic(
        source,
        DiagnosticKind::CheckingConflictingBorrow,
    );
}

fn assert_standard_memory_body_has_no_diagnostic(source: &str, diagnostic: DiagnosticKind) {
    let compilation = standard_memory_compilation(source);

    let key = source_function_body_key(&compilation, "main");

    let analysis = compilation
        .storage_flow(key)
        .unwrap_or_else(|error| panic!("storage-flow checking must publish: {error:?}"));

    assert!(
        analysis.diagnostics().by_kind(diagnostic).next().is_none(),
        "{analysis:#?}"
    );
}

fn standard_memory_compilation(source: &str) -> Compilation {
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
            source_input(source, 2),
        ],
    )
    .with_standard_library_source_authority();

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard library compilation must load: {error:?}"))

}

#[test]
fn standard_spin_lock_guard_has_verified_non_reporting_cleanup() {
    let compilation = standard_memory_compilation(include_str!(
        "../../../../../../../standard-library/std/src/sync/spin_lock.bray"
    ));

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}
