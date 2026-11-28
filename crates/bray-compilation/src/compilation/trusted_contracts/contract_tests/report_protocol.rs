use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key, source_module_function_body_key};
use super::fixtures::{assert_production_probes, assert_production_contract};

#[test]
#[ignore = "tier:proof"]
fn native_report_addresses_retain_authority_only_for_the_unchanged_owner() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        &[
            ("probe_primary_callbacks_admitted", None),
            ("probe_primary_callbacks_independent_mutation", None),
            ("probe_primary_callbacks_mutated", missing),
            ("probe_primary_callbacks_copied", missing),
        ],
        &[],super::fixtures::check_probe_storage,
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
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(&[
        ("probe_retained_incident", None),
        ("probe_forged_incident", missing),
        ("probe_repacked_incident", missing),
    ], &[
        ("probe_forged_incident", "bray.runtime.bootstrap.consume_values(forged_value,"),
        ("probe_repacked_incident", "bray.runtime.bootstrap.consume_values(repacked_value,"),
    ], super::fixtures::check_probe_storage);
}

#[test]
#[ignore = "tier:proof"]
fn incident_outcome_destinations_require_distinct_exclusive_borrows() {
    assert_production_probes(&[
        ("probe_distinct_incident_destinations", None),
        ("probe_aliased_incident_destinations", Some(DiagnosticKind::CheckingConflictingBorrow)),
    ], &[
        ("probe_aliased_incident_destinations", "bray.runtime.bootstrap.report.incident.destinations"),
    ], super::fixtures::check_probe_storage);
}

#[test]
#[ignore = "tier:proof"]
fn source_report_chain_requires_unchanged_exclusive_ownership() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        &[
            ("probe_report_chain_admitted", None),
            ("probe_report_chain_fabricated", missing),
            ("probe_report_chain_copied", missing),
            ("probe_report_chain_mutated", missing),
            ("probe_report_chain_replaced", missing),
        ],
        &[],super::fixtures::check_probe_storage,
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
    let check = |target: bray_target::NativeTarget| {
        let compilation = super::fixtures::production_host_compilation_for_target(
            "module app;", crate::SelectedTarget::for_native(target));

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
    };

    // Each target's check is one long serial chain, so the targets run side by side.
    std::thread::scope(|scope| {
        let targets = [bray_target::NativeTarget::X86_64LinuxGnu, bray_target::NativeTarget::X86_64WindowsMsvc]
            .map(|target| scope.spawn(move || check(target)));

        for target in targets {
            target.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        }
    });
}
