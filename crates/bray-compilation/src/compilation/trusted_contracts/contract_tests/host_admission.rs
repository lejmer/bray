use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{assert_production_probes, production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn execution_domains_use_live_witnesses_without_accepting_fabricated_or_stale_ones() {
    for (body, valid, missing_witness) in [
        (
            "let owner = produce(1); return observe(&owner);",
            true,
            false,
        ),
        (
            "let owner = produce(2); return observe(&owner);",
            false,
            false,
        ),
        (
            "let owner: Owner = { value = 1 }; return observe(&owner);",
            false,
            true,
        ),
        (
            "let mut owner = produce(1); change(&mut owner); return observe(&owner);",
            false,
            true,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                value: u64;
                mut epoch: u64 = 0;
            }}
            trusted predicate live(owner: &Owner);
            trusted func produce(pos value: u64) -> Owner
                ensures(result.value == value, trusted live(owner = &result)) executes(total)
            {{
                return
                {{
                    value = value
                }};
            }}
            trusted func observe(pos owner: &Owner) -> u64
                requires(trusted live(owner = owner), owner.value == 1) executes(total)
            {{
                return owner.value;
            }}
            func change(pos owner: &mut Owner)
                executes(total)
            {{
                owner.epoch = 2;
            }}
            func production_probe() -> u64
                executes(total)
            {{
                {body}
            }}
        "#
        );

        let compilation = crate::test_support::compilation(&source);

        let key = source_function_body_key(&compilation, "production_probe");
        let source_id = key.source().syntax().source_id();

        let checked = compilation
            .execution_properties(key)
            .expect("live execution domain evidence must be checked");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {checked:#?}"
        );

        assert_eq!(
            checked.diagnostics().iter().any(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingTrustedObligationNotProven
                    && super::fixtures::diagnostic_at_call(
                        diagnostic, source_id, &source, "observe(&owner)",
                    )
            }),
            missing_witness,
            "{body}: {checked:#?}",
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn completion_witnesses_require_proven_preservation_across_calls() {
    for (body, valid) in [
        ("", true),
        ("mutate(&mut completed);", false),
        ("observe_completion(&completed);", true),
        ("publish_completion(&completed);", true),
        ("let incidents = completed.incidents;", true),
        ("let value = observe();", true),
        ("let value = acquire();", true),
        (
            "let incidents = completed.incidents; let value = acquire();",
            true,
        ),
    ] {
        let source = r#"
        trusted module app;
        struct Owner
        {
            value: u64;
        }
        struct Completed
        {
            selected: Owner;
            incidents: u32;
        }
        trusted predicate live(owner: &Owner);
        trusted predicate done(owner: &Completed);
        extern trusted func owner() -> Owner
            ensures(trusted live(owner = &result));
        extern func acquire() -> u64;
        extern func mutate(pos value: &mut Completed);
        func observe_completion(pos value: &Completed)
        {
            _ = acquire();
            _ = value.incidents;
        }
        extern func publish_completion(pos value: &Completed);
        func observe() -> u64
            executes(pure, total)
        {
            return 0;
        }
        trusted func complete(pos selected: Owner) -> Completed
            requires(trusted live(owner = &selected))
            ensures(trusted done(owner = &result))
        {
            return
            {
                selected = selected, incidents = 0
            };
        }
        extern trusted func consume_completion(pos completed: Completed)
            requires(trusted done(owner = &completed));
        func caller()
        {
            let selected = owner();
            let mut completed = trusted complete(selected);
            BODY
            consume_completion(completed);
        }
    "#.replace("BODY", body);

        let compilation = crate::test_support::compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn closing_epoch_authority_cannot_be_copied_from_internal_fields() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        &[
            ("probe_closing_epoch_selected", None),
            ("probe_closing_epoch_moved", None),
            ("probe_closing_epoch_mutated", missing),
            ("probe_closing_epoch_fabricated_exit", missing),
            ("probe_closing_epoch_fabricated_retirement", missing),
        ],
        &[],super::fixtures::check_probe_storage,
    );
}

#[test]
#[ignore = "tier:proof"]
fn driver_view_keeps_its_source_acquisition_with_the_complete_owner() {
    for (body, valid) in [
        ("return owner;", true),
        (
            "return { acquisition = owner.acquisition, pointer = pointer, status = 0 };",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.execution;

            trusted func production_probe(
                pos owner: bray.runtime.bootstrap.execution.Current,
                pos pointer: DataPointer<bray.runtime.bootstrap.execution.Context>,
            ) -> bray.runtime.bootstrap.execution.Current
                requires(trusted bray.runtime.bootstrap.execution.view_owned(owner = &owner.acquisition))
                ensures(trusted bray.runtime.bootstrap.execution.view_owned(owner = &result.acquisition))
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
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_storage_lease_requires_admission_evidence() {
    for (admission, body, valid) in [
        (true, "let status = retire(pointer);", true),
        (false, "let status = retire(pointer);", false),
        (
            false,
            "let address = activate(pointer, description);",
            false,
        ),
        (
            false,
            "trusted claim(description, pointer); let status = retire(pointer);",
            false,
        ),
        (
            true,
            "lease = empty_lease(); let status = retire(pointer);",
            false,
        ),
        (
            true,
            "let address = activate(pointer, description); std.memory.write<u8>(address, 1);",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.storage;

            using internal bray.runtime.bootstrap.pool;
            using std.memory;
            func probe()
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
            {{
                let descriptor: Descriptor = {{ identity = [0; 32], bytes = 64, align = 32 }};
                let mut lease: Lease = empty_lease();
                let pointer = std.memory.address_of_mut<Lease>(&mut lease);
                let description = std.memory.address_of<Descriptor>(&descriptor);
                {admission}
                {{
                    {body}
                }}
            }}
            "#,
            admission = if admission {
                "if trusted admit(description, pointer) == 0"
            } else {
                ""
            },
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "probe",
            ))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_host_lease_retains_authority_across_fallible_preparation() {
    let compilation = production_host_compilation(
        r#"
        trusted internal module bray.runtime.bootstrap.host;
        func production_probe() -> u32
        {
            let lease = trusted host_access();
            let mut anchor = trusted host_anchor(&lease);

            if !lease.active
            {
                return 4;
            }

            let status = trusted initialize_host();
            let host = trusted host_value(&lease, &mut anchor);

            return status;
        }
        "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "production_probe"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "a private acquisition survives unrelated preparation: {checked:#?}"
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_host_authority_cannot_be_constructed_from_fields() {
    let cases = [
        ("let host = host_value(&lease, &mut anchor);", None),
        (
            "let moved = lease; let host = host_value(&moved, &mut anchor);",
            None,
        ),
        (
            "let fake: HostLease = { acquisition = lease.acquisition, synchronization = lease.synchronization, owner = lease.owner, active = true }; let host = host_value(&fake, &mut anchor);",
            Some(DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let fake: HostLease = { acquisition = lease.acquisition, synchronization = lease.synchronization, owner = lease.owner, active = lease.active };",
            Some(DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let mut fake: HostAnchor = { owner = anchor.owner }; let value = host_value(&lease, &mut fake);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "let other_lease = trusted host_access(); let mut other = trusted host_anchor(&other_lease); let value = host_value(&lease, &mut other);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "let host = host_value(&lease, &mut anchor); host.next_identity = 0;",
            None,
        ),
        (
            "let host = host_value(&lease, &mut anchor); host.transitioning = true;",
            None,
        ),
        (
            "let host = host_value(&lease, &mut anchor); host.storage = none;",
            None,
        ),
        (
            "let host = host_value(&lease, &mut anchor); if let ?storage = host.storage { storage.released = true; }",
            None,
        ),
    ];

    let callers = cases
        .iter()
        .enumerate()
        .map(|(index, (body, _))| {
            format!(
                r#"
        func production_probe_{index}()
        {{
            let lease = trusted host_access();
            let mut anchor = trusted host_anchor(&lease);
            if !lease.active
            {{
                return;
            }}
            {body}
        }}
        "#,
            )
        })
        .collect::<String>();

    let compilation = production_host_compilation(&format!(
        "trusted internal module bray.runtime.bootstrap.host; {callers}"
    ));

    for (index, (body, expected)) in cases.into_iter().enumerate() {
        let checked = compilation
            .lowered_unit(source_function_body_key(
                &compilation,
                &format!("production_probe_{index}"),
            ))
            .expect("production host probe must retain its checked result");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            expected.is_none(),
            "{body}: {checked:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), expected);
        }
    }
}
