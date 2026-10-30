use super::fixtures::{
    export, implementation_dependency, standard_library_compilation_with_options,
};
use crate::test_support::{assert_boolean_constants, package_identity, source_input};
use crate::{Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};

const PROVIDER: &str = r#"
module std.layout_fixture;
using std.memory;

@layout(c)
struct Record
{
    byte: u8;
    word: u64;
}

@layout(c)
struct Trailing
{
    byte: u8;
    words: [u64; ..];
}

const SIZE: usize = std.memory.size_of<Record>();
const LAYOUT: Result<std.memory.MemoryLayout, std.memory.MemoryLayoutError> =
    std.memory.layout_of<Record>(count = 3);

const func size<T>() -> usize
{
    return std.memory.size_of<T>();
}

const func trailing(pos count: usize) -> bool
{
    match std.memory.trailing_layout_of<Trailing>(count = count)
    {
        case Ok(layout) { return layout.bytes == 8 + count * 8 && layout.align == 8; }
        case Error(_) { return false; }
    }
}
"#;

const CONSUMER: &str = r#"
module app;
using std.layout_fixture;
using std.memory;
const IMPORTED_SIZE: bool = std.layout_fixture.SIZE == 16;
const GENERIC: bool = std.layout_fixture.size<[u16; 3]>() == 6;
const TRAILING: bool = std.layout_fixture.trailing(3);

const func correct_layout() -> bool
{
    match std.layout_fixture.LAYOUT
    {
        case Ok(layout) { return layout.bytes == 48 && layout.align == 8; }
        case Error(_) { return false; }
    }
}

const IMPORTED_LAYOUT: bool = correct_layout();
"#;

#[test]
#[ignore = "tier:exhaustive"]
fn constant_layout_queries_and_values_preserve_compiled_library_identity() {
    for target in bray_target::NativeTarget::ALL {
        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            SelectedTarget::for_native(target),
        );

        let provider = standard_library_compilation_with_options(
            [
                include_str!("../../../../../../../standard-library/std/src/std.bray"),
                include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
                include_str!(
                    "../../../../../../../standard-library/std/src/memory/allocation.bray"
                ),
                PROVIDER,
            ],
            options.clone(),
        );

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{target:?}: {:?}",
            provider.check_diagnostics()
        );

        let consumer = Compilation::load(
            CompilationRequest::with_options(
                package_identity(),
                vec![source_input(CONSUMER, 0)],
                options,
            )
            .with_dependency_interfaces([implementation_dependency(export(&provider))]),
        )
        .expect("compiled layout consumer loads");

        assert_boolean_constants(
            &consumer,
            &[
                ("IMPORTED_SIZE", true),
                ("GENERIC", true),
                ("TRAILING", true),
                ("IMPORTED_LAYOUT", true),
            ],
        );
    }
}
