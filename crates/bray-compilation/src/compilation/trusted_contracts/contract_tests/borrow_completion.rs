use bray_diagnostics::{DiagnosticArgValue, DiagnosticContractObligationKind, DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn owned_postconditions_follow_a_scoped_mutable_alias() {
    for (body, valid) in [
        (
            "let mut other: Flags = { ready = false }; { let mut borrowed: &mut Flags = &mut value; if condition { borrowed = &mut other; } borrowed.ready = true; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = true; }",
            true,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = false; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; borrowed.ready = true; } value.ready = false;",
            false,
        ),
        (
            "let mut other: Flags = { ready = false }; { let borrowed: &mut Flags = &mut other; borrowed.ready = true; }",
            false,
        ),
        (
            "let mut other: Flags = { ready = false }; { let mut borrowed: &mut Flags = &mut value; borrowed = &mut other; borrowed.ready = true; }",
            false,
        ),
        (
            "{ let borrowed: &mut Flags = &mut value; let nested: &mut Flags = borrowed; nested.ready = true; }",
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Flags
            {{
                mut ready: bool;
            }}
            func root()
            {{
                let action = lambda(pos mut value: Flags, pos condition: bool) -> Flags
                    ensures(result.ready) executes(total)
                {{
                    {body}
                    return value;
                }};
            }}
            "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );
    }
}

#[test]
fn compound_borrow_postconditions_agree_with_the_next_owner_read() {
    for (replacement, valid) in [("", true), ("lease = { first = first, second = second };", false)] {
        let source = format!(r#"
            trusted module app;

            struct Cell
            {{
                mut value: u32;
            }}

            struct Lease
            {{
                first: &Cell;
                second: &Cell;
            }}

            trusted predicate acquired(owner: &Lease);

            extern trusted func checkpoint(pos owner: &Lease) -> bool
                ensures(result || trusted acquired(owner = owner));

            func observe(pos owner: &Lease)
                requires(trusted acquired(owner = owner))
            {{
            }}

            func caller(pos first: &Cell, pos second: &Cell)
            {{
                let mut lease: Lease = {{ first = first, second = second }};
                let ready = checkpoint(&lease);

                if !ready
                {{
                    {replacement}
                    observe(&lease);
                }}
            }}
        "#);
        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn borrowed_scalar_authority_tracks_storage_independently_of_copied_values() {
    for (parameter, initial, action, valid) in [
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "observe(&value);",
            true,
        ),
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "observe(&other);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1; let mut other: u8 = 1;",
            "other = 2; observe(&value);",
            true,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 1; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; value = 1; observe(&value);",
            false,
        ),
        (
            "",
            "let mut value: u8 = 1;",
            "value = 2; establish(&value); observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "observe(&other);",
            false,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "other = 2; observe(&value);",
            true,
        ),
        (
            "pos mut value: u8",
            "let mut other = value;",
            "value = other; observe(&value);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted func establish(pos value: &u8)
                executes(pure, total)
                ensures(trusted live(value)) {{}}
            func observe(pos value: &u8)
                requires(trusted live(value))
            {{
            }}
            func caller({parameter})
            {{
                {initial} establish(&value);
                {action}
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
fn borrowed_postconditions_name_the_updated_storage_only() {
    let mut failures = Vec::new();
    let mut completed = 0;

    for (case, (action, observed, valid)) in [
        (
            "{ let mut writer: Writer = { slot = &mut first }; set(&mut writer); observe(&writer.slot); }",
            "first",
            true,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; }",
            "first",
            true,
        ),
        (
            "{ let mut writer: Writer = { slot = &mut first }; forward(&mut writer); observe(&writer.slot); }",
            "first",
            true,
        ),
        (
            "{ let mut writer: Writer = { slot = &mut first }; forward(&mut writer); writer.slot.epoch = 2; observe(&writer.slot); }",
            "first",
            false,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; }",
            "second",
            false,
        ),
        (
            "{ let mut writer: Writer = { slot = &mut first }; set(&mut writer); writer.slot.epoch = 2; observe(&writer.slot); }",
            "first",
            false,
        ),
        (
            "{ let writer: Writer = { slot = &mut first }; } first.epoch = 2;",
            "first",
            false,
        ),
    ].into_iter().enumerate() {
        let forward = if action.contains("forward(") { r#"
            func forward(pos writer: &mut Writer)
                ensures(trusted live(&writer.slot))
            {
                set(writer);
            }
        "# } else { "" };
        let source = format!(
            r#"
            trusted module app;

            struct Owner
            {{
                mut epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            struct Writer
            {{
                slot: &mut Owner;

                trusted destruct()
                    ensures(trusted live(&self.slot))
                {{
                    self.slot.epoch = 1;
                }}
            }}

            trusted func set(pos writer: &mut Writer)
                ensures(trusted live(&writer.slot))
            {{
                writer.slot.epoch = 1;
            }}

            {forward}

            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}

            func caller()
            {{
                let mut first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 0 }};

                {action}
                observe(&{observed});
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        let boundaries = diagnostics.iter().flat_map(|diagnostic| diagnostic.args())
            .filter_map(|argument| match argument.value() {
                DiagnosticArgValue::ContractObligation(obligation) => Some(obligation.kind),
                _ => None,
            }).collect::<Vec<_>>();
        completed += 1;

        if diagnostics.has_errors() == valid {
            failures.push(format!("nested borrowed case={case}, valid={valid}, boundaries={boundaries:?}"));
        }

        if !valid && !boundaries.contains(&DiagnosticContractObligationKind::CallRequirement) {
            failures.push(format!("nested borrowed case={case} did not reject the caller independently: {boundaries:?}"));
        }
    }

    assert!(failures.is_empty(), "nested borrowed cases completed: count={completed}\n{}", failures.join("\n"));
}

#[test]
fn scoped_borrow_postconditions_survive_capability_retirement() {
    let mut failures = Vec::new();

    for asynchronous in ["", "async"] {
    for (case, (action, promised, valid)) in [
        ("observe(&first);", true, true),
        ("let moved = relay(first); observe(&moved);", true, true),
        ("first.epoch = 2; observe(&first);", true, false),
        ("observe(&second);", true, false),
        ("observe(&first);", false, false),
    ].into_iter().enumerate() {
        let guarantee = if promised { "ensures(trusted live(&lease.slot))" } else { "" };
        let source = format!(r#"
            trusted module app;

            struct Owner
            {{
                mut epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            struct Lease
            {{
                slot: &mut Owner;
            }}

            struct Scope
            {{
                slot: &mut Owner;

                trusted consume enter() -> Lease
                    ensures(trusted live(&result.slot))
                {{
                    return
                    {{
                        slot = self.slot
                    }};
                }}

                trusted {asynchronous} exit(pos lease: Lease)
                    {guarantee}
                {{
                    lease.slot.epoch = 1;
                }}
            }}

            func relay(pos owner: Owner) -> Owner
                requires(trusted live(&owner))
                ensures(trusted live(&result))
            {{
                return owner;
            }}

            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}

            {asynchronous} func caller()
            {{
                let mut first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 0 }};
                let scope: Scope = {{ slot = &mut first }};

                with lease = scope
                {{
                    observe(&lease.slot);
                }};

                {action}
            }}
        "#);
        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();
        let kinds = diagnostics.iter().map(|diagnostic| diagnostic.kind()).collect::<Vec<_>>();

        if diagnostics.has_errors() == valid {
            failures.push(format!("scoped borrow asynchronous={asynchronous:?}, case={case}, valid={valid}, diagnostics={kinds:?}"));
        }

        if !valid && !kinds.contains(&DiagnosticKind::CheckingTrustedObligationNotProven) {
            failures.push(format!("scoped borrow asynchronous={asynchronous:?}, case={case} lacks its authority rejection: {kinds:?}"));
        }
    }

    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn completion_observes_the_evaluated_borrow_after_its_source_slot_changes() {
    for (invocation, observed, expected) in [
        (
            "redirect(holder.slot, &mut holder);",
            "first",
            Some(DiagnosticKind::CheckingConflictingBorrow),
        ),
        (
            "let selected: &Owner = holder.slot; redirect(selected, &mut holder);",
            "first",
            None,
        ),
        (
            "let selected: &Owner = holder.slot; redirect(selected, &mut holder);",
            "second",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
            }}
            struct Holder
            {{
                mut slot: &Owner;
                other: &Owner;
            }}
            trusted predicate live(value: &Owner);
            trusted func redirect(pos original: &Owner, pos holder: &mut Holder)
                ensures(trusted live(original)) {{ holder.slot = holder.other; }}
            func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}
            func caller()
            {{
                let first: Owner = {{ epoch = 0 }};
                let second: Owner = {{ epoch = 1 }};
                let mut holder: Holder = {{ slot = &first, other = &second }};
                {invocation}
                observe(&{observed});
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            diagnostics.has_errors(),
            expected.is_some(),
            "{source}: {diagnostics:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
        }
    }
}

#[test]
fn field_projection_preserves_the_evidence_of_a_borrowed_call_result() {
    for binding in [
        "let token = &project(owner).token;",
        "let value = project(owner); let token = &value.token;",
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Token
            {{
            }}
            struct Owner
            {{
                token: Token;
            }}
            struct Holder
            {{
                current: Owner;
                label: u32;
            }}
            extern func opaque();
            impl Owner
            {{
                func check()
                    executes(pure, total)
                {{
                }}
            }}
            trusted predicate live(owner: &Token);
            func project(pos owner: &Owner) -> &Owner
                requires(trusted live(&owner.token))
                ensures(trusted live(&result.token))
                executes(pure, total)
            {{
                return owner;
            }}
            func observe(pos token: &Token)
                requires(trusted live(token)) executes(pure, total) {{}}
            trusted func nested(pos owner: &Holder) -> &Holder
                ensures(trusted live(&result.current.token))
                executes(pure, total)
            {{
                return owner;
            }}
            extern trusted func introduced(pos owner: &Holder) -> &Holder
                ensures(trusted live(&result.current.token));
            func nested_caller(pos owner: &Holder)
            {{
                let selected = nested(owner);
                opaque();
                observe(&selected.current.token);

                let fresh = introduced(owner);

                opaque();
                observe(&fresh.current.token);
            }}
            func caller(pos owner: &Owner)
                requires(trusted live(&owner.token))
            {{
                {binding}
                observe(token);
                project(owner).check();
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");
    }
}

#[test]
fn const_borrowed_getters_preserve_the_existing_carrier_identity() {
    let source = r#"
        trusted module app;
        struct Marker
        {
        }
        struct Owner
        {
            marker: Marker;
            const func token() -> &Marker
                executes(pure, total)
            {
                return &self.marker;
            }
            func inspect_after_argument(pos sequence: u32)
                requires(trusted live(&self.marker)) executes(pure, total) {}
        }
        union Backing
        {
            Empty;
            Owned(pos owner: Owner);
        }
        trusted predicate live(owner: &Marker);
        const func owner(pos backing: &Backing) -> &Owner
            requires(backing matches Backing.Owned(_))
            executes(pure, total)
        {
            return match backing
            {
                case Backing.Owned(value)
                {
                    yield &value;
                }
                case Backing.Empty
                {
                    panic("missing owner");
                }
            };
        }
        predicate retained(backing: &Backing) = backing matches Backing.Owned(_) && live(owner(backing).token());
        func observe(pos marker: &Marker)
            requires(trusted live(marker))
            executes(pure, total)
        {
        }
        func argument_value() -> u32
            executes(pure, total)
        {
            return 0;
        }
        func caller(pos backing: &Backing)
            requires(trusted retained(backing))
        {
            if backing matches Backing.Owned(_)
            {
                observe(owner(backing).token());
                let selected = owner(backing);
                observe(selected.token());
                owner(backing).inspect_after_argument(argument_value());
                selected.inspect_after_argument(argument_value());
                let copied = selected;
                copied.inspect_after_argument(argument_value());
            }
        }
    "#;

    let application = compilation(source);
    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn mutable_union_completion_observes_the_updated_owner() {
    for (preserved, valid) in [("trusted held(value = &value),", true), ("", false)] {
        let source = r#"trusted module app;
using std.memory;
struct Marker
{
}
struct Reference
{
    marker: Marker;
}
struct Row
{
    mut provider: Reference;
}
union Value
{
    First(pos mut row: Row);
    Second(pos mut row: Row);
}
trusted predicate held(value: &Value);
trusted predicate reference_held(value: &Marker);
trusted func fresh() -> Reference
    ensures(trusted reference_held(value = &result.marker))
    executes(pure,total)
{
    return
    {
        marker = Marker {}
    };
}
trusted func take(pos value: &mut Value) -> Reference
    requires(trusted held(value = &value))
    ensures( $PRESERVED trusted reference_held(value = &result.marker))
{
    return match value
    {
        case Value.First(row)
        {
            yield trusted std.memory.replace_value<Reference>(&mut row.provider, fresh());
        }
        case Value.Second(row)
        {
            yield trusted std.memory.replace_value<Reference>(&mut row.provider, fresh());
        }
    };
}
func observe(pos value: &mut Value, pos provider: &Reference)
    requires(trusted held(value = &value), trusted reference_held(value = &provider.marker)) executes(pure,total) {}
func caller(pos mut value: Value)
    requires(trusted held(value = &value))
{
    let provider = trusted take(&mut value);
    observe(&mut value, &provider);
}
"#.replace("$PRESERVED", preserved);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");
    }
}
