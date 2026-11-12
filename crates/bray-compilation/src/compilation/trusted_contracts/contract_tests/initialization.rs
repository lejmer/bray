use crate::test_support::{compilation};

#[test]
fn initialized_protected_storage_transfers_and_consumes_its_witness() {
    for (intervening, valid) in [
        ("", true),
        (
            "_ = trusted core.memory.move_initialized<u32>(&mut storage);",
            false,
        ),
        ("storage = core.memory.uninit<u32>();", false),
    ] {
        let source = format!(
            r#"
                trusted module app;

                using core.memory;

                trusted func probe() -> u32
                    uses(unchecked_init)
                {{
                    let mut storage = core.memory.uninit_from<u32>(42);
                    {intervening}
                    return trusted core.memory.move_initialized<u32>(&mut storage);
                }}
            "#,
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );
    }
}

#[test]
fn closed_writes_preserve_storage_validity_but_opaque_calls_revoke_it() {
    for (intervening, valid) in [("", true), ("harmless();", true), ("opaque();", false)] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            func harmless() {{}}
            extern func opaque();
            trusted func roundtrip(pos pointer: DataPointer<u8>) -> u8
                requires(
                    trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                core.memory.write<u8>(pointer, 1);
                {intervening}
                return core.memory.read<u8>(pointer);
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn returned_terminal_storage_keeps_initialization_with_its_owner() {
    for (producer, intervening, valid) in [
        ("produce()", "", true),
        ("(1, core.memory.uninit<u32>())", "", false),
        (
            "produce()",
            "terminal.1 = core.memory.uninit<u32>();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func produce() -> (u32, Uninit<u32>)
                ensures(result.0 != 1 || trusted core.memory.uninit_initialized<u32>(storage = &result.1))
            {{
                let mut storage: Uninit<u32> = core.memory.uninit<u32>();
                _ = core.memory.uninit_write<u32>(&mut storage, 7);
                return (1, storage);
            }}
            trusted func consume_terminal() -> u32
                uses(unchecked_init)
            {{
                let mut terminal: (u32, Uninit<u32>) = {producer};
                if terminal.0 == 1
                {{
                    {intervening}
                    return core.memory.move_initialized<u32>(&mut terminal.1);
                }}
                return 0;
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn conditional_initialization_guarantees_from_field_observers() {
    for (status, branch, valid) in [
        (
            "1",
            "if state == 1 { _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            true,
        ),
        (
            "COMPLETE",
            "if state == COMPLETE { _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            true,
        ),
        (
            "1",
            "match state { case 1 { _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            true,
        ),
        (
            "COMPLETE",
            "match state { case COMPLETE { _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            true,
        ),
        (
            "COMPLETE",
            "if state == OTHER { _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            false,
        ),
        (
            "COMPLETE",
            "match state { case OTHER { _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            false,
        ),
        (
            "COMPLETE",
            "if state == COMPLETE { storage.value = core.memory.uninit<T>(); _ = trusted core.memory.move_initialized<T>(&mut storage.value); }",
            false,
        ),
        (
            "COMPLETE",
            "match state { case COMPLETE { storage.value = core.memory.uninit<T>(); _ = trusted core.memory.move_initialized<T>(&mut storage.value); } case _ {} }",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            const COMPLETE: u32 = 1;
            const OTHER: u32 = 2;
            struct Slots<T> {{ mut value: Uninit<T>; state: u32; }}
            trusted func observe<T>(pos storage: &mut Slots<T>) -> u32
                ensures(result != {status} || trusted core.memory.uninit_initialized<T>(storage = &storage.value))
            {{
                return storage.state;
            }}
            trusted func consume_initialized_field<T>(pos storage: &mut Slots<T>)
                uses(unchecked_init)
            {{
                let state: u32 = trusted observe<T>(storage);
                {branch}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn conditional_initialization_guarantees_reach_borrowed_fields() {
    for (guard, mutation, valid) in [
        ("state == 1", "", true),
        ("state == 0", "", false),
        (
            "state == 1",
            "storage.value = core.memory.uninit<u8>();",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Slots {{ mut value: Uninit<u8>; }}

            trusted func initialize(pos storage: &mut Slots, fail: bool) -> u32
                ensures(result != 1 || trusted core.memory.uninit_initialized<u8>(storage = &storage.value))
            {{
                if fail {{ return 0; }}
                _ = core.memory.uninit_write<u8>(&mut storage.value, 42);
                return 1;
            }}

            trusted func consume_initialized_field(pos storage: &mut Slots, fail: bool)
                uses(unchecked_init)
            {{
                let state: u32 = trusted initialize(storage, fail = fail);
                {mutation}
                if {guard} {{ _ = trusted core.memory.move_initialized<u8>(&mut storage.value); }}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn initialized_storage_requirements_supply_consuming_operations() {
    for (signature, operation) in [
        (
            "pos storage: &mut Uninit<u8>",
            "core.memory.move_initialized<u8>(storage)",
        ),
        (
            "pos mut storage: Uninit<u8>",
            "core.memory.assume_initialized<u8>(storage)",
        ),
    ] {
        let argument = "&storage";

        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func consume_initialized({signature}) -> u8
                requires(trusted core.memory.uninit_initialized<u8>(storage = {argument}))
                uses(unchecked_init)
            {{
                return trusted {operation};
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
    }
}
