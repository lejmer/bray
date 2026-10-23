use std::collections::BTreeSet;
use std::sync::Arc;

use bray_compiler_known::RecognizedStandardLibraryDeclarationKey;
use bray_ir::{MirOperationKind, MirProjectionKind};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageImplementationArtifact, ValidatedPackageInterface,
    build_package_interface_surface, encode_package_interface,
};
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    InherentImplementationSymbolId, MemberLookupResult, ModulePathKey, PackageIdentity,
    ProductKind, TypeCallableMemberSymbolId,
};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};
use bray_testing::test_source_inputs;

use crate::test_support::{
    source_function_body_key, source_named_trait_callable_fulfillment_body_key,
};
use crate::{
    Compilation, CompilationOptions, CompilationRequest, DependencyInterfaceInput, SelectedTarget,
    WorkerBudget,
};

use super::fixtures::{
    compilation_from_sources_for_product_with_platform_services, export,
    standard_library_compilation, standard_library_compilation_with_options,
};

#[test]
fn platform_service_implementations_publish_their_role_with_the_root_template() {
    let Some(binding) =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
    else {
        panic!("test platform binding must be valid");
    };

    let compilation = compilation_from_sources_for_product_with_platform_services(
        [r#"
            trusted module app;

            @layout(c)
            internal struct PlatformStatus
            {
                category: u32;
                reserved: u32;
                native_code: i64;
            }

            @abi(c)
            trusted internal func flush() -> PlatformStatus
            {
                return
                {
                    category = 0,
                    reserved = 0,
                    native_code = 0
                };
            }
        "#],
        ProductKind::Library,
        [binding],
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let platform_templates = bundle
        .executable_templates()
        .iter()
        .filter(|template| template.platform_service().is_some())
        .collect::<Vec<_>>();

    assert_eq!(platform_templates.len(), 1);

    assert_eq!(
        platform_templates[0].identity(),
        bray_ir::MirExecutableTemplateId::ROOT
    );

    assert_eq!(
        platform_templates[0].platform_service(),
        Some(PlatformServiceRole::StandardOutputFlush)
    );
}

#[test]
fn copied_raw_buffer_fields_do_not_establish_initialized_ownership() {
    for (initialized, access) in [
        (
            1,
            "let _ = trusted std.memory.initialized_slice<u8>(&buffer);",
        ),
        (
            0,
            "let pointer = trusted std.memory.spare_pointer<u8>(&mut buffer); trusted std.memory.write<u8>(pointer, 1);",
        ),
    ] {
        let source = format!(
            r#"
        trusted module app;

        using core.memory;
        using std.memory;

        func forged_buffer() {{
            let mut buffer: std.memory.RawBuffer<u8> = {{
                pointer = core.memory.null<u8>(), capacity = 1, initialized = {initialized},
            }};
            {access}
        }}
    "#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn raw_addresses_do_not_extend_borrows_or_retain_local_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            module app;

            using std.memory;

            func shared_address() -> DataPointer<i32> {
                let local: i32 = 1;
                return std.memory.address_of(&local);
            }

            func mutable_address() -> DataPointer<i32> {
                let mut local: i32 = 1;
                let pointer = std.memory.address_of_mut(&mut local);
                local = 2;
                return pointer;
            }

            func protected_address() -> DataPointer<i32> {
                let local: Uninit<i32> = std.memory.uninit<i32>();
                return std.memory.uninit_pointer(&local);
            }

            func protected_mutable_address() -> DataPointer<i32> {
                let mut local: Uninit<i32> = std.memory.uninit<i32>();
                return std.memory.uninit_pointer_mut(&mut local);
            }
        "#,
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn raw_buffer_nullable_views_retain_the_borrowed_owner() {
    for borrow in ["", "mut "] {
        let helper = if borrow.is_empty() {
            "initialized_slice"
        } else {
            "initialized_slice_mut"
        };

        let source = format!(
            r#"
            trusted module app;

            using std.memory;

            struct Holder<T> {{ mut storage: std.memory.RawBuffer<(T?)>; }}

            trusted func element<T>(pos holder: &{borrow}Holder<T>) -> &{borrow}T
                uses(raw_memory)
            {{
                let slots: &{borrow}[(T?)] = trusted std.memory.{helper}<(T?)>(&{borrow}holder.storage);

                return match slots[0]
                {{
                    case ?value {{ yield &{borrow}value; }}
                    case none {{ panic("empty slot"); }}
                }};
            }}

            trusted func caller<T>(pos holder: &{borrow}Holder<T>) -> &{borrow}T
            {{
                return trusted element<T>(holder);
            }}
            "#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}\n{:#?}",
            compilation.check_diagnostics(),
        );
    }
}

#[test]
fn checked_slice_prefix_addresses_establish_the_exact_extent() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func consume_pair(pos pointer: DataPointer<i32>)
                requires(
                    trusted core.memory.valid_write<i32>(pointer = pointer, count = 2),
                    trusted core.memory.aligned_for<i32>(pointer = pointer),
                ) {}

            func from_array() {
                let mut pair: [i32; 2] = [-1, -1];
                let values: &mut [i32] = &mut pair[0..2];
                if !(2 <= values.length()) { panic("storage too small"); }
                let pointer = internal std.memory.slice_pointer_mut<i32>(values, count = 2);
                let copied = pointer;

                consume_pair(copied);
                pair[0] = 1;
            }

            func from_slice(pos values: &mut [i32]) {
                if !(2 <= values.length()) { panic("storage too small"); }
                consume_pair(internal std.memory.slice_pointer_mut<i32>(values, count = 2));
            }

            func empty_address() -> DataPointer<i32> {
                let values: [i32; 1] = [0];

                return internal std.memory.slice_pointer<i32>(&values[0..0], count = 0);
            }
        "#,
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn checked_slice_prefix_authority_expires_with_its_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func observe(pos pointer: DataPointer<i32>)
                requires(trusted core.memory.valid_read<i32>(pointer = pointer, count = 2)) {}

            func expired() {
                let pointer: DataPointer<i32> = {
                    let values: [i32; 2] = [1, 2];

                    yield internal std.memory.slice_pointer<i32>(&values[0..2], count = 2);
                };

                observe(pointer);
            }
        "#,
    ]);

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn raw_buffer_subslices_preserve_disjoint_field_authority() {
    let source = r#"
trusted module std.slice_field_probe;

using std.memory;

trait Writer { mut func write(pos values: &[u8]); }
struct Buffer
{
    mut storage: std.memory.RawBuffer<u8>;

    func as_slice() -> &[u8]
    {
        return trusted std.memory.initialized_slice<u8>(&self.storage);
    }
}

struct Pair<S>
{
    mut storage: Buffer;
    mut sink: S;
    start: usize;
}

func run<S>(pos pair: &mut Pair<S>)
    with(S: Writer)
{
    let values: &[u8] = pair.storage.as_slice();
    let pending: &[u8] = &values[pair.start..values.length()];

    pair.sink.write(pending);
}

func conflict<S>(pos pair: &mut Pair<S>)
    with(S: Writer)
{
    let values: &[u8] = pair.storage.as_slice();
    let pending: &[u8] = &values[pair.start..values.length()];
    let replacement: &mut [u8] = trusted std.memory.initialized_slice_mut<u8>(&mut pair.storage.storage);

    pair.sink.write(pending);

    let _: &mut [u8] = replacement;
}
    "#;

    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        source,
    ]);

    let key = source_function_body_key(&compilation, "run");
    let flow = compilation.storage_flow(key).expect("slice storage flow");

    assert!(flow.diagnostics().is_empty(), "{flow:#?}");

    let key = source_function_body_key(&compilation, "conflict");

    let flow = compilation
        .storage_flow(key)
        .expect("overlapping slice storage flow");

    bray_testing::assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
fn lock_guard_borrows_end_before_unlock_and_keep_the_lock_alive() {
    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/tickets.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/lock.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/guard.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/state.bray"),
    ];

    let compilation = standard_library_compilation(sources.into_iter().chain([r#"
        module app;

        using core.target;

        using std.sync;

        func increment(pos lock: &std.sync.SpinLock<i32>)
            requires(trusted std.sync.spin_lock.valid(marker = &lock.instance)) {
            let mut guard: std.sync.SpinLockGuard<i32> = lock.lock();
            let value: &mut i32 = guard.get();

            value += 1;

            guard.unlock();
        }
    "#]));

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}. Source: {:?}",
        compilation.check_diagnostics(),
        crate::test_support::diagnostic_source_fragments(
            &compilation,
            compilation.check_diagnostics()
        ),
    );

    for (source, kind) in [
        (
            r#"
            module app;

            using core.target;

            using std.sync;

            func escape() -> &mut i32 {
                let lock: std.sync.SpinLock<i32> = std.sync.SpinLock<i32>(7);
                let mut guard: std.sync.SpinLockGuard<i32> = lock.lock();

                return guard.get();
            }
        "#,
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        ),
        (
            r#"
            module app;

            using core.target;

            using std.sync;

            func use_after_unlock(pos lock: &std.sync.SpinLock<i32>)
                requires(trusted std.sync.spin_lock.valid(marker = &lock.instance)) {
                let mut guard: std.sync.SpinLockGuard<i32> = lock.lock();
                let value: &mut i32 = guard.get();

                guard.unlock();

                value += 1;
            }
        "#,
            bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
        ),
    ] {
        let compilation = standard_library_compilation(sources.into_iter().chain([source]));

        bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(), kind);
    }
}

#[test]
fn sparse_hash_table_access_retains_its_owner_and_rejects_removal_while_borrowed() {
    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/collection/hash_table.bray"),
    ];

    let compilation = standard_library_compilation(sources.into_iter().chain([r#"
        module std.collection;

        internal func read(pos table: &HashTable<i32>, index: usize) -> &i32 {
            return trusted hash_table_item(table, index = index);
        }

        internal func update(pos table: &mut HashTable<i32>, index: usize) {
            let value: &mut i32 = trusted hash_table_item_mut(table, index = index);
            value += 1;
        }

        internal func remove(pos table: &mut HashTable<i32>, index: usize) -> i32 {
            return trusted hash_table_take(table, index = index);
        }
        "#]));

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );

    for (source, kind) in [
        (
            r#"
            module std.collection;

            using std.memory;

            internal func escape() -> Result< &i32, std.memory.MemoryLayoutError> {
                let table: HashTable<i32> = try trusted HashTable<i32>(capacity = 4);
                return Ok(trusted hash_table_item(&table, index = 0));
            }
            "#,
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        ),
        (
            r#"
            module std.collection;

            internal func use_after_remove(pos table: &mut HashTable<i32>, index: usize) {
                let value: &mut i32 = trusted hash_table_item_mut(table, index = index);
                let removed: i32 = trusted hash_table_take(table, index = index);
                value += removed;
            }
            "#,
            bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
        ),
    ] {
        let compilation = standard_library_compilation(sources.into_iter().chain([source]));

        bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(), kind);
    }
}

#[test]
fn byte_buffer_bulk_writes_publish_the_initialized_prefix() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/bytes/buffer.bray"),
        r#"
            trusted module app;

            using std.bytes;
            using std.memory;

            func exercise() -> Result<usize, std.memory.MemoryLayoutError> {
                let bytes: [u8; 2] = [1, 2];
                let mut buffer: std.bytes.Buffer = try std.bytes.Buffer.from_slice(&bytes[..]);

                try trusted std.bytes.append(&mut buffer, bytes = &bytes[..]);
                try trusted std.bytes.append(&mut buffer, value = 7, count = 2);
                try trusted std.bytes.resize(&mut buffer, new_length = 8, fill = 9);
                trusted std.bytes.truncate(&mut buffer, new_length = 3);

                return Ok(buffer.as_slice().length());
            }
        "#,
    ]);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );
}

