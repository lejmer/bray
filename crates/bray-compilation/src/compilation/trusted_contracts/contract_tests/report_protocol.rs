use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn production_report_consumers_check_their_callback_protocol() {
    let compilation = production_host_compilation("module app;");

    let names = [
        "dispose",
        "retain",
        "begin",
        "number",
        "consume_segment",
        "install",
        "compose",
        "exchange_record",
        "pop_record_value",
        "retain_primary",
        "accept_progress",
        "transfer",
        "retire",
        "acquire",
        "resolve",
        "execute",
        "invoke_retaining",
        "retained_address",
    ];

    let symbols = compilation
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

    let sources = symbols
        .functions()
        .iter()
        .filter_map(|function| {
            let symbol = function.id().into();
            let name = symbols.member_name(symbol)?;

            names.contains(&name.as_str()).then(|| {
                symbols
                    .declaration_syntax_anchor(symbol)
                    .expect("production source function has its declaration")
                    .source_id()
            })
        })
        .collect::<Vec<_>>();

    let diagnostics = compilation.check_diagnostics();

    let failures = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .primary_span()
                .is_some_and(|span| sources.contains(&span.source_id()))
        })
        .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
#[ignore = "tier:proof"]
fn native_report_addresses_retain_authority_only_for_the_unchanged_owner() {
    for (body, valid) in [
        (
            "let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            true,
        ),
        (
            "other.message_length = 0; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            true,
        ),
        (
            "primary.release_message = 0; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&primary);",
            false,
        ),
        (
            "let mut forged = trusted bray.runtime.bootstrap.empty_primary(); forged.release_message = primary.release_message; let callbacks = trusted bray.runtime.bootstrap.primary_callbacks(&forged);",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;
            trusted func production_probe(
                pos mut primary: bray.runtime.bootstrap.ReportPrimary,
                pos other: &mut bray.runtime.bootstrap.ReportPrimary,
            )
                requires(trusted bray.runtime.bootstrap.native_primary_callbacks(primary = &primary))
            {{
                {body}
            }}
        "#,
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
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
fn raw_typed_incident_headers_do_not_grant_disposal_authority() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.pool;
        using internal bray.runtime.bootstrap.report.incident;
        trusted func production_probe(pos pointer: DataPointer<bray.runtime.bootstrap.report.incident.Record>) -> u32
            requires(trusted bray.runtime.bootstrap.pool.protocol_access())
        {
            return trusted bray.runtime.bootstrap.report.incident.dispose(pointer, 1);
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
#[ignore = "tier:proof"]
fn retained_incident_values_require_the_exact_ownership_witness() {
    for (body, valid) in [
        (
            "let value = trusted bray.runtime.bootstrap.report.incident.read_record(pointer);",
            true,
        ),
        (
            "let value = bray.runtime.bootstrap.ReportValue.Error(forged);",
            false,
        ),
        (
            r#"
            let original = trusted bray.runtime.bootstrap.report.incident.read_record(pointer);
            let value = match consume original
            {
                case Panic(primary) { yield bray.runtime.bootstrap.ReportValue.Panic(primary); }
                case Error(record) { yield bray.runtime.bootstrap.ReportValue.Error(record); }
            };
            "#,
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.pool;
            using internal bray.runtime.bootstrap.report.incident;
            trusted func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.report.incident.Record>,
                pos forged: bray.runtime.bootstrap.report.incident.Record,
            ) -> u32
                requires(
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.report.incident.owned(pointer = pointer),
                )
            {{
                {body}
                return trusted bray.runtime.bootstrap.consume_values(
                    value,
                    bray.runtime.bootstrap.empty_records(),
                    0
                );
            }}
            "#,
        ));

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("retained incident probe must lower");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            checked.diagnostics(),
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
fn incident_outcome_destinations_require_distinct_exclusive_borrows() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.report.incident;
        trusted func production_probe(pos outcome: &mut ReportOutcome)
        {
            let pair = trusted bray.runtime.bootstrap.report.incident.destinations(outcome, outcome);
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_kind(
        checked.diagnostics(),
        DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
#[ignore = "tier:proof"]
fn source_report_chain_requires_unchanged_exclusive_ownership() {
    for (body, valid) in [
        ("let owner = trusted admit(2);", true),
        (
            "let owner: Owner = { records = bray.runtime.bootstrap.ReportRecords { head = 1, tail = 1, count = 1 }, status = 0 };",
            false,
        ),
        (
            "let owner = trusted admit(2); let copied: Owner = { records = owner.records, status = owner.status };",
            false,
        ),
        (
            "let mut owner = trusted admit(2); owner.records.head = 1;",
            false,
        ),
        (
            "let mut owner = trusted admit(2); let other = trusted admit(2); owner.records = other.records;",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.report.chain;
            using internal bray.runtime.bootstrap;
            func production_probe()
            {{
                {body}
            }}
        "#
        );

        let compilation = production_host_compilation(&source);

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
fn production_report_pool_mutation_requires_protocol_authority() {
    for operation in [
        "let mut guard = trusted bray.runtime.bootstrap.pool.lock(); let pool = guard.get(); pool.credits = 0; pool.free = 1;",
        "let mut records: bray.runtime.bootstrap.ReportRecords = bray.runtime.bootstrap.empty_records(); let status = trusted bray.runtime.bootstrap.report_records_admit(1, std.memory.address_of_mut<bray.runtime.bootstrap.ReportRecords>(&mut records));",
        "let records: bray.runtime.bootstrap.ReportRecords = { head = 1, tail = 1, count = 1 }; let result = trusted bray.runtime.bootstrap.pop_record_value(records);",
    ] {
        let source = format!(
            r#"module app;
            using std.memory;
            using internal bray.runtime.bootstrap;
            using internal bray.runtime.bootstrap.pool;
            func production_probe()
{{
    {operation}
}}"#,
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("production report-pool probe must retain its checked result");

        bray_testing::assert_goal_state_diagnostic_kind(
            checked.diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn production_observer_keeps_authority_through_conditional_protocol_changes() {
    for target in [bray_target::NativeTarget::X86_64LinuxGnu, bray_target::NativeTarget::X86_64WindowsMsvc] {
    let compilation = super::fixtures::production_host_compilation_for_target(
        "module probe;", crate::SelectedTarget::for_native(target));

    let symbols = compilation.symbol_graph().expect("production symbols");

    let function = symbols.functions().iter().find(|function| {
        if !symbols.member_name(function.id().into()).is_some_and(|name| name.as_str() == "poll") {
            return false;
        }

        let bray_symbols::SymbolKeyData::SourceDeclaration { owner, .. } = function.key().data() else { return false; };

        matches!(owner.data(), bray_symbols::SymbolKeyData::Module { path, .. }
            if path.segments().eq(["bray", "runtime", "bootstrap", "task", "observer"]))
    }).expect("actual observer poll");

    let key = crate::test_support::source_symbol_body_key(&compilation, symbols, function.id().into());
    let flow = compilation.storage_flow(key).expect("production poll storage flow");

    assert!(!flow.diagnostics().has_errors(), "{target:?}: {:?}", flow.diagnostics());
    }
}
