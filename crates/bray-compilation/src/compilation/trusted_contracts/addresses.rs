use bray_diagnostics::DiagnosticKind;

use crate::test_support::compilation;

#[test]
fn address_retirement_preserves_only_independent_returned_guarantees() {
    for (after, valid) in [
        ("observe_marker(&receipt);", true),
        ("observe_address(&receipt);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Receipt {{ marker: Marker; pointer: DataPointer<u32>; }}
            trusted predicate held(owner: &Marker);
            trusted func produce(pos payload: &u32) -> Receipt
                ensures(
                    trusted held(owner = &result.marker),
                    trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
                )
            {{ return {{ marker = Marker {{}}, pointer = core.memory.address_of<u32>(payload) }}; }}
            func observe_marker(pos receipt: &Receipt)
                requires(trusted held(owner = &receipt.marker)) {{}}
            func observe_address(pos receipt: &Receipt)
                requires(trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1)) {{}}
            func caller() {{
                let receipt = {{
                    let payload: u32 = 0;
                    yield produce(&payload);
                }};
                {after}
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn returned_address_components_retain_their_distinct_storage_owners() {
    for (observed, valid) in [("left", false), ("right", true)] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.memory;
            struct Addresses {{ left: DataPointer<u32>; right: DataPointer<u32>; }}
            trusted func produce(pos left: &u32, pos right: &u32) -> Addresses
                ensures(
                    trusted core.memory.valid_read<u32>(pointer = result.left, count = 1),
                    trusted core.memory.valid_read<u32>(pointer = result.right, count = 1)
                )
            {{ return {{ left = core.memory.address_of<u32>(left), right = core.memory.address_of<u32>(right) }}; }}
            func observe(pos address: DataPointer<u32>)
                requires(trusted core.memory.valid_read<u32>(pointer = address, count = 1)) {{}}
            func caller() {{
                let retained: u32 = 1;
                let addresses = {{
                    let expired: u32 = 0;
                    yield produce(&expired, &retained);
                }};
                observe(addresses.{observed});
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{observed}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn address_guarantees_follow_derived_values_without_revoking_independent_markers() {
    for (body, valid) in [
        (
            "let payload: u32 = 0; let receipt = shifted(core.memory.address_of<u32>(&payload)); observe(receipt.pointer);",
            true,
        ),
        (
            "let receipt = { let payload: u32 = 0; yield shifted(core.memory.address_of<u32>(&payload)); }; observe(receipt.pointer);",
            false,
        ),
        (
            "let receipt = { let payload: u32 = 0; yield shifted(core.memory.address_of<u32>(&payload)); }; observe_marker(&receipt);",
            true,
        ),
        (
            "let receipt = { let payload: u32 = 0; yield shifted(core.memory.address_of<u32>(&payload)); }; let complete = receipt; observe_marker(&complete);",
            true,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Receipt {{ marker: Marker; pointer: DataPointer<u32>; }}
            trusted predicate held(owner: &Marker);
            trusted func shifted(pos pointer: DataPointer<u32>) -> Receipt
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                ensures(
                    trusted held(owner = &result.marker),
                    trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
                )
            {{ return {{ marker = Marker {{}}, pointer = core.memory.offset<u32>(pointer, elements = 0) }}; }}
            func observe(pos pointer: DataPointer<u32>)
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {{}}
            func observe_marker(pos receipt: &Receipt)
                requires(trusted held(owner = &receipt.marker)) {{}}
            func caller() {{ {body} }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{body}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn independent_returned_address_does_not_inherit_unused_input_bounds() {
    let application = compilation(
        r#"
        trusted module app;
        using core.memory;
        static backing: u32 = 1;
        trusted func separate(pos pointer: DataPointer<u32>) -> DataPointer<u32>
            requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
            ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1))
        { return core.memory.address_of<u32>(&backing); }
        func observe(pos pointer: DataPointer<u32>)
            requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {}
        func caller() {
            let pointer = {
                let payload: u32 = 0;
                yield separate(core.memory.address_of<u32>(&payload));
            };
            observe(pointer);
        }
        "#,
    );

    assert!(
        !application.check_diagnostics().has_errors(),
        "{:#?}",
        application.check_diagnostics()
    );
}

#[test]
fn address_bounds_survive_owned_inputs_forwarders_and_runtime_defaults() {
    for (helper, expression, observed) in [
        (
            "trusted func adapt(pos receipt: Receipt) -> DataPointer<u32> requires(trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1)) ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1)) { return core.memory.offset<u32>(receipt.pointer, elements = 0); }",
            "adapt(produce(&payload))",
            "returned",
        ),
        (
            "trusted func adapt(pos pair: (Receipt, bool)) -> DataPointer<u32> requires(trusted core.memory.valid_read<u32>(pointer = pair.0.pointer, count = 1)) ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1)) { return core.memory.offset<u32>(pair.0.pointer, elements = 0); }",
            "adapt((produce(&payload), true))",
            "returned",
        ),
        (
            "func adapt(pos pointer: DataPointer<u32>) -> Receipt requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) { return shifted(pointer); }",
            "adapt(core.memory.address_of<u32>(&payload))",
            "returned.pointer",
        ),
        (
            "func adapt(pos pointer: DataPointer<u32>) -> Receipt requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) ensures(trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)) { return shifted(pointer); }",
            "adapt(core.memory.address_of<u32>(&payload))",
            "returned.pointer",
        ),
        (
            "impl Receipt { trusted consume func derived() -> DataPointer<u32> requires(trusted core.memory.valid_read<u32>(pointer = self.pointer, count = 1)) ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1)) { return core.memory.offset<u32>(self.pointer, elements = 0); } }",
            "produce(&payload).derived()",
            "returned",
        ),
        (
            "func adapt(pos payload: &u32, pos receipt: Receipt = produce(payload)) -> Receipt requires(trusted held(owner = &receipt.marker), trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1)) { return receipt; }",
            "adapt(&payload)",
            "returned.pointer",
        ),
    ] {
        for live in [true, false] {
            let body = if live {
                format!("let payload: u32 = 0; let returned = {expression}; observe({observed});")
            } else {
                format!(
                    "let returned = {{ let payload: u32 = 0; yield {expression}; }}; observe({observed});"
                )
            };

            let application = compilation(&format!(
                r#"
                trusted module app;
                using core.memory;
                struct Marker {{}}
                struct Receipt {{ marker: Marker; pointer: DataPointer<u32>; }}
                trusted predicate held(owner: &Marker);
                trusted func produce(pos payload: &u32) -> Receipt
                    ensures(trusted held(owner = &result.marker), trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1))
                {{ return {{ marker = Marker {{}}, pointer = core.memory.address_of<u32>(payload) }}; }}
                trusted func shifted(pos pointer: DataPointer<u32>) -> Receipt
                    requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                    ensures(trusted held(owner = &result.marker), trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1))
                {{ return {{ marker = Marker {{}}, pointer = core.memory.offset<u32>(pointer, elements = 0) }}; }}
                func observe(pos pointer: DataPointer<u32>) requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {{}}
                {helper}
                func caller() {{ {body} }}
            "#
            ));

            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                live,
                "{helper}\n{body}\n{diagnostics:#?}"
            );

            if !live {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn awaited_address_guarantees_keep_their_original_storage_bounds() {
    for (body, valid) in [
        (
            "let payload: u32 = 0; let receipt = await shifted(core.memory.address_of<u32>(&payload)); observe(receipt.pointer);",
            true,
        ),
        (
            "let receipt = { let payload: u32 = 0; yield await shifted(core.memory.address_of<u32>(&payload)); }; observe(receipt.pointer);",
            false,
        ),
        (
            "let receipt = { let payload: u32 = 0; yield await shifted(core.memory.address_of<u32>(&payload)); }; observe_marker(&receipt);",
            true,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker {{}}
            struct Receipt {{ marker: Marker; pointer: DataPointer<u32>; }}
            trusted predicate held(owner: &Marker);
            trusted async func shifted(pos pointer: DataPointer<u32>) -> Receipt
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                ensures(trusted held(owner = &result.marker), trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1))
            {{ return {{ marker = Marker {{}}, pointer = core.memory.offset<u32>(pointer, elements = 0) }}; }}
            func observe(pos pointer: DataPointer<u32>) requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {{}}
            func observe_marker(pos receipt: &Receipt) requires(trusted held(owner = &receipt.marker)) {{}}
            async func caller() {{ {body} }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{body}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn indirect_owned_inputs_keep_address_bounds_through_callable_values() {
    for (body, valid) in [
        (
            "let payload: u32 = 0; let pointer = trusted derive(produce(&payload)); observe(pointer);",
            true,
        ),
        (
            "let pointer = { let payload: u32 = 0; yield trusted derive(produce(&payload)); }; observe(pointer);",
            false,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.memory;
            struct Receipt {{ pointer: DataPointer<u32>; }}
            trusted func produce(pos payload: &u32) -> Receipt
                ensures(trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1))
            {{ return {{ pointer = core.memory.address_of<u32>(payload) }}; }}
            func observe(pos pointer: DataPointer<u32>) requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {{}}
            func caller() {{
                let derive = trusted lambda(pos receipt: Receipt) -> DataPointer<u32>
                    requires(trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1))
                    ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1))
                {{ return core.memory.offset<u32>(receipt.pointer, elements = 0); }};
                {body}
            }}
        "#
        ));
        let diagnostics = application.check_diagnostics();
        assert_eq!(!diagnostics.has_errors(), valid, "{body}: {diagnostics:#?}");
        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
