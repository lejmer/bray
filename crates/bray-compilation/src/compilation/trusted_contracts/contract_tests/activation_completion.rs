use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn activation_retirement_does_not_preserve_dispatch_ownership() {
    for (operation, valid) in [(0, true), (5, false)] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
            ) -> bray.runtime.bootstrap.activation.Progress
                requires(
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = cursor.pointer),
                )
            {{
                _ = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, {operation}, 0);
                let (progress, _, _, _, _) = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 0, 0);
                return progress;
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn activation_deferred_retirement_preserves_dispatch_ownership() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.activation.callback;
        using internal bray.runtime.bootstrap.activation.execution;
        using internal bray.runtime.bootstrap.activation.run;
        using internal bray.runtime.bootstrap.entry;
        trusted func production_probe(
            pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
            pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
            pos ready: bray.runtime.bootstrap.activation.callback.Ready,
        )
            requires(
                trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = cursor.pointer),
            )
        {
            let (_, _, _, status, _) = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 5, 0);

            if status == 0
            {
                return;
            }

            _ = trusted bray.runtime.bootstrap.activation.execution.read(cursor);
        }
        "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .expect("fixture callable must lower successfully");

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
fn activation_retirement_requires_retained_native_destinations() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = other, output = output, records = records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = other_output, records = records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = other_records, incidents = incidents))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = records, incidents = incidents))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            using core.memory;
            trusted func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos callback: FunctionPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos ready: FunctionPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos output: DataPointer<bray.runtime.bootstrap.activation.Progress>,
                pos other_output: DataPointer<bray.runtime.bootstrap.activation.Progress>,
                pos records: DataPointer<bray.runtime.bootstrap.ReportRecords>,
                pos incidents: DataPointer<bray.runtime.bootstrap.ReportRecords>,
                pos other_records: DataPointer<bray.runtime.bootstrap.ReportRecords>,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = pointer),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.retiring(pointer = pointer),
                    trusted bray.runtime.bootstrap.activation.execution.observed(pointer = pointer),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.activation.Progress>(pointer = output, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Progress>(pointer = output),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportRecords>(pointer = records, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportRecords>(pointer = records),
                )
                {requirement}
            {{
                return trusted bray.runtime.bootstrap.activation.execution.execute(pointer, callback, ready, 5, 0, output, records, incidents);
            }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn activation_completion_requires_the_exact_admitted_destination() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = other, address = destination))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = cursor.pointer, address = other_destination))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.execution.completion_destination(pointer = cursor.pointer, address = destination))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
                pos destination: usize,
                pos other_destination: usize,
            )
                requires(
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                )
                {requirement}
            {{
                _ = trusted bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 4, destination);
            }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            lowered.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn inactive_cleanup_retains_only_its_independent_native_destination() {
    for (authority, valid) in [
        ("", false),
        ("trusted destination(output = outcome),", true),
        ("trusted destination(output = other),", false),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.activation.inactive;

            using internal bray.runtime.bootstrap;
            using core.memory;
            using std.memory;

            func production_probe(
                pos mut cursor: bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
                pos outcome: DataPointer<bray.runtime.bootstrap.ReportOutcome>,
                pos other: DataPointer<bray.runtime.bootstrap.ReportOutcome>,
            )
                requires(
                    {authority}
                    trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportOutcome>(pointer = outcome, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportOutcome>(pointer = outcome),
                )
            {{
                _ = trusted resolve(cursor, callback, ready, 0, outcome);
            }}
            "#,
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
