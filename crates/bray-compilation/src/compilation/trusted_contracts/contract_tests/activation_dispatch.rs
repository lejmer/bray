use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn inactive_exit_authority_requires_the_unchanged_whole_owner() {
    for (body, valid) in [
        ("", true),
        ("let moved = selected;", true),
        ("selected.ready = true;", false),
        ("selected.cleanup = none;", false),
        (
            "let fake: AttachmentExit = { identity = selected.identity, native_thread = selected.native_thread, cleanup = none, ready = false, incidents = none };",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;
            func production_probe(pos mut selected: AttachmentExit)
                requires(!selected.ready, trusted attachment_exit_inactive(owner = &selected))
            {{ {body} }}
        "#
        );

        let compilation = production_host_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn activation_dispatch_requires_exact_native_ownership() {
    for (requirements, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = other))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor))",
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
                pos other: &bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
            ) -> bray.runtime.bootstrap.activation.Progress
                requires(trusted bray.runtime.bootstrap.entry.native_entry())
                {requirements}
            {{
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
fn native_activation_dispatch_requires_both_exact_callback_addresses() {
    for (requirement, valid) in [
        ("", false),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = other))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback), trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = other_ready))",
            false,
        ),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = callback), trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready))",
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
                pos callback: FunctionPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos ready: FunctionPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos other_ready: FunctionPointer<bray.runtime.bootstrap.activation.callback.Ready>,
                pos other: FunctionPointer<bray.runtime.bootstrap.activation.callback.Invoke>,
                pos output: DataPointer<bray.runtime.bootstrap.activation.Progress>,
                pos records: DataPointer<bray.runtime.bootstrap.ReportRecords>,
                pos incidents: DataPointer<bray.runtime.bootstrap.ReportRecords>,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = pointer),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = records, incidents = incidents),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.activation.Progress>(pointer = output, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Progress>(pointer = output),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportRecords>(pointer = records, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportRecords>(pointer = records),
                )
                {requirement}
            {{
                return trusted bray.runtime.bootstrap.activation.execution.execute(pointer, callback, ready, 0, 0, output, records, incidents);
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
fn activation_callback_requires_one_exact_authorized_invocation() {
    let authorized =
        "trusted bray.runtime.bootstrap.activation.callback.authorized(owner = &owner.acquisition)";

    let outputs = "trusted bray.runtime.bootstrap.activation.callback.outputs_owned(owner = &owner.acquisition)";

    let other_authorized =
        "trusted bray.runtime.bootstrap.activation.callback.authorized(owner = &other.acquisition)";

    let other_outputs = "trusted bray.runtime.bootstrap.activation.callback.outputs_owned(owner = &other.acquisition)";
    let invoke = "_ = trusted callback(owner);";

    for (requirements, body, valid) in [
        (String::new(), invoke, false),
        (authorized.to_owned(), invoke, false),
        (outputs.to_owned(), invoke, false),
        (format!("{other_authorized}, {outputs}"), invoke, false),
        (format!("{authorized}, {other_outputs}"), invoke, false),
        (format!("{authorized}, {outputs}"), invoke, true),
        (
            format!("{authorized}, {outputs}"),
            r#"
                let reconstructed: bray.runtime.bootstrap.activation.callback.Invocation = {
                    acquisition = owner.acquisition,
                    execution = other.execution,
                    descriptor = owner.descriptor,
                    context = owner.context,
                    operation = owner.operation,
                    progress = owner.progress,
                    records = owner.records,
                };

                _ = trusted callback(reconstructed);
            "#,
            false,
        ),
        (
            format!("{authorized}, {outputs}"),
            "let returned = trusted callback(owner); _ = trusted callback(returned);",
            false,
        ),
    ] {
        let requirements = if requirements.is_empty() {
            requirements
        } else {
            format!(", {requirements}")
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            trusted func production_probe(
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos owner: bray.runtime.bootstrap.activation.callback.Invocation,
                pos other: bray.runtime.bootstrap.activation.callback.Invocation,
            )
                requires(trusted bray.runtime.bootstrap.entry.native_entry(){requirements})
            {{
                {body}
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
fn active_snapshot_requires_exact_dispatch_ownership_and_retained_output() {
    for (owner, retained, output, valid) in [
        ("", "pointer", "output", false),
        ("other", "pointer", "output", false),
        ("pointer", "other", "output", false),
        ("pointer", "pointer", "other_output", false),
        ("pointer", "pointer", "output", true),
    ] {
        let ownership = if owner.is_empty() {
            String::new()
        } else {
            format!(
                "requires(trusted bray.runtime.bootstrap.activation.execution.owned(pointer = {owner}))"
            )
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using core.memory;
            trusted func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos output: DataPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos other_output: DataPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
            ) -> u32
                {ownership}
                requires(
                    trusted bray.runtime.bootstrap.activation.requirements.active_destination(pointer = {retained}, output = {output}),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Header>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Header>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Header>(pointer = pointer),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.snapshot(pointer, 0, 0, 0, output);
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
fn activation_header_access_retains_ownership_and_excludes_overlapping_borrows() {
    let mut failures = Vec::new();

    for (body, valid) in [
        (
            "_ = trusted bray.runtime.bootstrap.activation.execution.read(cursor); _ = trusted bray.runtime.bootstrap.activation.execution.read(cursor);",
            true,
        ),
        (
            "let header = trusted bray.runtime.bootstrap.activation.execution.access(cursor); header.state = 1;",
            true,
        ),
        (
            "let mut forged: bray.runtime.bootstrap.activation.run.Cursor = { root = cursor.root, pointer = other }; let header = trusted bray.runtime.bootstrap.activation.execution.access(&mut forged); header.state = 1;",
            false,
        ),
        (
            "let left = trusted bray.runtime.bootstrap.activation.execution.access(cursor); let right = trusted bray.runtime.bootstrap.activation.execution.access(cursor); left.state = 1; right.state = 2;",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            trusted func production_probe(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
            )
                requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor))
            {{ {body} }}
            "#
        ));

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        if !checked.diagnostics().has_errors() != valid {
            failures.push(format!("{body}: {:?}", checked.diagnostics()));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
