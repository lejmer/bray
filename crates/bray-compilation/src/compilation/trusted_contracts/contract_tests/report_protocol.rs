use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key, source_module_function_body_key};
use super::fixtures::{assert_production_probes, assert_production_contract, diagnostic_at_call, production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn native_report_addresses_retain_authority_only_for_the_unchanged_owner() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        include_str!("fixtures/primary-callback-ownership.bray"),
        &[
            ("probe_primary_callbacks_admitted", None),
            ("probe_primary_callbacks_independent_mutation", None),
            ("probe_primary_callbacks_mutated", missing),
            ("probe_primary_callbacks_copied", missing),
        ],
        &[],
    );
}

#[test]
#[ignore = "tier:proof"]
fn raw_typed_incident_headers_do_not_grant_disposal_authority() {
    assert_production_contract(
        r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.pool;
        using internal bray.runtime.bootstrap.report.incident;
        trusted func production_probe(pos pointer: DataPointer<bray.runtime.bootstrap.report.incident.Record>) -> u32
            requires(trusted bray.runtime.bootstrap.pool.protocol_access())
        {
            return bray.runtime.bootstrap.report.incident.dispose(pointer, 1);
        }
    "#,
        Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        "bray.runtime.bootstrap.report.incident.dispose(pointer, 1)",
        "owned",
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
                return bray.runtime.bootstrap.consume_values(
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
    for (destination, expected) in [
        ("other", None),
        ("outcome", Some(DiagnosticKind::CheckingConflictingBorrow)),
    ] {
        let caller = format!(r#"
        trusted internal module bray.runtime.bootstrap;

        using internal bray.runtime.bootstrap.report.incident;
        func production_probe(pos outcome: &mut ReportOutcome, pos other: &mut ReportOutcome)
        {{
            let pair = bray.runtime.bootstrap.report.incident.destinations(outcome, {destination});
        }}
    "#);

        let compilation = production_host_compilation(&caller);
        let key = source_function_body_key(&compilation, "production_probe");
        let source = key.source().syntax().source_id();
        let checked = compilation.lowered_unit(key).expect("fixture callable must lower successfully");

        bray_testing::assert_goal_state_diagnostic_outcome(checked.diagnostics(), expected, &caller);

        if let Some(expected) = expected {
            assert!(checked.diagnostics().iter().any(|diagnostic| {
                diagnostic.kind() == expected && diagnostic_at_call(
                    diagnostic, source, &caller, "bray.runtime.bootstrap.report.incident.destinations",
                )
            }));
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn source_report_chain_requires_unchanged_exclusive_ownership() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        include_str!("fixtures/report-chain-ownership.bray"),
        &[
            ("probe_report_chain_admitted", None),
            ("probe_report_chain_fabricated", missing),
            ("probe_report_chain_copied", missing),
            ("probe_report_chain_mutated", missing),
            ("probe_report_chain_replaced", missing),
        ],
        &[],
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_report_pool_mutation_requires_protocol_authority() {
    for (operation, site) in [
        ("let mut guard = bray.runtime.bootstrap.pool.lock(); let pool = guard.get(); pool.credits = 0; pool.free = 1;", "bray.runtime.bootstrap.pool.lock()"),
        ("let mut records: bray.runtime.bootstrap.ReportRecords = bray.runtime.bootstrap.empty_records(); let status = bray.runtime.bootstrap.report_records_admit(1, std.memory.address_of_mut<bray.runtime.bootstrap.ReportRecords>(&mut records));", "bray.runtime.bootstrap.report_records_admit("),
        ("let records: bray.runtime.bootstrap.ReportRecords = { head = 1, tail = 1, count = 1 }; let result = bray.runtime.bootstrap.pop_record_value(records);", "bray.runtime.bootstrap.pop_record_value(records)"),
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

        assert_production_contract(
            &source,
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
            site,
            "protocol_access",
        );
    }
}

#[test]
fn production_observer_keeps_authority_through_conditional_protocol_changes() {
    for target in [bray_target::NativeTarget::X86_64LinuxGnu, bray_target::NativeTarget::X86_64WindowsMsvc] {
        let compilation = super::fixtures::production_host_compilation_for_target(
            "module probe;", crate::SelectedTarget::for_native(target));

        for name in ["poll", "destroy"] {
            let key = source_module_function_body_key(&compilation,
                &["bray", "runtime", "bootstrap", "task", "observer"], name);

            let flow = compilation.storage_flow(key).expect("production observer storage flow");

            assert!(!flow.diagnostics().has_errors(), "{target:?}: {:?}", flow.diagnostics());
        }

        let certification = compilation.execution_properties(
            source_function_body_key(&compilation, "continue_observation"),
        ).expect("observer callback certifies its transitive postconditions");

        assert!(!certification.diagnostics().has_errors(), "{target:?}: {:?}", certification.diagnostics());
    }
}