#[test]
fn integer_truncation_certifies_execution_without_certifying_other_calls_or_numeric_domains() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        r#"
        module app;

        using std.numeric;

        func narrow(pos value: u128) -> u8 executes(pure, total) {
            return std.numeric.truncate_to<u8>(value);
        }

        func wire_count(pos value: u64) -> usize executes(pure, total) {
            return std.numeric.truncate_to<usize>(value);
        }

        func flags(pos value: u32) -> u32 executes(pure, total) {
            return ((value | 1) & ~2) ^ 3;
        }
        "#,
    ]);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for source in [
        r#"
        module app;

        func truncate_to(pos value: u64) -> usize { loop {} }

        func ordinary_call(pos value: u64) -> usize executes(pure, total) {
            return truncate_to(value);
        }
        "#,
        r#"
        module app;

        using std.numeric;

        func floating_truncation(pos value: r64) -> i32 executes(pure, total) {
            return std.numeric.truncate_to<i32>(value);
        }
        "#,
    ] {
        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            source,
        ]);

        assert!(
            compilation.check_diagnostics().iter().any(|diagnostic| {
                diagnostic.kind()
                    == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven
            }),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn nul_terminated_storage_conditions_do_not_turn_addresses_into_owned_witnesses() {
    let declarations = r#"
        trusted module app;

        using core.memory;

        trusted func preserve_text(pos pointer: DataPointer<u8>) -> DataPointer<u8>
            requires(trusted core.memory.nul_terminated_read(pointer = pointer))
            ensures(trusted core.memory.nul_terminated_read(pointer = result))
        {
            return pointer;
        }
    "#;

    for (element, predicate) in [
        ("u8", "nul_terminated_read"),
        ("u16", "wide_nul_terminated_read"),
    ] {
        let declarations = declarations
            .replace("u8", element)
            .replace("nul_terminated_read", predicate);

        let copying = r#"
        trusted module app;

        using core.memory;

        trusted func copy_address(pos pointer: DataPointer<u8>) -> DataPointer<u8>
            requires(trusted core.memory.nul_terminated_read(pointer = pointer))
            ensures(trusted core.memory.nul_terminated_read(pointer = result))
        {
            let address: DataPointer<u8> = trusted preserve_text(pointer);
            let copied: DataPointer<u8> = address;

            return copied;
        }
        "#
        .replace("u8", element)
        .replace("nul_terminated_read", predicate);

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            &declarations,
            &copying,
        ]);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        for source in [
            r#"
        module app;

        func missing_termination(pos pointer: DataPointer<u8>) -> DataPointer<u8>
        {
            return trusted preserve_text(pointer);
        }
        "#,
            r#"
        trusted module app;

        using core.memory;
        using std.memory;

        trusted func changed_termination(pos pointer: DataPointer<u8>) -> DataPointer<u8>
            requires(
                trusted core.memory.nul_terminated_read(pointer = pointer),
                trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
                trusted core.memory.aligned_for<u8>(pointer = pointer),
            )
        {
            trusted std.memory.write<u8>(pointer, value = 1);

            return trusted preserve_text(pointer);
        }
        "#,
        ] {
            let source = source
                .replace("u8", element)
                .replace("nul_terminated_read", predicate);

            let compilation = standard_library_compilation([
                include_str!("../../../../../../../standard-library/std/src/std.bray"),
                include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
                include_str!(
                    "../../../../../../../standard-library/std/src/memory/allocation.bray"
                ),
                include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
                &declarations,
                &source,
            ]);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn native_nul_guarantees_follow_the_selected_pointer_null_guard() {
    for (guard, valid) in [
        ("if core.memory.is_null<u8>(pointer) { return; }", true),
        ("", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;

            @link(name = "c", kind = system)
            @symbol(name = "getcwd")
            @abi(c)
            extern trusted func native_directory(pos destination: DataPointer<u8>, pos capacity: usize) -> DataPointer<u8>
                requires(capacity != 0, trusted core.memory.valid_write<u8>(pointer = destination, count = capacity))
                ensures(core.memory.is_null<u8>(result) || trusted core.memory.nul_terminated_read(pointer = result))
                uses(foreign_call);

            trusted func preserve_text(pos pointer: DataPointer<u8>) -> DataPointer<u8>
                requires(trusted core.memory.nul_terminated_read(pointer = pointer))
            {{
                return pointer;
            }}

            trusted func consume_directory(pos destination: DataPointer<u8>, pos capacity: usize)
                requires(capacity != 0, trusted core.memory.valid_write<u8>(pointer = destination, count = capacity))
                uses(foreign_call)
            {{
                let pointer: DataPointer<u8> = trusted native_directory(destination, capacity);
                {guard}

                let _: DataPointer<u8> = trusted preserve_text(pointer);
            }}
        "#
        );

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::default(),
        )
        .with_native_link_inputs([bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c").expect("native library name is valid"),
            bray_symbols::NativeLinkKind::System,
        )]);

        let compilation = Compilation::load(
            CompilationRequest::with_options(
                PackageIdentity::try_new("std").expect("standard-library identity is valid"),
                test_source_inputs(
                    "native-directory",
                    [
                        include_str!("../../../../../../../standard-library/std/src/std.bray"),
                        include_str!(
                            "../../../../../../../standard-library/std/src/numeric/policy.bray"
                        ),
                        &source,
                    ],
                ),
                options,
            )
            .with_standard_library_source_authority(),
        )
        .expect("native directory contract must load");

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:#?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn native_environment_cursor_requires_live_provider_storage() {
    let section = standard_library_source_section;

    let unix = include_str!("../../../../../../../standard-library/std/src/platform/unix.bray");
    let linux = include_str!("../../../../../../../standard-library/std/src/platform/linux.bray");

    let representation =
        include_str!("../../../../../../../standard-library/std/src/platform/representation.bray");

    let sdk = include_str!(
        "../../../../../../../standard-library/std/src/os/generated/x86_64_unknown_linux_gnu.bray"
    );

    let os = include_str!("../../../../../../../standard-library/std/src/os/unix/linux.bray");

    let native = format!(
        r#"module std.os {{}} trusted module std.os.linux {{
using core.memory;
@link(name = "c", kind = system)
{}
@link(name = "c", kind = system)
{}
}}
trusted module std.os.unix {{
using core.memory;
using std.os.linux;
{}
}}"#,
        section(
            sdk,
            "@symbol(name = \"strnlen\")",
            "// Error state: returns minus one"
        ),
        section(sdk, "@symbol(name = \"environ\")", "// Dynamic code symbol"),
        section(
            os,
            "trusted func narrow_string_length",
            "trusted func open_file"
        ),
    );

    let linux_platform = format!(
        r#"trusted internal module std.platform;
using core.memory;
using std.memory;
using std.os.linux;
using std.os.unix;
using std.numeric;
{}
{}
{}
{}
trusted internal func observe_cursor() {{ BODY let _ = trusted internal native_text_cursor_entry(&cursor); }}"#,
        section(
            unix,
            "internal struct NativeTextCursor",
            "trusted internal func prepare_native_text_copy"
        ),
        section(
            unix,
            "trusted internal func terminated_byte_length",
            "trusted internal func descriptor_read"
        ),
        section(
            representation,
            "trusted internal func read_native_pointer",
            "trusted internal func pointer_handle"
        ),
        section(
            linux,
            "trusted internal func environment_storage_address",
            "trusted internal func argument_storage"
        ),
    );

    let windows =
        include_str!("../../../../../../../standard-library/std/src/platform/windows.bray");

    let windows_sdk = include_str!(
        "../../../../../../../standard-library/std/src/os/generated/x86_64_pc_windows_msvc.bray"
    );

    let mut windows_native =
        "module std.os {} trusted module std.os.windows { using core.memory; ".to_owned();

    windows_native.push_str(section(
        windows_sdk,
        "const BCRYPT_USE_SYSTEM_PREFERRED_RNG",
        "callable BeginThreadRoutine",
    ));

    for (library, symbol) in [
        ("Kernel32", "GetEnvironmentStringsW"),
        ("Kernel32", "FreeEnvironmentStringsW"),
        ("Kernel32", "lstrlenW"),
        ("Kernel32", "GetCommandLineW"),
        ("Kernel32", "LocalFree"),
        ("Shell32", "CommandLineToArgvW"),
        ("Kernel32", "GetLastError"),
    ] {
        windows_native.push_str(section(
            windows_sdk,
            &format!("@link(name = \"{library}\", kind = system)\n@symbol(name = \"{symbol}\")"),
            "\n// ",
        ));
    }

    windows_native.push('}');

    let mut windows_platform = format!(
        "trusted internal module std.platform; using core.memory; using std.memory; using std.numeric; using std.os.windows; {} {} {} {} {} {} {} {} trusted internal func observe_cursor() {{ BODY let _ = trusted internal native_environment_entry(&owner); }}",
        section(
            representation,
            "@copy\n@layout(c)\ninternal struct PlatformStatus",
            "@copy\n@layout(c)\ninternal struct NativeTextPair"
        ),
        section(
            windows,
            "internal struct LocalAllocation",
            "trusted internal func process_identity"
        ),
        section(
            windows,
            "trusted internal func terminated_u16_length",
            "trusted internal predicate native_arguments_live"
        ),
        section(
            windows,
            "trusted internal predicate native_arguments_live",
            "trusted internal func native_arguments"
        ),
        section(
            windows,
            "trusted internal func native_arguments()",
            "trusted internal func native_environment()"
        ),
        section(
            windows,
            "trusted internal func copy_native_argument",
            "trusted internal func environment_keys_equal"
        ),
        section(
            representation,
            "trusted internal func read_native_pointer",
            "trusted internal func pointer_handle"
        ),
        section(
            windows,
            "trusted internal func native_environment()",
            "trusted internal func windows_error_status"
        ),
    );

    windows_platform.push_str(section(
        windows,
        "trusted internal func argument_storage()",
        "trusted internal func copy_native_argument",
    ));

    windows_platform.push_str(section(
        windows,
        "trusted internal func windows_error_status()",
        "trusted internal func prepare_clock_monotonic_now",
    ));

    let status = include_str!("../../../../../../../standard-library/std/src/platform/status.bray");

    windows_platform.push_str(
        &status[status
            .find("internal func native_status")
            .expect("native status constructor exists")..],
    );

    for (target, library, native, platform, bodies) in [
        (
            SelectedTarget::default(),
            &["c"][..],
            &native,
            &linux_platform,
            [
                "let cursor: NativeTextCursor = trusted internal environment_cursor();",
                "let mut cursor: NativeTextCursor = trusted internal environment_cursor(); cursor.current = std.memory.null<DataPointer<u8>>();",
                "let cursor: NativeTextCursor = { first = std.memory.null<DataPointer<u8>>(), limit = 1, current = std.memory.null<DataPointer<u8>>(), remaining = 1 };",
            ],
        ),
        (
            SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
            &["Kernel32", "Shell32"][..],
            &windows_native,
            &windows_platform,
            [
                "{ let arguments: LocalAllocation = trusted internal native_arguments(); if arguments.count > 0 { let _ = trusted internal native_argument(&arguments, index = 0); } }; let owner: EnvironmentBlock = trusted internal native_environment();",
                "let original = trusted internal native_environment(); let owner: EnvironmentBlock = { identity = original.identity, pointer = original.pointer, current = std.memory.uninit_from<DataPointer<u16>>(std.memory.null<u16>()) };",
                "let owner: EnvironmentBlock = { identity = NativeTextOwner {}, pointer = std.memory.null<u16>(), current = std.memory.uninit_from<DataPointer<u16>>(std.memory.null<u16>()) };",
            ],
        ),
    ] {
        for (index, body) in bodies.into_iter().enumerate() {
            let platform = platform.replace("BODY", body);

            let options = CompilationOptions::new(
                WorkerBudget::default(),
                ProductKind::Library,
                target.clone(),
            )
            .with_native_link_inputs(library.iter().map(|library| {
                bray_symbols::NativeLinkRequirement::new(
                    bray_base::NonEmptySharedStr::try_new(*library)
                        .expect("native library name is valid"),
                    bray_symbols::NativeLinkKind::System,
                )
            }));

            let compilation = standard_library_compilation_with_options(
                [
                    include_str!("../../../../../../../standard-library/std/src/std.bray"),
                    include_str!(
                        "../../../../../../../standard-library/std/src/numeric/policy.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/memory/pointer.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/memory/storage.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/memory/allocation.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/numeric/limits.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/memory/buffer.bray"
                    ),
                    include_str!(
                        "../../../../../../../standard-library/std/src/memory/function.bray"
                    ),
                    native,
                    &platform,
                ],
                options,
            );

            if index == 0 {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{:#?}",
                    compilation.check_diagnostics()
                );

                export(&compilation);
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    if target
                        == SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc)
                        && index == 1
                    {
                        bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
                    } else {
                        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                    },
                );
            }
        }
    }
}
#[test]
fn native_thread_payloads_keep_their_allocation_owner() {
    let types = include_str!("../../../../../../../standard-library/std/src/thread/types.bray")
        .replace("\r\n", "\n");

    let boundary =
        include_str!("../../../../../../../standard-library/std/src/thread/boundary.bray");

    let owner = types
        .split_once("@layout(stable)\ninternal struct TransferredThreadValue")
        .expect("native thread allocation owner must exist")
        .1
        .split_once("internal struct ThreadStart")
        .expect("native thread owner must terminate")
        .0;

    let operations = boundary
        .split_once("trusted internal func allocate_thread_value")
        .expect("native thread allocation producer must exist")
        .1
        .split_once("@abi(c)")
        .expect("native thread allocation operations must terminate")
        .0;

    let thread = format!(
        r#"trusted internal module std.thread;
using core.memory;
using std.memory;
@layout(stable)
internal struct TransferredThreadValue{owner}
trusted internal func allocate_thread_value{operations}"#
    );

    for (ty, value, capabilities) in [
        ("u32", "42", ""),
        ("unit", "unit", ""),
        (
            "std.memory.RawAllocation",
            "trusted std.memory.allocate({ bytes = 1, align = 1 })",
            "uses(manual_alloc)",
        ),
    ] {
        let source = format!(
            r#"trusted module app;

using core.memory;
using std.memory;

using internal std.thread;

 trusted func round_trip() -> {ty} {capabilities} {{ return trusted internal std.thread.take_transferred_value<{ty}>(trusted internal std.thread.allocate_thread_value<{ty}>({value})); }}"#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &thread,
            &source,
        ]);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn returned_tuple_and_array_components_retain_initialized_storage_contracts() {
    for (result_type, result_value, projection) in [
        ("(usize, bool)", "(initialized, true)", ".0"),
        ("[usize; 2]", "[initialized, 0]", "[0]"),
    ] {
        let source = format!(
            r#"
            trusted module std.platform;

            using core.memory;
            using std.memory;

            trusted func fill_prefix(
                pos buffer: &mut std.memory.RawBuffer<u8>, count: usize,
            ) -> {result_type}
                ensures(trusted core.memory.initialized_range_as<u8>(pointer = buffer.pointer, count = result{projection}))
            {{
                let initialized: usize = trusted internal std.memory.fill_spare_bytes(buffer, value = 11, count = count);

                return {result_value};
            }}

            trusted internal func consume_prefix(pos buffer: &mut std.memory.RawBuffer<u8>, count: usize) uses(unchecked_init) {{
                let prefix: {result_type} = trusted internal fill_prefix(buffer, count = count);

                trusted std.memory.set_initialized_count<u8>(buffer, count = prefix{projection});
            }}
        "#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{result_type}: {:#?}",
            compilation.check_diagnostics()
        );

        let artifact = encode_package_interface(export(&compilation))
            .expect("initialized-prefix interface must encode");

        let dependency = DependencyInterfaceInput::new(
            PackageIdentity::try_new("std").expect("standard-library identity is valid"),
            InterfaceProductIdentity::try_new("library").expect("library identity is valid"),
            "std.brayi",
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let consumer_source = format!(
            r#"
            module app;

            using std.memory;
            using internal std.platform;

            trusted func consume_prefix(pos buffer: &mut std.memory.RawBuffer<u8>, count: usize)
                uses(unchecked_init)
            {{
                let prefix: {result_type} = trusted std.platform.fill_prefix(buffer, count = count);

                trusted std.memory.set_initialized_count<u8>(buffer, count = prefix{projection});
            }}
        "#
        );

        let consumer = Compilation::load(
            CompilationRequest::new(
                crate::test_support::package_identity(),
                test_source_inputs("consumer", [&consumer_source]),
            )
            .with_dependency_interfaces([dependency]),
        )
        .expect("consumer of the initialized-prefix contract must load");

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{result_type}: {:#?}",
            consumer.check_diagnostics()
        );

        if projection == "[0]" {
            let source = source.replace("count = prefix[0]", "count = prefix[1]");

            let compilation = standard_library_compilation([
                include_str!("../../../../../../../standard-library/std/src/std.bray"),
                include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
                include_str!(
                    "../../../../../../../standard-library/std/src/memory/allocation.bray"
                ),
                include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
                include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
                &source,
            ]);

            assert!(
                compilation.check_diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind()
                        == bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                }),
                "{:#?}",
                compilation.check_diagnostics()
            );
        }
    }
}

