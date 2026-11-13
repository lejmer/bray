use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
fn async_ownership_composition_checks_real_source_transitions() {
    let source = include_str!("../../../../../../xtask/fixtures/native-execution/async-ownership-composition.bray");

    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), source);

    let source_id = source_function_body_key(&compilation, "main").source().syntax().source_id();
    let keys = compilation.declared_unit_keys_for_test().expect("composition units are declared");

    for key in keys.iter().filter(|key| key.kind() == bray_bound_tree::BoundUnitKind::CallableBody
        && key.source().syntax().source_id() == source_id) {
          let properties = compilation.execution_properties(key.clone())
              .expect("every composition function and lifecycle body is certified");

          assert!(!properties.diagnostics().has_errors(), "{key:?}: {:?}", properties.diagnostics());

        let checked = compilation.lowered_unit(key.clone())
            .expect("every composition function and lifecycle body lowers");

        assert!(!checked.diagnostics().has_errors(), "{key:?}: {:?}", checked.diagnostics());
    }
}

#[test]
fn async_ownership_composition_rejects_fabricated_and_moved_owners() {
    let source = include_str!("../../../../../../xtask/fixtures/native-execution/async-ownership-composition.bray");

    for (original, replacement, body, expected) in [
        ("acquire(false)", r#"ChildCleanup
        {
            marker = Marker
            {
            },
            cancelled = false,
            visits = 0
        }"#, "produce", DiagnosticKind::CheckingTrustedObligationNotProven),
        ("let pending = child(owner).start();", r#"let pending = child(owner).start();
    observe(&owner);"#, "main", DiagnosticKind::CheckingUseOfMovedStorage),
        ("await owner.advance();", r#"let pending = owner.advance();
        observe(&owner);
        await pending;"#, "transport", DiagnosticKind::CheckingConflictingBorrow),
    ] {
        assert_eq!(source.matches(original).count(), 1);

        let invalid = source.replace(original, replacement);

        let compilation = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::baseline(), &invalid);

        let lowered = compilation.lowered_unit(source_function_body_key(&compilation, body))
            .expect("invalid composition reports source diagnostics");

        bray_testing::assert_goal_state_diagnostic_kind(lowered.diagnostics(), expected);
    }
}

#[test]
#[ignore = "tier:proof"]
fn composition_requires_the_exact_parent_token_continuation_and_origin() {
    for (parent, token, state, origin, destination, abandoning, valid) in [
        ("parent", "token", "state", "origin", "destination", 0, true),
        ("other", "token", "state", "origin", "destination", 0, false),
        (
            "parent",
            "other_token",
            "state",
            "origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "other_state",
            "origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "state",
            "other_origin",
            "destination",
            0,
            false,
        ),
        (
            "parent",
            "token",
            "state",
            "origin",
            "other_destination",
            0,
            false,
        ),
        ("parent", "token", "state", "origin", "destination", 1, true),
        (
            "parent",
            "other_token",
            "state",
            "origin",
            "destination",
            1,
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.entry;
            using internal bray.runtime.bootstrap.pool;
            using core.memory;
            trusted func production_probe(
                pos parent: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
                pos token: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other_token: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos state: u32, pos other_state: u32, pos origin: u64, pos other_origin: u64,
                pos destination: usize, pos other_destination: usize,
                pos ready: FunctionPointer<bray.runtime.bootstrap.activation.callback.Ready>,
            ) -> usize
                requires(
                    trusted bray.runtime.bootstrap.activation.execution.owned(pointer = parent),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.entry.native_entry(),
                    trusted core.memory.callable_address_valid<bray.runtime.bootstrap.activation.callback.Ready>(pointer = ready),
                    trusted bray.runtime.bootstrap.activation.composition.authorized(
                        parent = {parent}, token = {token}, state = {state}, origin = {origin}, destination = {destination}, disposition = {abandoning}
                    ),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.composition.compose(parent, token, state, origin, destination, {abandoning}, ready);
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
