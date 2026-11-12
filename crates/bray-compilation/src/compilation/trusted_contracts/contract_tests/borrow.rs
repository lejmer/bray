use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn typed_data_transfers_preserve_borrowed_ownership_markers() {
    let mut failures = Vec::new();

    for (pointee, value, valid) in [
        ("u32", "1", true),
        ("(u32?)", "none", true),
        ("Data", "Data { value = 1 }", true),
        ("OtherPayload", "OtherPayload { marker = OtherMarker {}, value = 1 }", true),
        ("[u32; 2]", "[1, 2]", true),
        ("Uninit<[u32; 2]>", "core.memory.uninit_from<[u32; 2]>([1, 2])", true),
        ("Uninit<Payload>", "core.memory.uninit_from<Payload>(Payload { marker = Marker {}, value = 1 })", false),
        (
            "Payload",
            "Payload { marker = Marker {}, value = 1 }",
            false,
        ),
    ] {
        for (operation, capabilities) in [
            (
                format!("trusted core.memory.write<{pointee}>(destination, {value});"),
                "raw_memory, unchecked_init",
            ),
            (
                format!("_ = trusted core.memory.read<{pointee}>(destination);"),
                "raw_memory",
            ),
        ] {
            for (borrowed, mutable, expose, call) in [
                ("owner", "mut ", "_ = core.memory.address_of<Owner>(owner);", None),
                ("forward(owner)", "mut ", "_ = core.memory.address_of<Owner>(owner);", None),
                ("forward(owner)", "", "", None),
                ("forward(owner)", "", "", Some("trusted transfer(destination);")),
                ("forward(owner)", "", "", Some("trusted forward_transfer(borrowed);")),
                ("forward(owner)", "", "", Some("trusted transfer_external();")),
                ("forward(owner)", "", "", Some("trusted transfer_unresolved(address);")),
            ] {
            let invoke = call.unwrap_or(operation.as_str());
            let caller_capabilities = if call.is_some() { String::new() } else { format!("uses({capabilities})") };

            let source = format!(
                r#"
            trusted module app;
            using core.memory;
            using core.target;
            struct Marker
            {{
            }}
            struct OtherMarker
            {{
            }}
            struct Data
            {{
                value: u32;
            }}
            struct Payload
            {{
                marker: Marker;
                value: u32;
            }}
            struct OtherPayload
            {{
                marker: OtherMarker;
                value: u32;
            }}
            struct Owner
            {{
                marker: Marker;
                {mutable}data: u64;
                trusted destruct() requires(trusted held(&self.marker)) executes(total)
                {{
                }}
            }}
            trusted predicate held(marker: &Marker);
            extern trusted func pointer(pos owner: &Owner) -> DataPointer<{pointee}>
                requires(trusted held(&owner.marker))
                ensures(
                    trusted held(&owner.marker),
                    trusted core.memory.valid_write<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = result),
                    trusted core.memory.aligned_for<{pointee}>(pointer = result)
                )
                executes(pure, total);
            trusted predicate known_address(address: usize);
            extern trusted func external_pointer() -> DataPointer<{pointee}>
                ensures(
                    trusted core.memory.valid_write<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = result, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = result),
                    trusted core.memory.aligned_for<{pointee}>(pointer = result)
                ) executes(pure, total);
            trusted func transfer_external()
                uses({capabilities})
            {{
                let destination = external_pointer();
                {operation}
            }}
            trusted func transfer_unresolved(pos address: usize)
                requires(trusted known_address(address))
                uses(intrinsic, {capabilities})
            {{
                let destination = core.target.from_exposed_address<{pointee}>(address = address);
                let permit = trusted lambda(pos destination: DataPointer<{pointee}>, pos address: usize)
                    requires(trusted known_address(address))
                    ensures(
                        trusted core.memory.valid_write<{pointee}>(pointer = destination, count = 1),
                        trusted core.memory.valid_read<{pointee}>(pointer = destination, count = 1),
                        trusted core.memory.initialized_as<{pointee}>(pointer = destination),
                        trusted core.memory.aligned_for<{pointee}>(pointer = destination)
                    ) executes(pure, total) {{}};
                trusted permit(destination, address);
                {operation}
            }}
            trusted func transfer(pos destination: DataPointer<{pointee}>)
                requires(
                    trusted core.memory.valid_write<{pointee}>(pointer = destination, count = 1),
                    trusted core.memory.valid_read<{pointee}>(pointer = destination, count = 1),
                    trusted core.memory.initialized_as<{pointee}>(pointer = destination),
                    trusted core.memory.aligned_for<{pointee}>(pointer = destination)
                ) uses({capabilities}) {{ {operation} }}
            trusted func transfer_owner(pos owner: &Owner)
                requires(trusted held(&owner.marker))
                uses({capabilities})
            {{
                let destination = pointer(owner);
                {operation}
            }}
            trusted func forward_transfer(pos owner: &Owner)
                requires(trusted held(&owner.marker))
            {{
                trusted transfer_owner(owner);
            }}
            func forward(pos owner: &Owner) -> &Owner
                requires(trusted held(&owner.marker))
                ensures(trusted held(&result.marker)) executes(pure, total)
                {{
                    return owner;
                }}
            func observe(pos owner: &Owner)
                requires(trusted held(&owner.marker)) executes(pure, total) {{}}
            trusted func caller(pos owner: &Owner, pos address: usize)
                requires(trusted held(&owner.marker), trusted known_address(address))
                {caller_capabilities}
            {{
                {expose}
                let borrowed = {borrowed};
                let destination = pointer(borrowed);
                {invoke}
                observe(borrowed);
            }}
            "#
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            if diagnostics.has_errors() == valid {
                failures.push(format!("{source}: {diagnostics:#?}"));
                continue;
            }

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn borrowed_payload_writes_preserve_marker_evidence_without_retiring_the_carrier() {
    for (ty, result, argument, valid) in [
        ("Marker", "&result.marker", "&guard.marker", true),
        ("Guard", "&result", "guard", false),
    ] {
        for mutation in [
            "guard.payload.value = true;",
            "if flag { guard.payload.value = true; } else { guard.payload.value = false; }",
            "let index: usize = if flag { yield 0; } else { yield 1; }; guard.payload.counts[index] += 1; opaque();",
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Marker
                {{
                }}
                struct Payload
                {{
                    mut value: bool;
                    mut counts: [u32;
                    2];
                }}
                struct Guard
                {{
                    marker: Marker;
                    payload: &mut Payload;
                }}
                trusted predicate held(value: &{ty});
                trusted func acquire(pos payload: &mut Payload) -> Guard
                    ensures(trusted held({result})) executes(pure, total)
                {{
                    return
                    {{
                        marker = Marker {{}}, payload = payload
                    }};
                }}
                func observe(pos guard: &Guard)
                    requires(trusted held({argument})) executes(pure, total) {{}}
                extern func opaque();
                func caller(pos flag: bool)
                {{
                    let zero: u32 = 0;
                    let mut payload: Payload = {{ value = false, counts = [zero, zero] }};
                    let mut guard = acquire(&mut payload);
                    {mutation}
                    observe(&guard);
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
}

#[test]
fn consuming_wrapper_transfers_a_forwarded_stored_borrow() {
    let application = compilation(
        r#"
        module app;
        func forward(pos value: &u32) -> &u32
        {
            return value;
        }
        struct Wrapper
        {
            value: &u32;
            consume func get() -> &u32
            {
                return forward(self.value);
            }
        }
        func caller()
        {
            let value: u32 = 7;
            let wrapper: Wrapper = { value = &value };
            let borrowed = wrapper.get();
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}
