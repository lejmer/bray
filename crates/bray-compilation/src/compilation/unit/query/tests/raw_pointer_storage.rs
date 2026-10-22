use crate::test_support::source_function_body_key;
use crate::{Compilation, CompilationRequest};
use bray_diagnostics::DiagnosticKind;
use bray_symbols::PackageIdentity;

#[test]
fn in_place_write_returns_initialized_borrowed_storage() {
    let compilation = standard_memory_compilation(
        r#"trusted module std.test;

        using std.memory;

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

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("in-place initialization must lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );

    assert!(lowered.value().is_some());
}

#[test]
fn raw_pointer_use_does_not_extend_the_source_borrow() {
    assert_standard_memory_body_has_no_conflicting_borrow(
        r#"module std.test;

using std.memory;
func main()
{
    let mut value: i32 = 1;
    let pointer: DataPointer<i32> = std.memory.address_of<i32>(&value);
    std.memory.is_null<i32>(pointer);
    value = 2;
}
"#,
    );
}

#[test]
fn raw_pointer_assignment_preserves_addressed_storage_initialization() {
    assert_standard_memory_body_has_no_diagnostic(
        r#"trusted module std.test;

using std.memory;
trusted func main(pos value: &usize) -> usize uses(raw_memory)
{
    let pointer: DataPointer<usize> = std.memory.address_of<usize>(value);

    return trusted std.memory.read<usize>(pointer);
}
"#,
        DiagnosticKind::CheckingUninitializedRawStorage,
    );
}

#[test]
fn borrow_of_recovered_storage_preserves_the_original_diagnostic() {
    assert_standard_memory_body_has_no_conflicting_borrow(
        r#"trusted module std.test;

using std.memory;
trusted func main() -> DataPointer<u8> uses(layout_reinterpret) {
    let mut report: Missing = {};
    return trusted std.memory.reinterpret<u8, Missing>(std.memory.address_of_mut(&mut report));
}
"#,
    );
}

#[test]
fn mutable_owned_aggregate_can_be_addressed_inside_pointer_reinterpretation() {
    assert_standard_memory_body_has_no_conflicting_borrow(
        r#"trusted module std.test;

using std.memory;
struct Report { mut cause: u32; }
trusted func main() -> DataPointer<u8> uses(layout_reinterpret) {
    let mut report: Report = empty_report();
    if report.cause == 0 { report.cause = 1; }
    let result: DataPointer<u8> = trusted std.memory.reinterpret<u8, Report>(std.memory.address_of_mut(&mut report));
    return result;
}
func empty_report() -> Report { return { cause = 0 }; }
"#,
    );
}

#[test]
fn mutable_slice_element_can_be_reborrowed_for_a_raw_pointer() {
    assert_standard_memory_body_has_no_conflicting_borrow(
        r#"module std.test;

using std.memory;
func main(pos bytes: & mut [u8]) -> DataPointer<u8>
{
    if bytes.is_empty()
    {
        return std.memory.null<u8>();
    }

    return std.memory.address_of_mut<u8>(& mut bytes[0]);
}
"#,
    );
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

    let request = CompilationRequest::with_options(
        package,
        crate::test_support::source_inputs(&[
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            source,
        ]),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("native").expect("fixture link name"),
            bray_symbols::NativeLinkKind::Dynamic,
        )]),
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

#[test]
fn terminal_destructor_rejects_protected_payload_initialization() {
    let compilation = standard_memory_compilation(
        r#"
        trusted module std.test;

        using std.memory;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            mut slot: Uninit<Child>;

            trusted destruct()
            {
                let _: &mut Child = std.memory.uninit_write<Child>(
                    &mut self.slot,
                    Child
                    {
                    }
                );

                let _: Child = trusted std.memory.move_initialized<Child>(&mut self.slot);
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &compilation.check_diagnostics(),
        DiagnosticKind::CheckingDestructorOwnershipIntroduction,
    );
}

#[test]
fn terminal_destructor_rejects_unknown_foreign_writes_through_receiver() {
    let compilation = standard_memory_compilation(
        r#"
        trusted module std.test;

        using std.memory;

        @abi(c)
        @symbol(name = "change")
        @link(name = "native")
        extern trusted func change(pos target: DataPointer<i32>)
            uses(foreign_call);

        struct Owner
        {
            mut value: i32;

            finalize() {}

            trusted destruct()
            {
                trusted change(std.memory.address_of_mut<i32>(&mut self.value));
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &compilation.check_diagnostics(),
        DiagnosticKind::CheckingUnprovedDestructorEffects,
    );
}

#[test]
fn terminal_destructor_distinguishes_local_address_formation_from_escape() {
    for (operation, accepted) in [
        ("trusted opaque();", true),
        ("increment(&mut self.value); trusted opaque();", true),
        ("trusted publish(pointer); trusted opaque();", false),
    ] {
        let compilation = standard_memory_compilation(&format!(
            r#"
            trusted module std.test;

            using std.memory;

            @abi(c)
            @symbol(name = "opaque")
            @link(name = "native")
            extern trusted func opaque() uses(foreign_call);

            @abi(c)
            @symbol(name = "publish")
            @link(name = "native")
            extern trusted func publish(pos pointer: DataPointer<i32>) uses(foreign_call);

            func increment(pos value: &mut i32)
            {{
                value += 1;
            }}

            struct Owner
            {{
                mut value: i32;

                finalize() {{}}

                trusted destruct()
                    uses(foreign_call)
                {{
                    let pointer = std.memory.address_of_mut<i32>(&mut self.value);
                    {operation}
                }}
            }}
            "#
        ));

        let diagnostics = compilation.check_diagnostics();

        if accepted {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                &diagnostics,
                DiagnosticKind::CheckingUnprovedDestructorEffects,
            );
        }
    }
}

#[test]
fn terminal_destructor_checks_unnamed_retained_dependency() {
    let compilation = standard_memory_compilation(
        r#"
        trusted module std.test;

        @abi(c)
        @symbol(name = "opaque")
        @link(name = "native")
        extern trusted func opaque()
            uses(foreign_call);

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            child: &mut Child;

            finalize() {}

            trusted destruct()
            {
                trusted opaque();
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        &compilation.check_diagnostics(),
        DiagnosticKind::CheckingUnprovedDestructorEffects,
    );
}
