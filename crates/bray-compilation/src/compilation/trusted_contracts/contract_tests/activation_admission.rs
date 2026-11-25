use bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven;

#[test]
#[ignore = "tier:proof"]
fn activation_cursor_fields_do_not_create_or_repair_selection_authority() {
    let cases = [
        (
            "_ = bray.runtime.bootstrap.activation.execution.read(original);",
            true,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = other }; _ = bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = original.root, pointer = original.pointer }; _ = bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
        (
            "let mut cursor: bray.runtime.bootstrap.activation.run.Cursor = { root = other, pointer = original.pointer }; _ = bray.runtime.bootstrap.activation.execution.read(&mut cursor);",
            false,
        ),
    ].into_iter().enumerate().map(|(case, (body, valid))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using std.memory;
            trusted func production_probe_{case}(
                pos original: &mut bray.runtime.bootstrap.activation.run.Cursor,
                pos other: DataPointer<bray.runtime.bootstrap.activation.Header>,
            )
                requires(trusted bray.runtime.bootstrap.activation.run.selected(cursor = &original))
            {{
                {body}
            }}
            "#
        );

        (
            caller,
            (!valid).then_some(CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.execution.read(&mut cursor)",
            "selected",
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn activation_tokens_require_both_producer_and_entry_authority() {
    let cases = [
        ("", Some("producer_owned")),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_owned(owner = &token.acquisition))",
            Some("entry_authorized"),
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.entry_authorized(owner = &token.acquisition, origin = origin))",
            Some("producer_owned"),
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_owned(owner = &token.acquisition), trusted bray.runtime.bootstrap.activation.entry_authorized(owner = &token.acquisition, origin = origin))",
            None,
        ),
    ].into_iter().enumerate().map(|(case, (requirements, missing))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe_{case}(
                pos token: bray.runtime.bootstrap.activation.Producer,
                pos origin: u64,
            ) -> usize
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return bray.runtime.bootstrap.activation.begin(token, origin);
            }}
        "#
        );

        (
            caller,
            missing.map(|_| bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.begin(token, origin)",
            missing.unwrap_or_default(),
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn activation_publication_requires_the_exact_producer_witness() {
    let cases = [
        ("", false),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = token))",
            true,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer(token = other))",
            false,
        ),
    ].into_iter().enumerate().map(|(case, (requirements, valid))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe_{case}(
                pos token: &bray.runtime.bootstrap.activation.Inactive,
                pos other: &bray.runtime.bootstrap.activation.Inactive,
            ) -> u32
                requires(trusted bray.runtime.bootstrap.pool.protocol_access())
                {requirements}
            {{
                return bray.runtime.bootstrap.activation.publish(token);
            }}
            "#,
        );

        (
            caller,
            (!valid).then_some(CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.publish(token)",
            "producer",
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn native_activation_requires_producer_and_exact_entry_authority() {
    let cases = [
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer))",
            Some("entry_slot_authorized"),
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            Some("producer_slot"),
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = origin))",
            None,
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = other, origin = origin))",
            Some("entry_slot_authorized"),
        ),
        (
            "requires(trusted bray.runtime.bootstrap.activation.producer_slot(pointer = pointer), trusted bray.runtime.bootstrap.activation.entry_slot_authorized(pointer = pointer, origin = 0))",
            Some("entry_slot_authorized"),
        ),
    ].into_iter().enumerate().map(|(case, (requirements, missing))| {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            using core.memory;
            trusted func production_probe_{case}(
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
                return bray.runtime.bootstrap.activation.activate(pointer, origin);
            }}
        "#
        );

        (
            caller,
            missing.map(|_| CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.activate(pointer, origin)",
            missing.unwrap_or_default(),
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn inactive_inspection_requires_retained_exact_owner_and_output() {
    let cases = [
        (false, "pointer", "output", Some("producer_slot")),
        (true, "other", "output", Some("retained")),
        (true, "pointer", "other_output", Some("retained")),
        (true, "pointer", "output", None),
    ].into_iter().enumerate().map(|(case, (producer, owner, output, missing))| {
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
            trusted func production_probe_{case}(
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
                return bray.runtime.bootstrap.activation.requirements.inspect(pointer, 0, 0, 0, output);
            }}
            "#
        );

        (
            caller,
            missing.map(|_| CheckingTrustedObligationNotProven),
            "bray.runtime.bootstrap.activation.requirements.inspect(pointer, 0, 0, 0, output)",
            missing.unwrap_or_default(),
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn metadata_inspection_cannot_infer_exclusive_ownership_from_a_producer() {
    let cases = [("", false), ("token", true), ("other", false)].into_iter().enumerate().map(|(case, (owner, valid))| {
        let ownership = if owner.is_empty() {
            String::new()
        } else {
            format!(
                "requires(trusted bray.runtime.bootstrap.activation.requirements.inactive_owner(token = &{owner}))"
            )
        };

        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap;

            using internal bray.runtime.bootstrap.activation;
            using internal bray.runtime.bootstrap.pool;
            trusted func production_probe_{case}(
                pos token: bray.runtime.bootstrap.activation.Inactive,
                pos other: bray.runtime.bootstrap.activation.Inactive,
            ) -> bray.runtime.bootstrap.activation.requirements.Execution?
                requires(
                    trusted bray.runtime.bootstrap.activation.producer(token = &token),
                    trusted bray.runtime.bootstrap.pool.protocol_access(),
                )
                {ownership}
            {{
                let producer = token.into_producer();

                return trusted bray.runtime.bootstrap.activation.requirements.read(&producer, 0, 0, 0, 0);
            }}
            "#
        );

        (caller, (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven), "token.into_producer()", "inactive_owner")
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn inactive_cursor_requires_the_exact_native_token_and_outcome_pair() {
    let cases = [
        ("", false),
        ("trusted entry(token = pointer, outcome = outcome),", true),
        ("trusted entry(token = other, outcome = outcome),", false),
        (
            "trusted entry(token = pointer, outcome = other_outcome),",
            false,
        ),
    ].into_iter().enumerate().map(|(case, (authority, valid))| {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.activation.inactive;

            using internal bray.runtime.bootstrap;
            using core.memory;
            using std.memory;

            func production_probe_{case}(
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
                let cursor = acquire(pointer, outcome);
            }}
            "#,
        );

        (
            source,
            (!valid).then_some(CheckingTrustedObligationNotProven),
            "acquire(pointer, outcome)",
            "entry",
        )
    });

    super::fixtures::assert_production_contracts(cases);
}

#[test]
#[ignore = "tier:proof"]
fn native_frame_acquisition_consumes_the_claim_even_if_address_bits_are_restored() {
    let cases = [
        ("return root_frame(frame);", true),
        (
            r#"
                let first = root_frame(frame);

                return root_frame(frame);
            "#,
            false,
        ),
        (
            r#"
                let address = trusted std.memory.read<usize>(std.memory.address_of_mut<usize>(frame));
                let first = root_frame(frame);

                _ = trusted std.memory.replace_value<usize>(frame, address);

                return root_frame(frame);
            "#,
            false,
        ),
    ].into_iter().enumerate().map(|(case, (body, valid))| {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.execution;

            using internal bray.runtime.bootstrap.activation;
            using std.memory;

            trusted func production_probe_{case}(pos frame: &mut usize) -> bray.runtime.bootstrap.activation.Producer
                requires(trusted frame_input(owner = &frame))
                uses(raw_memory)
            {{
                {body}
            }}
            "#
        );

        (
            source,
            (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
            "return root_frame(frame)",
            "frame_input",
        )
    });

    super::fixtures::assert_production_contracts(cases);
}
