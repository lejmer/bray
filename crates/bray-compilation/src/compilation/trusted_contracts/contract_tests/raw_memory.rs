use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{callable_dependencies};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_sync_compilation};

#[test]
fn copied_addresses_keep_value_dependencies_without_the_container_loan() {
    for mutation in ["", "source = 1;"] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker
            {{
            }}
            struct Reference
            {{
                acquisition: Marker;
                pointer: DataPointer<u32>;
            }}
            trusted predicate acquired(owner: &Marker);
            trusted func fresh(pos pointer: DataPointer<u32>) -> Reference
                ensures(trusted acquired(owner = &result.acquisition))
            {{
                return
                {{
                    acquisition = Marker {{}}, pointer = pointer
                }};
            }}
            const func borrow_reference(pos owner: &Reference) -> &Reference
                executes(pure, total)
            {{
                return owner;
            }}
            trusted func retain(pos owner: &Reference) -> Reference
                requires(trusted acquired(owner = &owner.acquisition))
                ensures(trusted acquired(owner = &result.acquisition))
            {{
                return
                {{
                    acquisition = Marker {{}}, pointer = owner.pointer
                }};
            }}
            func release_reference(pos owner: Reference)
                requires(trusted acquired(owner = &owner.acquisition))
            {{
            }}
            func caller()
            {{
                let mut source: u32 = 0;
                let first = fresh(core.memory.address_of<u32>(&source));
                let second = retain(borrow_reference(&first));
                release_reference(first);
                {mutation}
                release_reference(second);
            }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("address retention keeps its checked producer contracts");

        assert!(
            !checked.diagnostics().has_errors(),
            "{source}: {checked:#?}"
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn returned_address_origin_is_independent_of_its_lifetime_anchor() {
    let application = production_sync_compilation(
        r#"
        trusted module app;

        using std.memory;
        trusted func exact(pos owner: &bool, pos payload: &bool) -> &bool
            uses(raw_memory, unchecked_alias)
        {
            let pointer = std.memory.address_of<bool>(payload);
            return trusted std.memory.borrow_from<bool, bool>(owner, pointer);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "exact"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "exact");

    let template = callable_dependencies(&application, callable.into());

    assert_eq!(
        template.borrows(),
        &[bray_symbols::DependencyBorrowRelation::new(
            Some(bray_symbols::DependencySubject::root(
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(1))
            )),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            bray_symbols::DependencyReferenceKind::Borrow,
        )]
    );
}

#[test]
fn raw_entry_addresses_do_not_replace_borrowed_owners_at_completion() {
    for (body, valid) in [
        (
            r#"
            let recognize = trusted lambda(pos input: &DataPointer<Owner>, pos address: DataPointer<Owner>)
                requires(trusted slot(pointer = address))
                ensures(trusted borrowed_slot(input = input))
                executes(pure, total) {};
            let observe = lambda(pos input: &DataPointer<Owner>)
                requires(trusted borrowed_slot(input = input))
                executes(pure, total) {};
            trusted recognize(&pointer, pointer);
            observe(&pointer);
            "#,
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); visit(owner, callback); visit(owner, callback);",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); loop { visit(owner, callback); if !again { return; } }",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); clear_slot(capability, pointer); visit(owner, callback);",
            true,
        ),
        (
            "let owner = borrow_owner(&pointer, pointer); pointer = std.memory.null<Owner>(); visit(owner, callback);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
                backing: DataPointer<u8>;
                mut slot: u32?;
            }}
            callable Callback = @abi(c) trusted func();
            trusted predicate slot(pointer: DataPointer<Owner>);
            trusted predicate borrowed_slot(input: &DataPointer<Owner>);
            trusted predicate held(marker: &Marker);
            trusted func borrow_owner(pos input: &DataPointer<Owner>, pos address: DataPointer<Owner>) -> &Owner
                requires(trusted slot(pointer = address))
                ensures(trusted held(marker = &result.marker))
                executes(pure, total)
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos address: DataPointer<Owner>)
                    requires(trusted slot(pointer = address))
                    ensures(
                        trusted core.memory.valid_read<Owner>(pointer = address, count = 1),
                        trusted core.memory.aligned_for<Owner>(pointer = address),
                        trusted core.memory.initialized_as<Owner>(pointer = address),
                        trusted core.memory.shared_alias_valid<Owner>(pointer = address),
                        trusted core.memory.epoch_current<Owner>(pointer = address),
                        trusted core.memory.synchronized_access<Owner>(pointer = address),
                        trusted core.memory.movement_stable<Owner>(pointer = address),
                        trusted core.memory.finalization_pending<Owner>(pointer = address)
                    )
                    executes(pure, total) {{}};
                trusted permit(address);
                return trusted std.memory.borrow_from<Owner, DataPointer<Owner>>(input, address);
            }}
            trusted func clear_slot(pos capability: &mut bool, pos address: DataPointer<Owner>)
                requires(trusted slot(pointer = address))
                uses(raw_memory, unchecked_alias)
            {{
                let permit = trusted lambda(pos address: DataPointer<Owner>)
                    requires(trusted slot(pointer = address))
                    ensures(
                        trusted core.memory.valid_write<Owner>(pointer = address, count = 1),
                        trusted core.memory.aligned_for<Owner>(pointer = address),
                        trusted core.memory.initialized_as<Owner>(pointer = address),
                        trusted core.memory.exclusive_alias_valid<Owner>(pointer = address),
                        trusted core.memory.epoch_current<Owner>(pointer = address),
                        trusted core.memory.synchronized_access<Owner>(pointer = address),
                        trusted core.memory.movement_stable<Owner>(pointer = address),
                        trusted core.memory.finalization_pending<Owner>(pointer = address)
                    )
                    executes(pure, total) {{}};
                trusted permit(address);
                let borrowed = trusted std.memory.borrow_mut_from<Owner, bool>(capability, address);
                borrowed.slot = none;
            }}
            trusted func visit(pos owner: &Owner, pos callback: Callback)
                requires(trusted held(marker = &owner.marker))
                ensures(trusted held(marker = &owner.marker))
                uses(foreign_call)
            {{
                callback();
            }}
            func caller(pos mut pointer: DataPointer<Owner>, pos callback: Callback, pos again: bool, pos capability: &mut bool)
                requires(trusted slot(pointer = pointer))
            {{
                {body}
            }}
        "#
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn typed_raw_transfers_preserve_only_unmoved_marker_free_owners() {
    for field in ["u64", "DataPointer<u8>"] {
        for (pointee, operation, valid) in [
            ("Owner", "let taken = trusted core.memory.read<$POINTEE>(address);", false),
            ("Other", "let taken = trusted core.memory.read<$POINTEE>(address);", true),
            ("Containing", "let taken = trusted core.memory.read<$POINTEE>(address);", false),
            ("u64", "trusted core.memory.write<u64>(address, 0);", false),
        ] {
        let source = r#"trusted module app;
using core.memory;
struct Owner
{
    address: $FIELD;
}
struct Other
{
    value: u64;
}
struct Containing
{
    owner: Owner;
}
trusted predicate live(owner: &Owner);
extern trusted func pointer(pos owner: &Owner) -> DataPointer<$POINTEE>
    requires(trusted live(owner))
    ensures(
        trusted live(owner),
        trusted core.memory.valid_read<$POINTEE>(pointer = result, count = 1),
        trusted core.memory.valid_write<$POINTEE>(pointer = result, count = 1),
        trusted core.memory.initialized_as<$POINTEE>(pointer = result),
        trusted core.memory.aligned_for<$POINTEE>(pointer = result)
    )
    executes(pure, total);
func observe(pos owner: &Owner)
    requires(trusted live(owner))
    executes(pure, total)
{
}
trusted func caller(pos owner: &Owner)
    requires(trusted live(owner))
    uses(raw_memory, unchecked_init)
{
    let address = pointer(owner);
    $OPERATION
    observe(owner);
}
"#.replace("$FIELD", field).replace("$OPERATION", operation).replace("$POINTEE", pointee);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
        }
    }
}

