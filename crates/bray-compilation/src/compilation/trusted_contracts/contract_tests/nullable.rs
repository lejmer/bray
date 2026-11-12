use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{source_function_dependencies};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn nullable_acquisition_contract_follows_the_present_owner() {
    for (body, valid) in [
        (
            "let value = acquire(); match consume value { case ?guard { observe(guard); } case none {} }",
            true,
        ),
        (
            "let value = acquire(); match consume value { case ?guard { let mut changed = guard; changed.active = false; observe(changed); } case none {} }",
            false,
        ),
        (
            "let value = acquire(); match consume value { case ?guard { let fake: Guard = { active = guard.active }; observe(fake); } case none {} }",
            false,
        ),
        ("forward(acquire());", true),
        (
            "let value = acquire(); match consume value { case ?guard { observe(guard); observe(guard); } case none {} }",
            false,
        ),
        (
            "let guard: Guard = { active = true }; observe(guard);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Guard {{ mut active: bool; }}
            trusted predicate held(value: Guard?);
            trusted func acquire() -> Guard?
                ensures(trusted held(result)) {{
                let guard: Guard = {{ active = true }};
                return guard;
            }}
            func observe(pos guard: Guard) requires(trusted held(guard)) {{}}
            func forward(pos value: Guard?) requires(trusted held(value)) {{
                match consume value {{ case ?guard {{ observe(guard); }} case none {{}} }}
            }}
            func caller() {{ {body} }}
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
fn nullable_borrowed_witnesses_preserve_their_portable_guards() {
    for (body, valid) in [
        ("return owner;", true),
        ("let stored: (&Owner)? = owner; return stored;", true),
        ("return lend(owner);", true),
        ("return other;", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker {{}}
            struct Owner {{ acquisition: Marker; }}
            trusted predicate held(owner: &Marker);
            extern func lend(pos owner: &Owner) -> &Owner
                requires(trusted held(owner = &owner.acquisition))
                ensures(trusted held(owner = &result.acquisition));
            const func marker(pos owner: &((&Owner)?)) -> &Marker
                requires(owner matches ?_)
                executes(pure, total)
            {{
                return match owner {{
                    case ?value {{ yield &value.acquisition; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}
            func forward(pos owner: &Owner, pos other: &Owner) -> (&Owner)?
                requires(trusted held(owner = &owner.acquisition))
                ensures(result matches none || trusted held(owner = marker(&result)))
            {{ {body} }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "forward"))
            .expect("nullable borrowed witness retains its checked body");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn mutated_nullable_owners_keep_promised_evidence_through_extraction() {
    for (deferred, promise, source_owner, route, valid) in [
        (false, true, "owner", "direct", true), (false, false, "owner", "direct", false),
        (false, true, "other", "direct", false), (true, true, "owner", "direct", true),
        (true, false, "owner", "direct", false), (true, true, "other", "direct", false),
        (false, true, "owner", "await", true), (false, false, "owner", "await", false),
        (false, true, "other", "await", false), (false, true, "owner", "cancel", true),
        (false, false, "owner", "cancel", false),
    ] {
        let asynchronous = if deferred { "async " } else { "" };
        let caller_async = if deferred || route != "direct" { "async " } else { "" };
        let execute = if deferred { "let mutation = trusted mutate(owner);\n                await mutation;" }
            else { "trusted mutate(owner);" };
        let extraction = match route {
            "direct" => format!("let outgoing = std.memory.replace_value<(Owner?)>({source_owner}, none);\n                consume_optional(outgoing);"),
            "await" => format!("let extraction = trusted forward_owner({source_owner});\n                let outgoing = await extraction;\n                consume_optional(outgoing);"),
            "cancel" => r#"
                {
                    let extraction = trusted forward_owner(owner);
                    _ = extraction;
                }

                observe_optional(&owner);
            "#.to_owned(),
            _ => unreachable!(),
        };
        let postcondition = if promise {
            "ensures(owner matches none || trusted optional_owner(owner = &owner))"
        } else { "" };
        let source = format!(r#"
            trusted module app;
            using std.memory;

            @layout(c)
            struct Marker
            {{
            }}

            @layout(c)
            struct Owner
            {{
                marker: Marker;
                bind: Enter;
            }}

            callable Enter = @abi(c) trusted func(pos owner: Owner) -> Owner
                requires(trusted acquired(owner = &owner.marker))
                ensures(trusted acquired(owner = &result.marker));

            trusted predicate acquired(owner: &Marker);

            const func present_owner(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_)
                executes(pure, total)
            {{
                return match owner
                {{
                    case ?value {{ yield &value; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}

            predicate optional_owner(owner: &(Owner?)) = owner matches none || acquired(owner = &present_owner(owner).marker);

            extern trusted {asynchronous}func mutate(pos owner: &mut(Owner?))
                requires(owner matches none || trusted optional_owner(owner = &owner))
                {postcondition};

            func consume_optional(pos owner: Owner?)
                requires(owner matches none || trusted optional_owner(owner = &owner))
            {{
            }}

            func observe_optional(pos owner: &(Owner?))
                requires(owner matches none || trusted optional_owner(owner = &owner))
            {{
            }}

            trusted async func forward_owner(pos owner: &mut(Owner?)) -> Owner?
            {{
                return std.memory.replace_value<(Owner?)>(owner, none);
            }}

            trusted {caller_async}func caller(pos owner: &mut(Owner?), pos other: &mut(Owner?))
                requires(owner matches none || trusted optional_owner(owner = &owner))
            {{
                {execute}
                {extraction}
            }}
        "#);
        let application = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::baseline(), &source);
        if route == "await" {
            let published = source_function_dependencies(&application, "forward_owner");
            assert!(!published.owned_result().is_empty(),
                "checked extraction must publish the source-to-result ownership transfer: {published:?}");
        }
        let checked = application.lowered_unit(source_function_body_key(&application, "caller"))
            .expect("nullable extraction caller reaches lowering");
        let diagnostics = checked.diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "deferred={deferred} promise={promise} source={source_owner} route={route}: {diagnostics:#?}");
        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn nested_nullable_absence_does_not_satisfy_outer_absence() {
    for (value, valid) in [("none", true), ("inner", false)] {
        let source = format!(
            r#"
            module app;
            func absent(pos value: (u32?)?) requires(value matches none) executes(total) {{}}
            func caller() executes(total) {{
                let inner: u32? = none;
                let outer: (u32?)? = {value};
                absent(outer);
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn nullable_call_coercions_preserve_presence() {
    for (argument, valid) in [("none", true), ("inner", false)] {
        let source = format!(
            r#"
            module app;
            func absent(pos value: (u32?)?) requires(value matches none) executes(total) {{}}
            func caller() executes(total) {{
                let inner: u32? = none;
                absent({argument});
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn nullable_aggregate_elements_keep_their_outer_discriminant() {
    for (declaration, access) in [
        ("let values: ((u32?)?, u32) = (outer, marker);", "values.0"),
        ("let values: [(u32?)?; 1] = [outer];", "values[0]"),
        ("let values: [(u32?)?; 1] = [outer; 1];", "values[0]"),
    ] {
        for (condition, valid) in [
            ("result matches none", false),
            ("!(result matches none)", true),
        ] {
            let source = format!(
                r#"
                module app;
                func caller() -> (u32?)? ensures({condition}) {{
                    let inner: u32? = none;
                    let outer: (u32?)? = inner;
                    let marker: u32 = 0;
                    {declaration}
                    return {access};
                }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "caller"))
                .expect("fixture callable must resolve its execution properties");

            let diagnostics = checked.diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn union_variant_contracts_share_checked_branch_evidence() {
    let application = compilation(
        r#"
        module app;
        const func unwrap(pos value: &Result<u32, u32>) -> u32
            requires(value matches Ok(_))
            executes(pure, total)
        {
            match value {
                case Ok(_) { return 7; }
                case Error(_) { panic("unreachable"); }
            }
        }
        func caller(pos value: Result<u32, u32>) {
            match value {
                case Ok(_) { let result = unwrap(&value); }
                case Error(_) {}
            }
        }
    "#,
    );

    for name in ["unwrap", "caller"] {
        let checked = application
            .execution_properties(source_function_body_key(&application, name))
            .expect("fixture callable must resolve its execution properties");

        assert!(
            !checked.diagnostics().has_errors(),
            "{name}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn union_variant_contracts_do_not_survive_replacement_or_prove_payload_tests() {
    for (action, pattern, valid) in [
        ("", "Ok(_)", true),
        (
            "let replacement: Result<u32, u32> = Error(9); holder.value = replacement;",
            "Ok(_)",
            false,
        ),
        (
            "let replacement: Result<u32, u32> = Ok(9); holder.value = replacement;",
            "Ok(_)",
            true,
        ),
        ("change(holder);", "Ok(_)", false),
        ("", "Ok(0)", false),
    ] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ mut value: Result<u32, u32>; }}
            func need(pos value: &Result<u32, u32>) requires(value matches {pattern}) executes(total) {{}}
            func change(pos holder: &mut Holder) executes(total) {{ let replacement: Result<u32, u32> = Error(9); holder.value = replacement; }}
            func caller(pos holder: &mut Holder) requires(holder.value matches Ok(_)) executes(total) {{
                {action}
                need(&holder.value);
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "caller"))
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn constructed_union_variants_establish_only_their_actual_tag() {
    for (constructed, valid) in [("Ok(7)", true), ("Error(7)", false)] {
        let source = format!(
            r#"
            module app;
            func create() -> Result<u32, u32> ensures(result matches Ok(_)) {{
                return {constructed};
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "create"))
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn nullable_propagation_keeps_only_the_successful_carriers_authority() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker {}
        struct Owner { instance: Marker; }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Owner ensures(trusted live(&result.instance)) { return { instance = Marker {} }; }
        const func marker(pos value: &(Owner?)) -> &Marker requires(!(value matches none)) executes(pure, total) {
            match value { case ?owner { return &owner.instance; } case none { panic("absent owner"); } }
        }
        func maybe(pos fail: bool) -> Owner? ensures(result matches none || trusted live(marker(&result))) {
            if fail { return none; }
            return make();
        }
        func observe(pos owner: &Owner) requires(trusted live(&owner.instance)) {}
        func caller(pos fail: bool) -> unit? {
            let owner: Owner = maybe(fail)?;
            observe(&owner);
            return unit;
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn nullable_input_adaptation_preserves_physical_owners_and_rejected_values() {
    for (operation, valid) in [("", true), ("replace(&mut owner);", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;

            struct Marker {{}}
            struct Owner {{ marker: Marker; pointer: DataPointer<u8>; }}
            struct Holder {{ value: Owner; }}
            trusted predicate live(owner: &Marker);
            trusted func make(pos pointer: DataPointer<u8>) -> Owner
                ensures(trusted live(&result.marker))
            {{
                return {{ marker = Marker {{}}, pointer = pointer }};
            }}
            const func present(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total)
            {{
                return match owner {{
                    case ?value {{ yield &value; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}
            predicate optional_live(owner: &(Owner?)) =
                owner matches none || live(&present(owner).marker);

            extern func exchange(pos pointer: DataPointer<u8>?) -> DataPointer<u8>;
            extern func replace(pos owner: (&mut Owner)?);
            extern func activity();
            func observe(pos owner: &Owner) requires(trusted live(&owner.marker)) {{}}
            func shared_view(pos owner: &mut Holder)
                requires(trusted live(&owner.value.marker))
                ensures(trusted live(&owner.value.marker))
            {{
                let selected = &owner.value;
                _ = exchange(selected.pointer);
                activity();
                observe(selected);
            }}
            func release(pos owner: Owner) requires(trusted live(&owner.marker)) {{}}
            func forward(pos owner: Owner?, pos reject: bool) -> Result<unit, (Owner?, u32)>
                requires(trusted optional_live(&owner))
            {{
                activity();
                if reject {{
                    let rejected: (Owner?, u32) = (owner, 4);
                    return Error(rejected);
                }}
                return Ok(unit);
            }}
            func caller_existing(pos mut owner: Owner, pos reject: bool)
                requires(trusted live(&owner.marker))
            {{
                _ = exchange(owner.pointer);
                {operation}
                match consume forward(owner, reject) {{
                    case Ok(_) {{}}
                    case Error((rejected, _)) {{
                        match consume rejected {{
                            case ?value {{ release(value); }}
                            case none {{}}
                        }}
                    }}
                }}
            }}
            func caller(pos pointer: DataPointer<u8>, pos reject: bool) {{
                caller_existing(make(pointer), reject);
            }}
            "#
        );

        let application = compilation(&source);

        assert!(!application.syntax_tree_result().diagnostics().has_errors(), "{source}");

        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic.kind(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                    | bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
            )), "{diagnostics:#?}");
        }
    }
}
