use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn ordinary_observers_supply_extents_to_trusted_memory_operations() {
    for (postcondition, valid) in [("ensures(result == count)", true), ("", false)] {
        let source = format!(
            r#"
                trusted module app;

                using core.memory;

                func observe(pos count: usize) -> usize
                    {postcondition}
                    executes(pure, total)
                {{
                    return count;
                }}

                trusted func copy(pos source: DataPointer<u8>, pos destination: DataPointer<u8>, count: usize)
                    requires(
                        trusted core.memory.valid_read<u8>(pointer = source, count = count),
                        trusted core.memory.valid_write<u8>(pointer = destination, count = count),
                        trusted core.memory.initialized_range_as<u8>(pointer = source, count = count),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    let observed: usize = observe(count);

                    core.memory.copy_overlapping<u8>(source, destination, count = observed);
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn conditional_memory_obligations_accept_smaller_proven_ranges() {
    for (condition, valid) in [
        (
            "count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count)",
            true,
        ),
        (
            "count == 0 && trusted core.memory.valid_write<u8>(pointer = pointer, count = count)",
            false,
        ),
        (
            "count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count + 1)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            extern trusted func consume_range(pos pointer: DataPointer<u8>, count: usize)
                requires({condition});

            trusted func caller(pos pointer: DataPointer<u8>, extent: usize, count: usize)
                requires(
                    count > 0,
                    count <= extent,
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = extent),
                )
            {{
                trusted consume_range(pointer, count = count);
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

    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;
        extern trusted func consume_range(pos pointer: DataPointer<u8>, count: usize)
            requires(count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count));

        trusted func caller(pos pointer: DataPointer<u8>, count: usize)
            requires(count == 0 || trusted core.memory.valid_write<u8>(pointer = pointer, count = count))
        {
            trusted consume_range(pointer, count = count);
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
fn closed_memory_ranges_can_supply_smaller_checked_ranges() {
    for (range, valid) in [(0, false), (1, true), (2, true)] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func write_byte(pos pointer: DataPointer<u8>)
                requires(
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = {range}),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                core.memory.write<u8>(pointer, 1);
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
fn checked_nonempty_ranges_supply_one_element_obligations() {
    for (guard, valid) in [("count > 0", true), ("count == 0", false)] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            trusted func write_byte(pos pointer: DataPointer<u8>, count: usize)
                requires(
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = count),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                )
                uses(raw_memory, unchecked_init)
            {{
                if {guard} {{ core.memory.write<u8>(pointer, 1); }}
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
fn copied_range_extents_describe_storage_without_owning_it() {
    let compilation = compilation(
        r#"
        trusted module app;

        using core.memory;

        trusted func extent(pos pointer: DataPointer<u8>) -> usize
            requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1))
            ensures(
                trusted core.memory.valid_read<u8>(
                    pointer = pointer,
                    count = result,
                ),
            )
        {
            return 1;
        }

        func caller()
        {
            let value: u8 = 1;
            let pointer = core.memory.address_of<u8>(&value);
            let count: usize = extent(pointer);
            let copied: usize = count;
            _ = copied;
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}