#[test]
fn standard_runtime_text_storage_has_complete_contracts() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/runtime/text.bray"),
    ]);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );
}

#[test]
fn raw_byte_access_requires_initialized_extents_and_rejects_index_overflow() {
    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
    ];

    for (guard, valid) in [
        ("4 <= count", true),
        ("count >= 4", true),
        ("3 < count", true),
        ("count > 3", true),
        ("3 <= count", false),
        ("count >= 3", false),
        ("2 < count", false),
        ("count > 2", false),
    ] {
        let source = format!(
            r#"
            module std.memory;

            using core.memory;
            trusted internal func read_prefix(pos pointer: DataPointer<u8>, count: usize) -> u8
                requires(
                    trusted core.memory.valid_read<u8>(pointer = pointer, count = count),
                    trusted core.memory.initialized_range_as<u8>(pointer = pointer, count = count),
                )
                uses(raw_memory)
            {{
                if !({guard}) {{ return 0; }}
                return trusted internal byte_buffer_read(pointer, index = 3);
            }}
        "#
        );

        let compilation =
            standard_library_compilation(sources.into_iter().chain([source.as_str()]));

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{guard}: {:#?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    let compilation = standard_library_compilation(sources.into_iter().chain([r#"
        module std.memory;

        using core.memory;
        using std.numeric;

        trusted internal func fill_widened_count(pos destination: DataPointer<u8>, length: u64)
            requires(trusted core.memory.valid_write<u8>(pointer = destination, count = length as usize))
            uses(raw_memory, unchecked_init) {
            trusted internal byte_buffer_fill(destination, value = 7, count = length as usize);
        }

        internal func fill_with_widened_count(pos bytes: &mut [u8]) {
            let count: usize = bytes.length();
            let byte_pointer: DataPointer<u8> = internal slice_pointer_mut(bytes, count = count);
            trusted internal fill_widened_count(byte_pointer, length = count as u64);
        }

        internal struct ByteSpan
        {
            address: DataPointer<u8>;
            count: usize;
        }

        internal func returned_bytes(pos source: &[u8]) -> &[u8] executes(pure, total) {
            return source;
        }

        internal func checked_span(pos source: &[u8]) -> ByteSpan
            ensures(
                trusted core.memory.valid_read<u8>(pointer = result.address, count = result.count),
                trusted core.memory.initialized_range_as<u8>(pointer = result.address, count = result.count),
            ) executes(pure) {
            let bytes: &[u8] = returned_bytes(source);

            return {
                address = internal slice_pointer(bytes, count = bytes.length()),
                count = bytes.length(),
            };
        }

        trusted internal func read_two_checked_spans(pos left: &[u8], pos right: &[u8]) -> u8 uses(raw_memory) {
            let left_span: ByteSpan = checked_span(left);
            let right_span: ByteSpan = checked_span(right);

            if !(1 <= left_span.count) || !(1 <= right_span.count) { panic("source empty"); }

            let left_byte: u8 = trusted internal byte_buffer_read(left_span.address, index = 0);
            let right_byte: u8 = trusted internal byte_buffer_read(right_span.address, index = 0);
            return left_byte ^ right_byte;
        }

        trusted internal func read_wire_span(pos source: &[u8]) -> u8 uses(raw_memory) {
            let span: ByteSpan = checked_span(source);
            let length: u64 = span.count as u64;
            let count: usize = length as usize;

            if !(1 <= count) { return 0; }

            return trusted internal byte_buffer_read(span.address, index = 0);
        }

        trusted internal func read_after_pointer_and_scalar_calculations(pos bytes: &[u8]) -> u8 uses(raw_memory) {
            if !(1 <= bytes.length()) { panic("source empty"); }
            let byte_pointer: DataPointer<u8> = internal slice_pointer(bytes, count = 1);
            let _: DataPointer<u8> = offset(byte_pointer, elements = 1);
            let _: DataPointer<u8> = byte_offset(byte_pointer, bytes = 1);
            let _: bool = is_null(null<u8>());
            let _: usize = size_of<u32>();
            let _: usize = align_of<u32>();
            let _: u32 = std.numeric.truncate_to<u32>(bytes.length());
            return trusted internal byte_buffer_read(byte_pointer, index = 0);
        }

        trusted internal func read_loop(pos bytes: &[u8]) uses(raw_memory) {
            let count: usize = bytes.length();
            let byte_pointer: DataPointer<u8> = internal slice_pointer(bytes, count = count);
            let mut index: usize = 0;

            while index < count && index != std.numeric.USIZE_MAX {
                let _: u8 = trusted internal byte_buffer_read(byte_pointer, index = index);
                index += 1;
            }
        }

        trusted internal func read_twice(pos bytes: &[u8]) -> u8 requires(2 <= bytes.length()) executes(pure) uses(raw_memory) {
            if !(2 <= bytes.length()) { panic("source too small"); }
            let byte_pointer: DataPointer<u8> = internal slice_pointer(bytes, count = 2);
            let first: u8 = trusted internal byte_buffer_read(byte_pointer, index = 0);
            let second: u8 = trusted internal byte_buffer_read(byte_pointer, index = 1);
            return first ^ second;
        }

        internal func checked_slice_operations(pos source: &[u8], pos destination: &mut [u8]) {
            trusted internal copy_bytes(&mut destination[..], source);
            trusted internal fill_bytes(&mut destination[..], value = 7);
        }

        trusted internal func copy_to_fresh_storage(pos source: &[u8]) -> DataPointer<u8> uses(manual_alloc, raw_memory, unchecked_init) {
            let count: usize = source.length();
            let source_pointer: DataPointer<u8> = internal slice_pointer(source, count = count);
            let destination: DataPointer<u8> = trusted core.memory.allocate(bytes = count, align = 1);
            trusted std.memory.copy_overlapping<u8>(source_pointer, destination, count = count);
            return destination;
        }

        trusted internal func copy_slice_to_fresh_buffer(
            pos source: &[u8],
        ) -> Result<RawBuffer<u8>, MemoryLayoutError>
            uses(unchecked_init)
        {
            let count: usize = source.length();
            let created: Result<RawBuffer<u8>, MemoryLayoutError> = trusted RawBuffer<u8>(capacity = count);

            match consume created {
                case Ok(buffer) {
                    let mut buffer: RawBuffer<u8> = buffer;
                    let source_pointer: DataPointer<u8> = internal slice_pointer(source, count = count);
                    let initialized: usize = trusted internal copy_spare_raw_bytes(&mut buffer,
                        source = source_pointer, count = count);
                    trusted set_initialized_count(&mut buffer, count = initialized);
                    return Ok(buffer);
                }
                case Error(error) { return Error(error); }
            }
        }

        trusted internal func copy_raw_to_fresh_buffer(pos source: DataPointer<u8>, count: usize) -> RawBuffer<u8>
            requires(
                trusted core.memory.valid_read<u8>(pointer = source, count = count),
                trusted core.memory.initialized_range_as<u8>(pointer = source, count = count),
            ) uses(manual_alloc, unchecked_init) {
            let mut buffer: RawBuffer<u8> = trusted internal allocate_buffer<u8>(capacity = count, bytes = count, align = 1);
            let initialized: usize = trusted internal copy_spare_raw_bytes(&mut buffer, source = source, count = count);
            trusted set_initialized_count(&mut buffer, count = initialized);
            return buffer;
        }

        trusted internal func copy_slice_bytes(pos source: &[u8], pos destination: &mut [u8]) uses(raw_memory, unchecked_init) {
            if !(2 <= source.length()) || !(2 <= destination.length()) { panic("storage too small"); }
            let source_pointer: DataPointer<u8> = internal slice_pointer(source, count = 2);
            let destination_pointer: DataPointer<u8> = internal slice_pointer_mut(destination, count = 2);
            trusted std.memory.copy_overlapping<u8>(source_pointer, destination_pointer, count = 2);
        }
        "#]));

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );

    for source in [
        r#"
        module std.memory;

        using std.numeric;
        internal func read_after_changing_guarded_index(pos bytes: &[u8]) {
            let count: usize = bytes.length();
            let byte_pointer: DataPointer<u8> = internal slice_pointer(bytes, count = count);
            let mut index: usize = 0;

            while index < count && index != std.numeric.USIZE_MAX {
                index = count;
                let _: u8 = trusted internal byte_buffer_read(byte_pointer, index = index);
            }
        }
        "#,
        r#"
        module std.memory;

        using core.memory;
        internal func read_without_initialization(pos pointer: DataPointer<u8>) -> u8
            requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)) {
            return trusted internal byte_buffer_read(pointer, index = 0);
        }
        "#,
        r#"
        module std.memory;

        using core.memory;
        internal func copy_without_initialization(pos source: DataPointer<u8>, pos destination: DataPointer<u8>)
            requires(
                trusted core.memory.valid_read<u8>(pointer = source, count = 1),
                trusted core.memory.valid_write<u8>(pointer = destination, count = 1),
            ) {
            trusted std.memory.copy_overlapping<u8>(source, destination, count = 1);
        }
        "#,
        r#"
        module std.memory;

        using std.numeric;

        using core.memory;
        internal func overflowing_index(pos pointer: DataPointer<u8>) -> u8
            requires(
                trusted core.memory.valid_read<u8>(pointer = pointer, count = 0),
                trusted core.memory.initialized_range_as<u8>(pointer = pointer, count = 0),
            ) {
            return trusted internal byte_buffer_read(pointer, index = std.numeric.USIZE_MAX);
        }
        "#,
    ] {
        let compilation = standard_library_compilation(sources.into_iter().chain([source]));

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn standard_unstable_sort_specializes_for_fixed_arrays() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/order.bray"),
        r#"
        module std.order;

        internal struct SortArray { mut values: [u32; 6]; }

        impl SortArraySequence = SortArray(OrderedSequence) {
            type Element = u32;
            func order_length() -> usize { return 6; }
            func compare_indices(pos left: usize, pos right: usize) -> Ordering {
                return self.compare_value(left, &self.values[right]);
            }
            func compare_value(pos index: usize, pos value: &u32) -> Ordering {
                if self.values[index] < value { return Ordering.Less; }
                if self.values[index] > value { return Ordering.Greater; }
                return Ordering.Equal;
            }
            mut func swap_indices(pos left: usize, pos right: usize) {
                let displaced: u32 = self.values[left];
                self.values[left] = self.values[right];
                self.values[right] = displaced;
            }
        }

        func sort_fixture() -> [u32; 6] {
            let input: [u32; 6] = [4, 1, 4, 0, 9, 2];
            let mut values: SortArray = { values = input };
            values.sort_unstable();
            return values.values;
        }
        "#,
    ]);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );

    let key = source_function_body_key(&compilation, "sort_fixture");

    let lowered = compilation
        .lowered_unit(key)
        .expect("sort fixture must lower");

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());
}

