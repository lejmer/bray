use bray_base::NonEmptySharedStr;
use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductKind};

#[test]
fn raw_move_from_distinct_input_preserves_nullable_heap_refinement() {
    let source = r#"trusted module std.test;
        using std.memory;
        using internal std.memory;
        callable Wake = @abi(c) trusted func(pos context: usize, pos notify: u32) -> u32 executes(total);
        struct Callback { context: usize; call: Wake; }
        struct Entry { deadline: u64; callback: Callback; }
        struct State { mut entries: std.memory.RawBuffer<Entry>?; }
        trusted func append(pos state: &mut State, pos callback: &mut Callback) -> usize
            uses(raw_memory, unchecked_init)
        {
            if let ?entries = state.entries
            {
                let pointer = std.memory.address_of_mut<Callback>(callback);
                let moved = trusted std.memory.read<Callback>(pointer);
                trusted std.memory.write<Callback>(pointer, { context = moved.context, call = moved.call });
                trusted internal std.memory.initialize_spare<Entry>(
                    &mut entries,
                    { deadline = 1, callback = moved }
                );
                return entries.initialized;
            }
            return 0;
        }
        "#;

    let sources = [
        include_str!("../../../standard-library/std/src/std.bray"),
        include_str!("../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../standard-library/std/src/memory/function.bray"),
        source,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, text)| {
        let identity = u32::try_from(index).expect("fixture source index fits its identity");

        SourceInput::virtual_text(
            SourceIdentity::new(identity),
            format!("memory-{index}"),
            SourceVersion::new(0),
            text,
        )
    })
    .collect();

    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("standard library package identity is valid"),
        sources,
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        )
        .with_native_link_inputs([NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native").expect("fixture native link name is valid"),
            NativeLinkKind::Dynamic,
        )]),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).expect("standard memory source loads");
    let diagnostics = compilation.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
