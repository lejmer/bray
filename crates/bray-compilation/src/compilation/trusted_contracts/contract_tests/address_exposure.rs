use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn publishing_a_static_address_only_exposes_that_static() {
    for directive in ["", "@thread_local"] {
        for address in ["&TOKEN", "&owner.marker"] {
            for observed in ["TOKEN", "OTHER"] {
                let source = format!(
                    r#"
                    trusted module app;
                    using core.memory;
                    using core.target;
                    struct Marker
                    {{
                    }}
                    struct Owner
                    {{
                        marker: Marker;
                    }}
                    trusted predicate live(marker: &Marker);
                    trusted const func make_marker() -> Marker
                        ensures(trusted live(&result))
                    {{
                        return Marker
                        {{
                        }};
                    }}
                    trusted func create() -> Owner
                        ensures(trusted live(&result.marker))
                    {{
                        return
                        {{
                            marker = Marker {{}}
                        }};
                    }}
                    {directive} static TOKEN: Marker = make_marker();
                    {directive} static OTHER: Marker = make_marker();
                    trusted func publish(pos owner: &Owner) -> usize
                        uses(intrinsic)
                    {{
                        return trusted core.target.expose_address<Marker>(
                            core.memory.address_of<Marker>({address})
                        );
                    }}
                    trusted func relay(pos owner: &Owner) -> usize
                    {{
                        return trusted publish(owner);
                    }}
                    extern func opaque();
                    trusted func observe(pos marker: &Marker)
                        requires(trusted live(marker)) executes(pure, total) {{}}
                    func caller()
                    {{
                        trusted observe(&TOKEN);
                        trusted observe(&OTHER);
                        let owner = trusted create();
                        _ = trusted relay(&owner);
                        opaque();
                        trusted observe(&{observed});
                    }}
                    "#,
                );

                let application = compilation(&source);
                let diagnostics = application.check_diagnostics();
                let valid = address != "&TOKEN" || observed != "TOKEN";

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
    }
}

#[test]
fn publishing_global_addresses_keeps_unpublished_input_ownership() {
    for directive in ["", "@thread_local"] {
        for (address, valid) in [("&TOKEN", true), ("&owner.marker", false)] {
            let source = format!(
                r#"
                trusted module app;
                using core.memory;
                using core.target;
                struct Marker
                {{
                }}
                struct Owner
                {{
                    marker: Marker;
                }}
                trusted predicate live(marker: &Marker);
                trusted func create() -> Owner
                    ensures(trusted live(&result.marker))
                {{
                    return
                    {{
                        marker = Marker {{}}
                    }};
                }}
                {directive} static TOKEN: Marker = Marker {{}};
                trusted func publish(pos owner: &Owner) -> usize
                    uses(intrinsic)
                {{
                    return trusted core.target.expose_address<Marker>(core.memory.address_of<Marker>({address}));
                }}
                extern func opaque();
                extern trusted func observe(pos owner: Owner)
                    requires(trusted live(&owner.marker));
                func caller()
                {{
                    let owner = trusted create();
                    _ = trusted publish(&owner);
                    opaque();
                    trusted observe(owner);
                }}
                "#,
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
}

#[test]
fn storing_address_bits_does_not_bind_destination_initialization_to_the_pointee() {
    for (after, valid) in [
        ("_ = bits;", true),
        ("_ = trusted core.memory.read<u32>(bits);", false),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            using core.memory;
            trusted func caller()
                uses(raw_memory, unchecked_init)
            {{
                let mut storage: DataPointer<u32> = core.memory.null<u32>();
                let slot = core.memory.address_of_mut<DataPointer<u32>>(&mut storage);
                {{
                    let payload: u32 = 7;
                    let pointer = core.memory.address_of<u32>(&payload);
                    trusted core.memory.write<DataPointer<u32>>(slot, pointer);
                }}
                let bits = trusted core.memory.read<DataPointer<u32>>(slot);
                {after}
            }}
            "#
        ));

        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{after}: {diagnostics:#?}"
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
fn address_retirement_preserves_only_independent_returned_guarantees() {
    let receipt_declarations = super::fixtures::RECEIPT_DECLARATIONS;

    for (after, valid) in [
        ("observe_marker(&receipt);", true),
        ("observe_address(&receipt);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            {receipt_declarations}
            trusted func produce(pos payload: &u32) -> Receipt
                ensures(
                    trusted held(owner = &result.marker),
                    trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
                )
            {{
                return
                {{
                    marker = Marker {{}}, pointer = core.memory.address_of<u32>(payload)
                }};
            }}
            func observe_marker(pos receipt: &Receipt)
                requires(trusted held(owner = &receipt.marker)) {{}}
            func observe_address(pos receipt: &Receipt)
                requires(trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1)) {{}}
            func caller()
            {{
                let receipt =
                {{
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
fn address_guarantees_follow_derived_values_without_revoking_independent_markers() {
    let receipt_declarations = super::fixtures::RECEIPT_DECLARATIONS;

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
            {receipt_declarations}
            trusted func shifted(pos pointer: DataPointer<u32>) -> Receipt
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                ensures(
                    trusted held(owner = &result.marker),
                    trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
                )
            {{
                return
                {{
                    marker = Marker {{}}, pointer = core.memory.offset<u32>(pointer, elements = 0)
                }};
            }}
            func observe(pos pointer: DataPointer<u32>)
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1)) {{}}
            func observe_marker(pos receipt: &Receipt)
                requires(trusted held(owner = &receipt.marker)) {{}}
            func caller()
            {{
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

#[test]
fn input_postconditions_expire_only_their_address_access() {
    for (setter, call) in [
        (
            r#"
                trusted func set(pos receipt: &mut Receipt, pos payload: &u32)
                    requires(trusted held(owner = &receipt.marker))
                    ensures(trusted held(owner = &receipt.marker), trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1))
                {
                    receipt.pointer = core.memory.address_of<u32>(payload);
                }
            "#,
            "set(&mut receipt, &payload)",
        ),
        (
            r#"
                impl Receipt
                {
                    trusted mut func set(pos payload: &u32)
                        requires(trusted held(owner = &self.marker))
                        ensures(trusted held(owner = &self.marker), trusted core.memory.valid_read<u32>(pointer = self.pointer, count = 1))
                    {
                        self.pointer = core.memory.address_of<u32>(payload);
                    }
                }
            "#,
            "receipt.set(&payload)",
        ),
        (
            r#"
                trusted func set(pos receipt: &mut Receipt, pos payload: &u32) -> Marker
                    requires(trusted held(owner = &receipt.marker))
                    ensures(trusted held(owner = &result) && trusted held(owner = &receipt.marker) && trusted core.memory.valid_read<u32>(pointer = receipt.pointer, count = 1))
                {
                    receipt.pointer = core.memory.address_of<u32>(payload);
                    return Marker {};
                }
            "#,
            "set(&mut receipt, &payload)",
        ),
    ] {
        for (body, valid) in [
            (
                format!("let payload: u32 = 0; let returned = {call}; observe(receipt.pointer);"),
                true,
            ),
            (
                format!(
                    "let returned = {{ let payload: u32 = 0; yield {call}; }}; observe(receipt.pointer);"
                ),
                false,
            ),
            (
                format!(
                    "let returned = {{ let payload: u32 = 0; yield {call}; }}; observe_marker(&receipt);"
                ),
                true,
            ),
        ] {
            let application = compilation(&format!(
                r#"
                trusted module app;
                using core.memory;
                struct Marker
                {{
                }}
                struct Receipt
                {{
                    marker: Marker;
                    mut pointer: DataPointer<u32>;
                }}
                trusted predicate held(owner: &Marker);
                trusted func produce() -> Receipt
                    ensures(trusted held(owner = &result.marker))
                {{
                    return
                    {{
                        marker = Marker {{}}, pointer = core.memory.null<u32>()
                    }};
                }}
                func observe(pos pointer: DataPointer<u32>)
                    requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                {{
                }}
                func observe_marker(pos receipt: &Receipt)
                    requires(trusted held(owner = &receipt.marker))
                {{
                }}
                {setter}
                func caller()
                {{
                    let mut receipt = produce();
                    {body}
                }}
            "#
            ));

            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{setter}\n{body}\n{diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}