#[test]
fn raw_writes_invalidate_only_the_reached_witness_observations() {
    for (field, ty, value, valid) in [
        ("payload", "u64", "1", true),
        ("lease.active", "bool", "false", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted predicate live(value: &Lease);
            struct Lease
            {{
                mut active: bool;
            }}
            struct Access
            {{
                mut payload: u64;
                mut lease: Lease;
            }}
            trusted func acquire() -> Access
                ensures(trusted live(&result.lease))
            {{
                return
                {{
                    payload = 0, lease =
                    {{
                        active = true
                    }}
                }};
            }}
            func observe(pos lease: &Lease)
                requires(trusted live(lease))
                ensures(trusted live(lease)) executes(pure, total) {{}}
            trusted func caller()
                uses(raw_memory, unchecked_init)
            {{
                let mut access = acquire();
                observe(&access.lease);
                let pointer = core.memory.address_of_mut<{ty}>(&mut access.{field});
                let alias = pointer;
                trusted core.memory.write<{ty}>(alias, {value});
                observe(&access.lease);
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn raw_retirement_distinguishes_private_descriptors_from_borrowed_storage() {
    for (parameter, operation, valid) in [
        ("Owner", "", true),
        ("&Owner", "", false),
        ("Owner", "_ = trusted core.memory.read<u8>(owner.pointer);", false),
    ] {
        let source = r#"
            trusted module app;

            using core.memory;

            struct Marker
            {
            }

            trusted predicate live(marker: &Marker);

            struct Owner
            {
                marker: Marker;
                pointer: DataPointer<u8>;

                trusted destruct()
                    requires(trusted live(&self.marker))
                {
                }
            }

            func observe(pos owner: &Owner)
                requires(trusted live(&owner.marker))
            {
            }

            trusted func probe(pos owner: PARAMETER, pos allocation: DataPointer<u8>)
                requires(
                    trusted live(&owner.marker),
                    trusted core.memory.owned_allocation(pointer = allocation, bytes = 1, align = 1),
                    trusted core.memory.valid_read<u8>(pointer = owner.pointer, count = 1),
                    trusted core.memory.aligned_for<u8>(pointer = owner.pointer),
                    trusted core.memory.initialized_as<u8>(pointer = owner.pointer)
                )
                uses(manual_alloc, raw_memory)
            {
                trusted core.memory.deallocate(allocation, bytes = 1, align = 1);
                READ_POINTEE
                observe(&owner);
            }
        "#.replace("PARAMETER", parameter).replace("READ_POINTEE", operation);
        let application = compilation(&source);
        let checked = application.lowered_unit(source_function_body_key(&application, "probe"))
            .expect("raw retirement probe lowers");

        assert_eq!(!checked.diagnostics().has_errors(), valid, "{parameter}, {operation}: {:?}", checked.diagnostics());

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn raw_extraction_transfers_a_complete_nullable_witness() {
    for (extra_read, use_owner, valid) in [
        ("", "observe(&complete);", true),
        ("", "let fabricated: Owner = { epoch = complete.epoch }; observe(&fabricated);", false),
        ("_ = trusted core.memory.read<T>(pointer);", "observe(&complete);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate live(owner: &Owner);
            trusted func acquire() -> Owner
                ensures(trusted live(owner = &result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func observe(pos owner: &Owner)
                requires(trusted live(owner = owner))
            {{
            }}
            trusted func exchange<T>(pos storage: &mut T, pos value: T) -> T
                uses(raw_memory, unchecked_init)
            {{
                let pointer = core.memory.address_of_mut<T>(storage);
                let previous = trusted core.memory.read<T>(pointer);
                {extra_read}
                trusted core.memory.write<T>(pointer, value);
                return previous;
            }}
            func caller(pos present: bool)
            {{
                let mut owner: Owner? = if present {{ yield acquire(); }} else {{ yield none; }};
                let moved = exchange<(Owner?)>(&mut owner, none);
                if let ?complete = moved {{ {use_owner} }}
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
    }
}

#[test]
fn raw_read_uses_declared_storage_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        trusted func read_byte(pos pointer: DataPointer<u8>) -> u8
            requires(
                trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                trusted core.memory.aligned_for<u8>(pointer = pointer),
                trusted core.memory.initialized_as<u8>(pointer = pointer),
            )
            uses(raw_memory)
        {
            return trusted core.memory.read<u8>(pointer);
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn checked_raw_primitives_complete_only_on_their_required_storage_domains() {
    for (requirements, valid) in [
        (
            "trusted core.memory.valid_read<u8>(pointer = pointer, count = 1), trusted core.memory.aligned_for<u8>(pointer = pointer), trusted core.memory.initialized_as<u8>(pointer = pointer)",
            true,
        ),
        (
            "trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func read_byte(pos pointer: DataPointer<u8>) -> u8
                requires({requirements}) executes(pure, total) uses(raw_memory)
            {{
                return trusted core.memory.read<u8>(pointer);
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }

    for (pointee, result, access, body) in [
        (
            "u8",
            "u8",
            "write",
            "trusted core.memory.write<u8>(pointer, 1); return 0;",
        ),
        (
            "Owner",
            "Owner",
            "read",
            "return trusted core.memory.read<Owner>(pointer);",
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Owner
            {{
                value: u8;
            }}
            trusted func mutate(pos pointer: DataPointer<{pointee}>) -> {result}
                requires(
                    trusted core.memory.valid_{access}<{pointee}>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<{pointee}>(pointer = pointer),
                    trusted core.memory.initialized_as<{pointee}>(pointer = pointer),
                )
                executes(pure, total) uses(raw_memory, unchecked_init)
            {{
                {body}
            }}
        "#
        );

        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }

    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        trusted func allocate() -> DataPointer<u8>
            executes(total)
            uses(manual_alloc)
        {
            return trusted core.memory.allocate(bytes = 1, align = 1);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}
