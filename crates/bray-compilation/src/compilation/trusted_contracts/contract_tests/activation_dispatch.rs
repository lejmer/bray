use crate::test_support::{source_function_body_key};
use super::fixtures::production_host_compilation;

#[test]
#[ignore = "tier:proof"]
fn inactive_exit_authority_requires_the_unchanged_whole_owner() {
    for (body, valid) in [
        ("inspect(&selected);", true),
        (r#"
            let moved = selected;
            inspect(&moved);
        "#, true),
        (r#"
            selected.cleanup = none;
            inspect(&selected);
        "#, false),
        (
            r#"
                let fake: AttachmentExit =
                {
                    identity = selected.identity,
                    native_thread = selected.native_thread,
                    cleanup = none
                };
                inspect(&fake);
            "#,
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host;

            trusted func inspect(pos owner: &AttachmentExit)
                requires(trusted attachment_exit_inactive(owner = owner))
            {{
            }}

            trusted func production_probe(pos mut selected: AttachmentExit)
                requires(trusted attachment_exit_inactive(owner = &selected))
            {{
                {body}
            }}
        "#
        );

        super::fixtures::assert_production_contract(&caller, (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven), "inspect(", "attachment_exit_inactive");
    }
}

#[test]
#[ignore = "tier:proof"]
fn activation_dispatch_requires_exact_native_ownership() {
    let cases = [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = other))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &cursor))",
            true,
        ),
    ].into_iter().enumerate().map(|(case, (requirements, valid))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            trusted func production_probe_{case}(
                pos cursor: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: &bray.runtime.bootstrap.activation.run.Cursor,
                pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
                pos ready: bray.runtime.bootstrap.activation.callback.Ready,
            ) -> bray.runtime.bootstrap.activation.Progress
                requires(
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                )
                {requirements}
            {{
                let (progress, _, _, _, _) = bray.runtime.bootstrap.activation.execution.invoke(cursor, callback, ready, 0, 0);
                return progress;
            }}
        "#
        );

        (caller, (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven), "let (progress, _, _, _, _) = bray.runtime.bootstrap.activation.execution.invoke", "selected")
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn native_activation_dispatch_requires_both_exact_callback_addresses() {
    let cases = [
        ("", false),
        (
            "requires(trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Invoke>(pointer = other), trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready))",
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
    ].into_iter().enumerate().map(|(case, (requirement, valid))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            using core.memory;
            trusted func production_probe_{case}(
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
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.activation.execution.destinations(pointer = pointer, output = output, records = records, incidents = incidents),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.activation.Progress>(pointer = output, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Progress>(pointer = output),
                    trusted core.memory.valid_write<bray.runtime.bootstrap.ReportRecords>(pointer = records, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.ReportRecords>(pointer = records),
                )
                {requirement}
            {{
                return bray.runtime.bootstrap.activation.execution.execute(pointer, callback, ready, 0, 0, output, records, incidents);
            }}
            "#
        );

        (caller, (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven), "return bray.runtime.bootstrap.activation.execution.execute", "callable_address_valid")
    });

    super::fixtures::assert_production_contracts(cases);
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
    let invoke = "_ = bray.runtime.bootstrap.activation.callback.invoke(callback, owner);";

    use bray_diagnostics::DiagnosticKind;

    let requirement = DiagnosticKind::CheckingTrustedObligationNotProven;
    let separation = DiagnosticKind::CheckingTrustedWitnessTransferNotProven;

    let cases = [
        (String::new(), invoke, Some(requirement), invoke, "authorized"),
        (authorized.to_owned(), invoke, Some(requirement), invoke, "outputs_owned"),
        (outputs.to_owned(), invoke, Some(requirement), invoke, "authorized"),
        (format!("{other_authorized}, {outputs}"), invoke, Some(requirement), invoke, "authorized"),
        (format!("{authorized}, {other_outputs}"), invoke, Some(requirement), invoke, "outputs_owned"),
        (format!("{authorized}, {outputs}"), invoke, None, invoke, ""),
        (
            format!("{authorized}, {outputs}"),
            r#"
                let reconstructed: bray.runtime.bootstrap.activation.callback.Invocation =
                {
                    acquisition = owner.acquisition,
                    execution = other.execution,
                    descriptor = owner.descriptor,
                    context = owner.context,
                    operation = owner.operation,
                    progress = owner.progress,
                    records = owner.records,
                    admission = owner.admission
                };

                _ = trusted bray.runtime.bootstrap.activation.callback.invoke(callback, reconstructed);
            "#,
            Some(separation),
            "acquisition = owner.acquisition",
            "authorized",
        ),
    ].into_iter().enumerate().map(|(case, (requirements, body, expected, site, predicate))| {
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
            trusted func production_probe_{case}(
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

        (caller, expected, site, predicate)
    });

    super::fixtures::assert_production_contracts(cases);

    let caller = r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.activation;
        using internal bray.runtime.bootstrap.entry;
        trusted func production_probe(
            pos callback: bray.runtime.bootstrap.activation.callback.Invoke,
            pos owner: bray.runtime.bootstrap.activation.callback.Invocation,
        )
            requires(
                trusted bray.runtime.bootstrap.entry.native_entry(),
                trusted bray.runtime.bootstrap.activation.callback.authorized(owner = &owner.acquisition),
                trusted bray.runtime.bootstrap.activation.callback.outputs_owned(owner = &owner.acquisition),
            )
        {
            _ = trusted bray.runtime.bootstrap.activation.callback.invoke(callback, owner);
            _ = trusted bray.runtime.bootstrap.activation.callback.invoke(callback, owner);
        }
    "#;

    let compilation = production_host_compilation(caller);

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingUseOfMovedStorage,
    );

    let source = source_function_body_key(&compilation, "production_probe")
        .source().syntax().source_id();

    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage
            && super::fixtures::diagnostic_at_call(
                diagnostic,
                source,
                caller,
                "_ = trusted bray.runtime.bootstrap.activation.callback.invoke",
            )
    }), "the attempted second invocation must reject the moved owner: {:?}", checked.diagnostics());
}

#[test]
#[ignore = "tier:proof"]
fn active_snapshot_requires_exact_dispatch_ownership_and_retained_output() {
    let cases = [
        ("", "pointer", "output", Some("owned")),
        ("other", "pointer", "output", Some("owned")),
        ("pointer", "other", "output", Some("active_destination")),
        ("pointer", "pointer", "other_output", Some("active_destination")),
        ("pointer", "pointer", "output", None),
    ].into_iter().enumerate().map(|(case, (owner, retained, output, missing))| {
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
            trusted func production_probe_{case}(
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
                return bray.runtime.bootstrap.activation.requirements.snapshot(pointer, 0, 0, 0, output);
            }}
            "#
        );

        (
            caller,
            missing.map(|_| bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.requirements.snapshot(pointer, 0, 0, 0, output)",
            missing.unwrap_or_default(),
        )
    });

    super::fixtures::assert_production_contracts(cases);
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
            "let mut forged: bray.runtime.bootstrap.activation.run.Cursor = { root = cursor.root, pointer = other }; let header = bray.runtime.bootstrap.activation.execution.access(&mut forged); header.state = 1;",
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
            {{
                {body}
            }}
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