#[test]
fn copying_raw_reads_preserve_initialization() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.atomic;
            using std.memory;

            trusted func read_twice() -> i32 uses(raw_memory) {
                let state: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);
                let value: i32 = 7;
                let pointer: DataPointer<i32> = std.memory.address_of<i32>(&value);
                let first: i32 = trusted std.memory.read<i32>(pointer);
                let _: u32 = core.atomic.load<u32, 0>(&state);
                let second: i32 = trusted std.memory.read<i32>(pointer);

                return first + second;
            }
        "#,
    ]);

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics(),
    );
}

#[test]
fn raw_reads_retire_initialization_after_moves_and_opaque_calls() {
    for (value_type, interruption) in [
        (
            "Token",
            "let first: Token = trusted std.memory.read<Token>(pointer);",
        ),
        ("i32", "interfere(pointer);"),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            using std.memory;

            struct Token {{ value: i32; }}

            extern func interfere(pos pointer: DataPointer<i32>);

            trusted func invalid(pos pointer: DataPointer<{value_type}>)
                requires(
                    trusted core.memory.valid_read<{value_type}>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<{value_type}>(pointer = pointer),
                    trusted core.memory.initialized_as<{value_type}>(pointer = pointer),
                )
                uses(raw_memory)
            {{
                {interruption}
                let second: {value_type} = trusted std.memory.read<{value_type}>(pointer);
            }}
        "#,
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn raw_allocation_copied_fields_do_not_transfer_the_live_owner() {
    for operation in [
        "let second: std.memory.RawAllocation = { pointer = first.pointer, bytes = first.bytes, align = first.align };",
        "let second = pack(pointer = first.pointer, bytes = first.bytes, align = first.align);",
        "trusted core.memory.deallocate(first.pointer, bytes = first.bytes, align = first.align);",
        "let pointer = first.pointer; let bytes = first.bytes; let align = first.align; let second = pack(pointer = pointer, bytes = bytes, align = align);",
        "let pointer = first.pointer; let bytes = first.bytes; let align = first.align; let second = pack_with_owner(owner = first, pointer = pointer, bytes = bytes, align = align);",
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func pack(pointer: DataPointer<u8>, bytes: usize, align: usize) -> std.memory.RawAllocation
                requires(trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align))
            {{
                return {{ pointer = pointer, bytes = bytes, align = align }};
            }}

            func duplicate() {{
                let first = trusted std.memory.allocate(std.memory.MemoryLayout {{ bytes = 8, align = 8 }});
                {operation}
            }}

            func pack_with_owner(owner: std.memory.RawAllocation, pointer: DataPointer<u8>, bytes: usize, align: usize) -> std.memory.RawAllocation
                requires(trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align))
            {{
                return {{ pointer = pointer, bytes = bytes, align = align }};
            }}
        "#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn scoped_raw_allocation_requirements_reject_copied_owner_fields() {
    for mode in ["", "consume "] {
        let source = format!(
            r#"trusted module app;

using core.memory;
using std.memory;

struct Resource
{{
    pointer: DataPointer<u8>;
    bytes: usize;
    align: usize;
}}

impl Resource
{{
    {mode}enter() -> unit
        requires(trusted core.memory.owned_allocation(pointer = self.pointer, bytes = self.bytes, align = self.align))
    {{ return unit; }}

    exit(pos lease: unit) {{}}
}}

func duplicate()
{{
    let first = trusted std.memory.allocate(std.memory.MemoryLayout {{ bytes = 8, align = 8 }});
    let resource: Resource = {{ pointer = first.pointer, bytes = first.bytes, align = first.align }};

    with lease = resource {{ let _ = lease; }};
}}
"#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn raw_allocation_address_copies_preserve_whole_owner_transfer() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            trusted func dispose(pos owner: std.memory.RawAllocation)
                requires(trusted core.memory.owned_allocation(pointer = owner.pointer, bytes = owner.bytes, align = owner.align))
                uses(manual_alloc)
            {
                trusted std.memory.deallocate(owner);
            }

            trusted func transfer() uses(manual_alloc) {
                let first = trusted std.memory.allocate(std.memory.MemoryLayout { bytes = 8, align = 8 });
                let pointer = first.pointer;
                let copied = pointer;
                let bytes = first.bytes;
                let align = first.align;
                let second = first;
                dispose(second);
            }
        "#,
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn raw_allocation_entry_field_proofs_remain_attached_to_the_owner() {
    for ownership in ["", "&"] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func duplicate(pos first: {ownership}std.memory.RawAllocation) -> std.memory.RawAllocation
                requires(trusted core.memory.owned_allocation(pointer = first.pointer, bytes = first.bytes, align = first.align))
            {{
                return {{ pointer = first.pointer, bytes = first.bytes, align = first.align }};
            }}
        "#
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn raw_allocation_field_construction_requires_ownership() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func forge() -> std.memory.RawAllocation {
                return { pointer = core.memory.null<u8>(), bytes = 8, align = 8 };
            }
        "#,
    ]);

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn raw_allocation_field_construction_accepts_actual_ownership() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func pack(pointer: DataPointer<u8>, bytes: usize, align: usize) -> std.memory.RawAllocation
                requires(trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align))
            {
                return { pointer = pointer, bytes = bytes, align = align };
            }
        "#,
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn raw_buffer_origins_require_recognition_and_survive_prefix_writes() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            module app;

            using core.memory;
            using internal std.memory;

            struct RawBuffer
            {
                pointer: DataPointer<u8>;
                capacity: usize;
                initialized: usize;
            }

            func ordinary() -> RawBuffer {
                return { pointer = core.memory.null<u8>(), capacity = 1, initialized = 1 };
            }

            trusted func filled_buffer(pos value: u8, pos unused: &u8) -> std.memory.RawBuffer<u8>
            {
                let mut storage: std.memory.RawBuffer<u8> = match trusted std.memory.RawBuffer<u8>(capacity = 1)
                {
                    case Ok(buffer) { yield buffer; }
                    case Error(_) { panic("buffer layout failed"); }
                };
                trusted internal std.memory.initialize_spare<u8>(&mut storage, value);
                return storage;
            }

            trusted func drop_filled(pos unused: &u8)
            {
                let storage = filled_buffer(1, unused);
            }

            trusted func choose_buffer(pos input: std.memory.RawBuffer<u8>, pos reuse: bool) -> std.memory.RawBuffer<u8>
            {
                if reuse { return input; }
                let value: u8 = 1;
                return filled_buffer(value, &value);
            }

            trusted func drop_selected(pos input: std.memory.RawBuffer<u8>, pos reuse: bool)
            {
                let storage = choose_buffer(input, reuse);
            }

        "#,
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let filled = crate::compilation::result_dependencies::source_function_dependencies(
        &compilation,
        "filled_buffer",
    );

    assert!(
        filled
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_none()
                && relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result),
        "the real allocated pointer retains its independent origin after filling: {filled:#?}"
    );

    let append = crate::compilation::result_dependencies::source_function_dependencies(
        &compilation,
        "initialize_spare",
    );

    assert!(
        !append
            .effects()
            .contains(&bray_symbols::DependencyStorageEffect::Unknown),
        "the checked intrinsic retains its finite header and backing transitions: {append:#?}"
    );

    for name in ["drop_filled", "drop_selected"] {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &compilation,
            name,
        );

        assert!(
            !summary
                .effects()
                .contains(&bray_symbols::DependencyStorageEffect::Unknown),
            "{name}: {summary:#?}"
        );

        assert!(
            summary
                .effects()
                .contains(&bray_symbols::DependencyStorageEffect::Ambient),
            "{name}: {summary:#?}"
        );

        if name == "drop_selected" {
            assert!(summary.effects().iter().any(|effect| matches!(effect,
                bray_symbols::DependencyStorageEffect::Indirect { reference, kind: bray_symbols::DependencyMutationKind::Retire, .. }
                    if reference.subject_root() == bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0)))),
                "the input alternative must retain its retirement effect: {summary:#?}");
        }
    }
}

