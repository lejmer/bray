use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn activation_cursor_fields_do_not_create_or_repair_selection_authority() {
    for (body, valid) in [
        (
            "_ = trusted bray.runtime.bootstrap.activation.execution.read(original);",
            true,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = other }; _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = original.pointer }; _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = other, pointer = original.pointer }; _ = trusted bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
    ] {
        let compilation = production_host_compilation(&format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using std.memory;
            trusted func production_probe(
                pos original: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
            )
                requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &original))
            {{
                {body}
            }}
            "#
        ));

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
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn activation_tokens_require_both_producer_and_entry_authority() {
    for (requirements, valid) in [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token), trusted bray.runtime.bootstrap.activation.entry_authorized(token = token, origin = origin))",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos origin: u64,
            ) -> usize
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return trusted bray.runtime.bootstrap.activation.begin(token, origin);
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
            "{caller}: {lowered:#?}"
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
fn activation_publication_requires_the_exact_producer_witness() {
    for (requirements, expression, valid) in [
        ("", "token", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token))",
            "token",
            true,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = other))",
            "token",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos other: &bray.runtime.bootstrap.activation.Inactive,
            ) -> u32
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return trusted bray.runtime.bootstrap.activation.publish({expression});
            }}
            "#,
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{caller}: {lowered:#?}"
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
fn native_activation_requires_producer_and_exact_entry_authority() {
    for (requirements, valid) in [
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            true,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = other, origin = origin))",
            false,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = 0))",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            using core.memory;
            trusted func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos origin: u64,
            ) -> usize
                requires(
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
                {requirements}
            {{
                return trusted bray.runtime.bootstrap.activation.activate(pointer, origin);
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
fn inactive_inspection_requires_retained_exact_owner_and_output() {
    for (producer, owner, output, valid) in [
        (false, "pointer", "output", false),
        (true, "other", "output", false),
        (true, "pointer", "other_output", false),
        (true, "pointer", "output", true),
    ] {
        let producer = if producer {
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer))"
        } else {
            ""
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            using core.memory;
            trusted func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos output: DataPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
                pos other_output: DataPointer<bray.runtime.bootstrap.activation.requirements.Execution>,
            ) -> u32
                {producer}
                requires(
                    trusted bray.runtime.bootstrap.activation.requirements.retained(token = {owner}, output = {output}),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.inspect(pointer, 0, 0, 0, output);
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
fn metadata_inspection_cannot_infer_exclusive_ownership_from_a_producer() {
    for (owner, valid) in [("", false), ("token", true), ("other", false)] {
        let ownership = if owner.is_empty() {
            String::new()
        } else {
            format!(
                "requires(trusted bray.runtime.bootstrap.activation.requirements.inactive_owner(token = {owner}))"
            )
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos other: &bray.runtime.bootstrap.activation.Inactive,
            ) -> bray.runtime.bootstrap.activation.requirements.State?
                requires(
                    trusted bray.runtime.bootstrap.activation.producer(token = token),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                )
                {ownership}
            {{
                return trusted bray.runtime.bootstrap.activation.requirements.read(token, 0, 0, 0);
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
fn inactive_cursor_requires_the_exact_native_token_and_outcome_pair() {
    for (authority, valid) in [
        ("", false),
        ("trusted entry(token = pointer, outcome = outcome),", true),
        ("trusted entry(token = other, outcome = outcome),", false),
        (
            "trusted entry(token = pointer, outcome = other_outcome),",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.activation.inactive;

            using internal bray.runtime.bootstrap;
            using core.memory;
            using std.memory;

            func production_probe(
                pos pointer: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Inactive>,
                pos outcome: DataPointer<bray.runtime.bootstrap.ReportOutcome>,
                pos other_outcome: DataPointer<bray.runtime.bootstrap.ReportOutcome>,
            )
                requires(
                    {authority}
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                    trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer),
                    trusted core.memory.valid_read<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                    trusted core.memory.initialized_as<bray.runtime.bootstrap.activation.Inactive>(pointer = pointer),
                )
            {{
                let cursor = trusted acquire(pointer, outcome);
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