#[test]
fn raw_allocation_field_construction_transfers_ownership_once() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        r#"
            trusted module app;

            using core.memory;
            using std.memory;

            func duplicate(pointer: DataPointer<u8>, bytes: usize, align: usize)
                requires(trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align))
            {
                let first: std.memory.RawAllocation = { pointer = pointer, bytes = bytes, align = align };
                let second: std.memory.RawAllocation = { pointer = pointer, bytes = bytes, align = align };
            }
        "#,
    ]);

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn standard_memory_surface_exports_uninitialized_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
    ]);

    let source_graph = compilation
        .product_source_graph()
        .unwrap_or_else(|error| panic!("standard memory source graph must build: {error:?}"));

    assert!(
        source_graph.diagnostics().is_empty(),
        "standard memory source graph diagnostics: {:?}",
        source_graph.diagnostics()
    );

    let product = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("standard memory product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "standard memory product diagnostics: {:?}",
        product.diagnostics()
    );

    assert!(
        !product.value().is_recovered(),
        "standard memory product semantics must not recover"
    );

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("standard memory symbol graph must build: {error:?}"));

    let identity = super::super::construction::build_identity_surface(
        &compilation,
        symbols,
        product.value().public_symbols(),
        &[],
    )
    .unwrap_or_else(|error| panic!("standard memory identity surface must build: {error:?}"));

    let request = compilation
        .package_interface_export_request()
        .unwrap_or_else(|| panic!("standard memory export request must exist"));

    let surface = build_package_interface_surface(
        request.identity().clone(),
        [],
        identity.symbols,
        identity.relationships,
        identity.exports,
    )
    .unwrap_or_else(|error| panic!("standard memory interface surface must build: {error:?}"));

    let (semantics, _, _, _) = super::super::super::semantic::build_semantics(
        &compilation,
        symbols,
        &surface,
        &identity.selected,
        &identity.keys,
    )
    .unwrap_or_else(|error| panic!("standard memory semantic export must build: {error:?}"));

    assert_strictly_canonical("constraints", semantics.constraints());
    assert_strictly_canonical("callable contracts", semantics.callable_contracts());
    assert_strictly_canonical("callable signatures", semantics.callable_signatures());
    assert_strictly_canonical("generic declarations", semantics.generic_declarations());

    assert_strictly_canonical(
        "callable parameter defaults",
        semantics.callable_parameter_defaults(),
    );

    assert_strictly_canonical("predicate definitions", semantics.predicate_definitions());
    assert_strictly_canonical("declared types", semantics.declared_types());
    assert_strictly_canonical("type representations", semantics.type_representations());
    assert_strictly_canonical("implementations", semantics.implementations());
    assert_strictly_canonical("coherence", semantics.coherence());
    assert_strictly_canonical("target dependencies", semantics.target_dependencies());
    assert_strictly_canonical("ABI dependencies", semantics.abi_dependencies());
    assert_strictly_canonical("runtime requirements", semantics.runtime_requirements());
    assert_strictly_canonical("provenance", semantics.provenance());

    compilation
        .package_implementation_configuration(None)
        .unwrap_or_else(|error| {
            panic!("standard memory implementation configuration must build: {error:?}")
        });

    let _ = export(&compilation);
}

#[test]
fn standard_memory_api_fixture_checks() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/tests/api/memory.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "standard memory API diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let mut recovered = Vec::new();

    walk_syntax_tree(compilation.syntax_tree(), |event| {
        if let SyntaxWalkEvent::EnterNode(node) = event
            && node.is_recovered()
        {
            recovered.push((node.kind(), node.full_range()));
        }

        SyntaxWalkControl::Continue
    });

    assert!(
        recovered.is_empty(),
        "standard memory API syntax must not recover: {recovered:?}"
    );
}

#[test]
fn standard_string_equality_satisfies_source_and_imported_generic_constraints() {
    let provider = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/string.bray"),
        r#"
            module bray.standard_library_tests.string_operations;

            using std.string.StringEquatable;

            func generic_equal<T>(pos left: T, pos right: T) -> bool
                with(T: Equatable<T>)
            {
                return left == right;
            }

            func source_string_equality()
            {
                assert(generic_equal<string>("same", "same"));
            }
        "#,
    ]);

    assert!(
        provider.check_diagnostics().is_empty(),
        "source standard-library equality diagnostics: {:#?}",
        provider.check_diagnostics()
    );

    let artifact = encode_package_interface(export(&provider))
        .unwrap_or_else(|error| panic!("string interface must encode: {error:?}"));

    let standard_library = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        standard_library,
        product,
        "std.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer_package = PackageIdentity::try_new("std.tests.api")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        r#"
            module bray.standard_library_tests.string_operations;

            using std.string.StringEquatable;

            func generic_equal<T>(pos left: T, pos right: T) -> bool
                with(T: Equatable<T>)
            {
                return left == right;
            }

            func imported_string_equality()
            {
                assert(generic_equal<string>("same", "same"));
            }
        "#,
    );

    let consumer = Compilation::load(
        CompilationRequest::new(consumer_package, vec![source])
            .with_dependency_interfaces([dependency])
            .with_standard_library_source_authority(),
    )
    .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.check_diagnostics().is_empty(),
        "imported standard-library equality diagnostics: {:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn standard_formatting_surface_round_trips_and_specializes_without_provider_source() {
    // This interface fixture never executes its buffer adapters. Give them explicit diverging
    // bodies so imported reachability cannot silently treat Bray declarations as foreign imports.
    let provider = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        r#"
            module std.memory;

            union MemoryLayoutError
            {
                SizeOverflow;
                UnsupportedAlignment;
            }

            trusted func copy_bytes(pos destination: &mut [u8], pos source: &[u8])
            {
                loop
                {
                }
            }

            trusted func fill_bytes(pos destination: &mut [u8], value: u8)
            {
                loop
                {
                }
            }
        "#,
        r#"
            module std.bytes;

            using std.memory;

            struct Buffer
            {
                internal value: bool;

                internal construct(capacity: usize = 0) -> Result<Self, std.memory.MemoryLayoutError>
                {
                    let buffer: Buffer =
                    {
                        value = false,
                    };

                    return Ok(buffer);
                }

                func as_slice() -> &[u8]
                {
                    return as_slice(&self);
                }
            }

            func as_slice(pos buffer: &Buffer) -> &[u8]
            {
                loop
                {
                }
            }

            func length(pos buffer: &Buffer) -> usize
            {
                loop
                {
                }
            }

            func push(pos buffer: &mut Buffer, value: u8) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            internal func append_slice(pos buffer: &mut Buffer, pos bytes: &[u8]) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            internal func append_repeated(
                pos buffer: &mut Buffer,
                pos value: u8,
                pos count: usize
            ) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            overload append =
            {
                append_slice,
                append_repeated,
            }

            func reserve(pos buffer: &mut Buffer, additional: usize) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }

            func resize(pos buffer: &mut Buffer, new_length: usize, fill: u8 = 0) -> Result<unit, std.memory.MemoryLayoutError>
            {
                loop
                {
                }
            }
        "#,
        r#"
            module std.string;

            union Utf8Error
            {
                InvalidEncoding;
            }

            impl string
            {
                func as_bytes() -> &[u8]
                {
                    return internal utf8(&self);
                }

                static func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>
                {
                    return internal decode_utf8(bytes);
                }
            }

            internal func utf8(pos value: &string) -> &[u8]
            {
                loop
                {
                }
            }

            internal func decode_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>
            {
                loop
                {
                }
            }
        "#,
        include_str!("../../../../../../../standard-library/std/src/character.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/checked.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/integer.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/options.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/argument.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/sink.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/integer_width.bray"),
        include_str!("../../../../../../../standard-library/std/src/format/rendering.bray"),
        r#"
            trusted module std.io;

            union IoErrorKind
            {
                BrokenStream;
            }

            struct IoError
            {
                kind: IoErrorKind;
                transferred: usize;
            }

            trait Writer
            {
                mut func write(pos source: &[u8]) -> Result<usize, IoError>
                    requires(blocking_execution());

                mut func flush() -> Result<unit, IoError>
                    requires(blocking_execution());

                mut func write_all(pos source: &[u8]) -> Result<unit, IoError>
                    requires(blocking_execution())
                {
                    let length: usize = source.length();
                    let mut written: usize = 0;

                    while written < length
                    {
                        let result: Result<usize, IoError> = self.write(&source[written..length]);

                        match consume result
                        {
                            case Ok(count)
                            {
                                if count == 0 || count > length - written
                                {
                                    return Error(
                                        {
                                            kind = IoErrorKind.BrokenStream,
                                            transferred = written
                                        }
                                    );
                                }

                                written += count;
                            }
                            case Error(error)
                            {
                                return Error(prefixed_error(error, prefix = written));
                            }
                        }
                    }

                    return Ok(unit);
                }
            }

            internal func smaller(pos left: usize, pos right: usize) -> usize
            {
                if left < right
                {
                    return left;
                }

                return right;
            }

            internal func prefixed_error(pos error: IoError, prefix: usize) -> IoError
            {
                return
                {
                    kind = error.kind,
                    transferred = prefix + error.transferred
                };
            }
        "#,
        include_str!("../../../../../../../standard-library/std/src/io/formatting.bray"),
        crate::test_support::RUNTIME_MEMORY_SOURCE,
        crate::test_support::RUNTIME_TEXT_SOURCE,
        crate::test_support::RUNTIME_CHARACTER_SOURCE,
    ]);

    assert!(
        provider.syntax_tree_result().diagnostics().is_empty(),
        "{:?}",
        provider.syntax_tree_result().diagnostics()
    );

    assert!(
        provider.declaration_diagnostics().is_empty(),
        "{:?}",
        provider.declaration_diagnostics()
    );

    let product = provider
        .product_semantics()
        .unwrap_or_else(|error| panic!("formatting product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "{:?}",
        product.diagnostics()
    );

    assert!(!product.value().is_recovered());

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let adapter = provider
        .lowered_unit(source_named_trait_callable_fulfillment_body_key(
            &provider,
            "WriterFormattingSink",
            "write",
        ))
        .unwrap_or_else(|error| panic!("writer formatting adapter must lower: {error:?}"));

    let adapter = adapter
        .value()
        .as_ref()
        .and_then(bray_lowering::LoweredUnit::mir)
        .unwrap_or_else(|| panic!("writer formatting adapter must produce MIR: {adapter:#?}"));

    assert!(
        adapter.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Borrow { place, .. }
                if place
                    .projections()
                    .iter()
                    .any(|projection| matches!(projection.kind(), MirProjectionKind::Field(_)))
                    && matches!(
                        place.projections().last().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
        )),
        "generic writer field borrow must reach the destination value: {adapter:#?}"
    );

    let interface = export(&provider);

    let runtime_capabilities: BTreeSet<_> = interface
        .semantics()
        .runtime_requirements()
        .iter()
        .flat_map(|requirement| requirement.requirements().capabilities())
        .copied()
        .collect();

    assert!(runtime_capabilities.is_empty());

    let artifact = encode_package_interface(interface)
        .unwrap_or_else(|error| panic!("formatting interface must encode: {error:?}"));

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated = ValidatedPackageInterface::try_new(artifact.bytes(), policy)
        .unwrap_or_else(|error| panic!("formatting interface must validate: {error:?}"));

    let implementation = PackageImplementationArtifact::try_new(
        &validated,
        interface.surface(),
        interface.semantics(),
        interface.implementation_configuration().clone(),
        [],
        interface.executable_templates().iter().cloned(),
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("formatting implementation must encode: {error:?}"));

    let provider_package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let provider_product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        provider_package.clone(),
        provider_product,
        "std.brayi",
        artifact.shared_bytes(),
        policy,
    )
    .with_implementation_artifact("std.brayimpl", Arc::new(implementation));

    let consumer_package = PackageIdentity::try_new("example.application")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        r#"
            module app;

            using std.format;
            using std.format.ByteSinkFormatting;
            using std.format.StringFormat;
            using std.format.I32Format;
            using std.format.U32Format;
            using std.bytes;
            using std.io;
            using std.io.WriterFormattingSink;
            using std.memory;

            struct RecordingWriter
            {
                mut written: usize;
            }

            impl RecordingWriterIo = RecordingWriter(std.io.Writer)
            {
                mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
                    requires(blocking_execution())
                {
                    let length: usize = source.length();

                    self.written += length;
                    return Ok(length);
                }

                mut func flush() -> Result<unit, std.io.IoError>
                    requires(blocking_execution())
                {
                    return Ok(unit);
                }
            }

            func render(pos destination: &mut std.format.ByteSink, pos value: string) -> Result<unit, std.memory.MemoryLayoutError>
                requires(blocking_execution())
            {
                return std.format.write(destination, std.format.Argument<string>(&value));
            }

            func render_integer(
                pos destination: &mut std.format.ByteSink,
                pos value: i32
            ) -> Result<unit, std.memory.MemoryLayoutError>
                requires(blocking_execution())
            {
                return std.format.write(destination, std.format.Argument<i32>(&value));
            }

            func resolved_defaults() -> std.format.Options
            {
                return std.format.Options();
            }

            trusted func stream_integer(pos writer: &mut RecordingWriter, pos value: u32) -> Result<unit, std.io.IoError>
                requires(blocking_execution())
            {
                let mut destination: std.io.FormattingSink<RecordingWriter> = std.io.FormattingSink<RecordingWriter>(writer);

                return trusted std.format.write_to<u32, std.io.FormattingSink<RecordingWriter>, std.io.IoError>(
                    &mut destination,
                    std.format.Argument<u32>(&value),
                );
            }
        "#,
    );

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Library,
        SelectedTarget::default(),
    );

    let request = CompilationRequest::with_options(consumer_package, vec![source], options)
        .with_dependency_interfaces([dependency]);

    let consumer = Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.imported_diagnostics().is_empty(),
        "{:?}",
        consumer.imported_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("formatting skeleton must build: {error:?}"));

    let skeleton = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton"));

    let recognized = Arc::clone(
        imported
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton")),
    )
    .recognize_standard_library(&provider_package, |_| true);

    let recognized_key = |value| {
        RecognizedStandardLibraryDeclarationKey::try_new(value)
            .unwrap_or_else(|| panic!("recognized standard-library key must be valid: {value}"))
    };

    assert!(
        recognized
            .declaration_symbol::<InherentImplementationSymbolId>(&recognized_key(
                "StandardStringImplementation",
            ))
            .is_some(),
        "the string implementation must retain its imported identity"
    );

    for key in ["StandardStringAsBytes", "StandardStringFromUtf8"] {
        assert!(
            recognized
                .declaration_symbol::<TypeCallableMemberSymbolId>(&recognized_key(key))
                .is_some(),
            "{key} must retain its nested imported identity"
        );
    }

    let package = skeleton
        .package_by_identity(&provider_package)
        .unwrap_or_else(|| panic!("standard-library package must be imported"));

    assert!(
        consumer
            .symbol_graph()
            .unwrap_or_else(|error| panic!("consumer source symbols must build: {error:?}"))
            .packages()
            .iter()
            .all(|package| package.identity() != &provider_package),
        "provider symbols must come only from the package interface"
    );

    let format_path = ModulePathKey::try_new(["std", "format"])
        .unwrap_or_else(|| panic!("format module path must be valid"));

    let format = skeleton
        .module_by_path(package.id(), &format_path)
        .unwrap_or_else(|| panic!("format module must be imported"));

    for name in [
        "Argument",
        "ByteSink",
        "ByteSinkFormatting",
        "Options",
        "write",
        "write_to",
    ] {
        assert!(
            matches!(
                skeleton.lookup(format.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    let bytes_path = ModulePathKey::try_new(["std", "bytes"])
        .unwrap_or_else(|| panic!("bytes module path must be valid"));

    let bytes = skeleton
        .module_by_path(package.id(), &bytes_path)
        .unwrap_or_else(|| panic!("bytes module must be imported"));

    assert!(matches!(
        skeleton.lookup(bytes.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let memory_path = ModulePathKey::try_new(["std", "memory"])
        .unwrap_or_else(|| panic!("memory module path must be valid"));

    let memory = skeleton
        .module_by_path(package.id(), &memory_path)
        .unwrap_or_else(|| panic!("memory module must be imported"));

    assert!(matches!(
        skeleton.lookup(memory.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let io_path = ModulePathKey::try_new(["std", "io"])
        .unwrap_or_else(|| panic!("io module path must be valid"));

    let io = skeleton
        .module_by_path(package.id(), &io_path)
        .unwrap_or_else(|| panic!("io module must be imported"));

    for name in ["IoError", "Writer", "WriterFormattingSink"] {
        assert!(
            matches!(
                skeleton.lookup(io.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let lowered = consumer
        .lowered_unit(source_function_body_key(&consumer, "resolved_defaults"))
        .unwrap_or_else(|error| panic!("imported named constructor must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(!skeleton.traits().is_empty());
    assert!(!skeleton.structures().is_empty());
    assert!(!skeleton.named_trait_implementations().is_empty());

    let streamed = consumer
        .lowered_unit(source_function_body_key(&consumer, "stream_integer"))
        .unwrap_or_else(|error| panic!("imported formatting adapter must lower: {error:?}"));

    assert!(streamed.value().is_some(), "{:#?}", streamed.diagnostics());

    assert!(
        streamed.diagnostics().is_empty(),
        "{:#?}",
        streamed.diagnostics()
    );

    let imported_instances = consumer
        .codegen_instance_count_for_test(|key| {
            matches!(key, bray_ir::MirUnitKey::ImportedExecutable(_))
        })
        .unwrap_or_else(|error| panic!("imported formatting reachability must close: {error:?}"));

    assert!(imported_instances > 0);
}

fn assert_strictly_canonical<T>(table: &str, values: &[T])
where
    T: std::fmt::Debug + Ord,
{
    if let Some(pair) = values.windows(2).find(|pair| pair[0] >= pair[1]) {
        panic!(
            "standard memory {table} are not canonical: {:?} then {:?}",
            pair[0], pair[1]
        );
    }
}

#[test]
fn empty_raw_copies_preserve_deallocation_authority() {
    for (operation, extent) in [("copy", "0"), ("copy_overlapping", "count")] {
        let source = format!(
            r#"
            trusted module app;

            using std.memory;

            using core.memory;
            trusted func copy_nothing(pos source: DataPointer<u8>, pos destination: DataPointer<u8>, pos count: usize)
                requires(
                    count == 0,
                    trusted core.memory.valid_read<u8>(pointer = source, count = {extent}),
                    trusted core.memory.initialized_range_as<u8>(pointer = source, count = {extent}),
                    trusted core.memory.valid_write<u8>(pointer = destination, count = {extent}),
                    trusted core.memory.owned_allocation(pointer = destination, bytes = 1, align = 1),
                    trusted core.memory.non_overlapping<u8>(left = source, left_count = {extent}, right = destination, right_count = {extent}),
                )
                uses(manual_alloc, raw_memory, unchecked_init) {{
                trusted std.memory.{operation}<u8>(source, destination, count = {extent});
                trusted core.memory.deallocate(destination, bytes = 1, align = 1);
            }}
            "#,
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            &source,
        ]);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{operation}: {:#?}",
            compilation.check_diagnostics(),
        );
    }
}

#[test]
fn thread_storage_load_only_initializes_successful_outputs() {
    let boundary =
        include_str!("../../../../../../../standard-library/std/src/thread/boundary.bray");

    let start = boundary
        .find("trusted internal func platform_thread_storage_load(")
        .expect("thread storage load boundary exists");

    let boundary = format!(
        "trusted internal module std.thread; @abi(c) {}",
        &boundary[start..]
    );

    let platform = r#"
        trusted internal module std.platform;
        @copy @layout(c) internal struct PlatformStatus {
            category: u32; reserved: u32; native_code: i64;
        }
    "#;

    let preparation = r#"
        trusted internal module std.thread;

        using core.memory;
        using internal std.platform;
        trusted internal func need_initialized(pos pointer: DataPointer<DataPointer<u8>>)
            requires(trusted core.memory.initialized_as<DataPointer<u8>>(pointer = pointer)) {}
        trusted internal func prepare_thread_storage_load(
            pos key: u64, pos value: DataPointer<DataPointer<u8>>
        ) -> Result<DataPointer<u8>, std.platform.PlatformStatus>
            executes(total)
            requires(
                trusted core.memory.valid_write<DataPointer<u8>>(pointer = value, count = 1),
                trusted core.memory.aligned_for<DataPointer<u8>>(pointer = value),
            )
            ensures(
                trusted core.memory.valid_write<DataPointer<u8>>(pointer = value, count = 1),
                trusted core.memory.aligned_for<DataPointer<u8>>(pointer = value),
            ) {
            return Error({ category = 5, reserved = 0, native_code = 0 });
        }
    "#;

    for guarded in [true, false] {
        let caller = format!(
            r#"
            trusted internal module std.thread;

            using std.numeric;

            using core.memory;
            using std.memory;
            trusted internal func load_uninitialized_slot(pos pointer: DataPointer<DataPointer<u8>>) -> DataPointer<u8>
                requires(
                    trusted core.memory.valid_read<DataPointer<u8>>(pointer = pointer, count = 1),
                    trusted core.memory.valid_write<DataPointer<u8>>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<DataPointer<u8>>(pointer = pointer),
                ) {{
                let status = trusted platform_thread_storage_load(std.numeric.U64_MAX, pointer);
                {}
                need_initialized(pointer);
                return std.memory.null<u8>();
            }}
            "#,
            if guarded {
                "if status.category != 0 { return std.memory.null<u8>(); }"
            } else {
                ""
            },
        );

        let compilation = standard_library_compilation([
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            include_str!("../../../../../../../standard-library/std/src/platform/status.bray"),
            platform,
            &preparation,
            &boundary,
            &caller,
        ]);

        if guarded {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:#?}",
                compilation.check_diagnostics(),
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

fn standard_library_source_section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let locate = |source: &str, marker: &str| {
        source
            .find(marker)
            .or_else(|| source.find(&marker.replace('\n', "\r\n")))
            .unwrap_or_else(|| panic!("native source marker {marker:?} must exist"))
    };

    let source = &source[locate(source, start)..];

    &source[..locate(source, end)]
}

#[test]
fn unix_native_outputs_remain_in_typed_storage() {
    let function = |source: &str, name: &str| {
        let start = source
            .find(&format!("func {name}("))
            .expect("native function exists");

        let start = source[..start]
            .rfind("@link(")
            .expect("native function retains its link");

        let end = start + source[start..].find(';').expect("native declaration ends") + 1;

        source[start..end].to_owned()
    };

    let section = standard_library_source_section;

    for (target, sdk, wrappers, darwin) in [
        (
            bray_target::NativeTarget::X86_64LinuxGnu,
            include_str!(
                "../../../../../../../standard-library/std/src/os/generated/x86_64_unknown_linux_gnu.bray"
            ),
            include_str!("../../../../../../../standard-library/std/src/os/unix/linux.bray"),
            false,
        ),
        (
            bray_target::NativeTarget::Aarch64LinuxGnu,
            include_str!(
                "../../../../../../../standard-library/std/src/os/generated/aarch64_unknown_linux_gnu.bray"
            ),
            include_str!("../../../../../../../standard-library/std/src/os/unix/linux.bray"),
            false,
        ),
        (
            bray_target::NativeTarget::X86_64MacOs,
            include_str!(
                "../../../../../../../standard-library/std/src/os/generated/x86_64_apple_darwin.bray"
            ),
            include_str!("../../../../../../../standard-library/std/src/os/unix/darwin.bray"),
            true,
        ),
        (
            bray_target::NativeTarget::Aarch64MacOs,
            include_str!(
                "../../../../../../../standard-library/std/src/os/generated/aarch64_apple_darwin.bray"
            ),
            include_str!("../../../../../../../standard-library/std/src/os/unix/darwin.bray"),
            true,
        ),
    ] {
        let system = if darwin { "darwin" } else { "linux" };

        let mut native = format!(
            "trusted module std.os.{system}; using core.memory; struct PThreadAttribute; {}",
            section(
                sdk,
                "callable ThreadStartRoutine =",
                "callable ThreadStorageDestructor ="
            )
        );

        for name in ["socket_pair", "pthread_create"] {
            native.push_str(&function(sdk, name));
        }

        for name in if darwin {
            &["AF_UNIX", "SOCK_STREAM", "F_SETFD", "FD_CLOEXEC"][..]
        } else {
            &["AF_UNIX", "SOCK_STREAM", "SOCK_CLOEXEC"][..]
        } {
            let start = sdk
                .find(&format!("const {name}:"))
                .expect("native constant exists");

            let end = start + sdk[start..].find(';').expect("native constant ends") + 1;

            native.push_str(&sdk[start..end]);
        }

        if darwin {
            for name in ["fcntl", "close", "error_location"] {
                native.push_str(&function(sdk, name));
            }
        }

        let platform_dependencies = if darwin {
            "using core.target; using internal std.platform;"
        } else {
            ""
        };

        let wrapper = format!(
            "trusted internal module std.os.unix; using std.memory; using std.os.{system}; {platform_dependencies} {} {}",
            section(
                wrappers,
                "trusted func socket_pair(",
                "trusted func duplicate_close_on_exec("
            ),
            section(
                wrappers,
                "trusted func pthread_create(",
                "trusted func pthread_detach("
            )
        );

        let helper = format!(
            "trusted internal module std.platform; using core.memory; using std.memory; {}",
            section(
                include_str!(
                    "../../../../../../../standard-library/std/src/platform/representation.bray"
                ),
                "trusted internal func read_native_i32(",
                "trusted internal func read_native_u16("
            )
        );

        let options = CompilationOptions::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::for_native(target),
        )
        .with_native_link_inputs(["c", "pthread"].map(|name| {
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new(name).expect("native library name is valid"),
                bray_symbols::NativeLinkKind::System,
            )
        }));

        let sources = [
            include_str!("../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
            include_str!("../../../../../../../standard-library/std/src/os/unix/thread.bray"),
            "module std.os {}",
            &native,
            &wrapper,
            &helper,
        ];

        let compilation = standard_library_compilation_with_options(sources, options.clone());

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{target:?}: {:#?}",
            compilation.check_diagnostics()
        );

        export(&compilation);

        let forged = format!(
            r#"trusted internal module std.os.unix;

using std.os;

using std.memory;
using std.memory.function;

 trusted func forged_output(pos output: DataPointer<{}>, pos entry: ThreadStartRoutine)
                    uses(foreign_call)
                {{
                    let native_entry = trusted std.memory.callable_from_pointer(
                        trusted std.memory.function.reinterpret<std.os.{system}.ThreadStartRoutine, ThreadStartRoutine>(
                            trusted std.memory.function_address(entry)
                        )
                    );

                    let _ = trusted std.os.{system}.pthread_create(
                        output,
                        attributes = std.memory.null<std.os.{system}.PThreadAttribute>(),
                        entry = native_entry,
                        context = std.memory.null<u8>(),
                    );
                }}
            "#,
            if darwin { "DataPointer<u8>" } else { "u64" }
        );

        let compilation = standard_library_compilation_with_options(
            sources.into_iter().chain([forged.as_str()]),
            options,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn local_callable_execution_proofs_export_their_named_dependencies() {
    let provider = super::fixtures::compilation(
        r#"
        module app;
        func helper() executes(pure, total) {}
        func invoke() executes(pure, total) {
            let action = lambda() executes(pure, total) { helper(); };
            action();
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = super::fixtures::execution_consumer(
        &provider,
        r#"
        module consumer;
        using example.package.app;
        func invoke() executes(pure, total) { example.package.app.invoke(); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );

    for application in [&provider, &consumer] {
        let proof = application
            .execution_properties(source_function_body_key(application, "invoke"))
            .unwrap();

        assert!(
            !proof.diagnostics().has_errors(),
            "{:?}",
            proof.diagnostics()
        );
    }
}

#[test]
fn production_path_view_preserves_its_borrowed_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/bytes.bray"),
        include_str!("../../../../../../../standard-library/std/src/bytes/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/path.bray"),
        include_str!("../../../../../../../standard-library/std/src/fs.bray"),
        include_str!("../../../../../../../standard-library/std/src/fs/platform.bray"),
        include_str!("../../../../../../../standard-library/std/src/io.bray"),
        include_str!("../../../../../../../standard-library/std/src/platform.bray"),
        include_str!("../../../../../../../standard-library/std/src/platform/status.bray"),
        "module std.collection;",
    ]);

    let key = source_function_body_key(&compilation, "platform_path");
    let checked = compilation.lowered_unit(key.clone()).unwrap();

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let proof = compilation.execution_properties(key).unwrap();

    assert!(
        !proof.diagnostics().has_errors(),
        "{:?}",
        proof.diagnostics()
    );
}

#[test]
fn guard_lifetime_anchors_are_not_opaque_call_targets() {
    let declarations = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/tickets.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/lock.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/guard.bray"),
        include_str!("../../../../../../../standard-library/std/src/sync/spin_lock/state.bray"),
    ];

    let caller = r#"
        trusted module app;
        using std.sync;
        using internal std.sync.spin_lock;
        static LOCK: std.sync.SpinLock<u32> =
        {
            let prepared = std.sync.SpinLock<u32>.prepare(0);

            yield { instance = prepared.0, storage = prepared.1 };
        };
        trusted func acquire() -> std.sync.SpinLockGuard<u32>
            ensures(trusted std.sync.spin_lock.acquired(&result.acquisition)) { return LOCK.lock(); }
        func static_caller(pos callback: func() -> unit) {
            let mut guard = acquire();
            change(guard.get(), callback);
        }
        func access(pos guard: &mut std.sync.SpinLockGuard<u32>) -> &mut u32
            requires(trusted std.sync.spin_lock.acquired(&guard.acquisition))
            ensures(trusted std.sync.spin_lock.acquired(&guard.acquisition)) {
            return guard.get();
        }
        func change(pos value: &mut u32, pos callback: func() -> unit) { callback(); value += 1; }
        func caller(pos callback: func() -> unit) {
            let lock = std.sync.SpinLock<u32>(0);
            let mut guard = lock.lock();
            change(access(&mut guard), callback);
        }
    "#;

    let source = standard_library_compilation(declarations.into_iter().chain([caller]));

    for name in ["caller", "static_caller"] {
        let checked = source
            .lowered_unit(source_function_body_key(&source, name))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{name}: {:?}",
            checked.diagnostics()
        );
    }

    let provider = standard_library_compilation(declarations);
    let consumer = super::fixtures::execution_consumer(&provider, caller);

    for name in ["caller", "static_caller"] {
        let checked = consumer
            .lowered_unit(source_function_body_key(&consumer, name))
            .unwrap();

        assert!(
            !checked.diagnostics().has_errors(),
            "{name}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn imported_nested_integer_truncation_retains_compiler_operation_proofs() {
    let provider = standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        r"
        module api;

        using std.numeric;

        func narrow(pos value: u64) -> u32
            executes(pure, total)
        {
            return std.numeric.truncate_to<u32>(value);
        }
        ",
    ]);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let consumer = super::fixtures::execution_consumer(
        &provider,
        r"
        module app;

        using std.api;

        func caller(pos value: u64) -> u32
            executes(pure, total)
        {
            return std.api.narrow(value);
        }
    ",
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn layout_queries_in_runtime_requirements_reach_native_predicate_operations() {
    let declarations = format!(
        r#"
        module std.memory;

        {}
    "#,
        standard_library_source_section(
            include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
            "extern const func size_of<T>()",
            "extern const func layout_of<T>",
        )
    );

    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        declarations.as_str(),
    ];

    let provider = standard_library_compilation(sources);

    for query in ["size_of", "align_of", "stride_of"] {
        let source = format!(
            r#"
            module app;

            using std.memory;

            struct Record
            {{
                prefix: u8;
                payload: u64;
            }}

            func selected(pos value: usize)
                requires(value == std.memory.{query}<Record>())
            {{
            }}

            func caller(pos value: usize)
            {{
                selected(value);
            }}

            func main()
            {{
                caller(16);
            }}
        "#
        );

        let local = standard_library_compilation(sources.into_iter().chain([source.as_str()]));
        let imported = super::fixtures::execution_consumer(&provider, &source);

        for compilation in [&local, &imported] {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{query}: {:#?}",
                compilation.check_diagnostics()
            );

            let helpers = compilation
                .codegen_instance_count_for_test(|key| {
                    matches!(key, bray_ir::MirUnitKey::Predicate(_))
                })
                .expect("checked layout queries lower to native memory operations");

            assert!(helpers > 0, "{query} must retain a runtime requirement");
        }
    }
}

#[test]
fn raw_buffer_reachability_preserves_reference_bearing_payload_dependencies() {
    let sources = [
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
    ];

    for (element, value, valid) in [("u8", "1", true), ("(&mut Owner)", "anchor", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using internal std.memory;

            struct Owner {{ mut value: u8; }}
            trusted predicate live(value: &Owner);
            trusted func make() -> Owner ensures(trusted live(&result))
            {{
                return {{ value = 0 }};
            }}
            extern trusted func accept(pos buffer: std.memory.RawBuffer<{element}>);
            extern trusted func opaque();
            func inspect(pos owner: &Owner) requires(trusted live(owner)) executes(pure, total) {{}}

            trusted func create_buffer(pos anchor: &mut Owner) -> std.memory.RawBuffer<{element}>
            {{
                let mut storage: std.memory.RawBuffer<{element}> = match trusted std.memory.RawBuffer<{element}>(capacity = 1)
                {{
                    case Ok(buffer) {{ yield buffer; }}
                    case Error(_) {{ panic("buffer layout failed"); }}
                }};
                trusted internal std.memory.initialize_spare<{element}>(&mut storage, {value});
                return storage;
            }}

            trusted func caller()
            {{
                let mut owner = make();
                accept(create_buffer(&mut owner));
                opaque();
                inspect(&owner);
            }}
        "#
        );

        let compilation =
            standard_library_compilation(sources.into_iter().chain([source.as_str()]));

        let diagnostics = compilation.check_diagnostics();

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &compilation,
            "create_buffer",
        );

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "element={element}: {diagnostics:#?}, summary={summary:#?}"
        );

        if !valid {
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.kind()
                    == bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
                "{diagnostics:#?}"
            );
        }
    }
}
