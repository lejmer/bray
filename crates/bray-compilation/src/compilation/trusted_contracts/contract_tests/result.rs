use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn caught_results_preserve_only_the_successful_owner() {
    for (body, valid) in [
        ("let value = acquire(); forward(value);", true),
        (
            "let caught = catch acquire(); match consume caught { case Ok(value) { forward(value); } case Error(_) {} }",
            true,
        ),
        (
            "let caught = catch { yield acquire(); }; match consume caught { case Ok(value) { forward(value); } case Error(_) {} }",
            true,
        ),
        (
            "let outer = catch catch acquire(); match consume outer { case Ok(inner) { match consume inner { case Ok(value) { forward(value); } case Error(_) {} } } case Error(_) {} }",
            true,
        ),
        (
            "let caught = catch acquire(); let value = match consume caught { case Ok(value) { yield value; } case Error(_) { return; } }; forward(value);",
            true,
        ),
        (
            "let caught = catch acquire(); match consume caught { case Ok(_) { let fake: Guard = { active = true }; forward(fake); } case Error(_) {} }",
            false,
        ),
        (
            "let caught = catch acquire(); match consume caught { case Ok(_) {} case Error(_) { let fake: Guard = { active = true }; forward(fake); } }",
            false,
        ),
    ] {
        let body = body.replace("acquire()", "acquire(fail)");

        let source = format!(
            r#"
            trusted module app;
            struct Guard
            {{
                active: bool;
            }}
            trusted predicate held(owner: &(Guard?));
            trusted func acquire(pos fail: bool) -> Guard?
                ensures(trusted held(owner = &result))
            {{
                if fail
                {{
                    panic("acquisition failed");
                }} let owner: Guard =
                {{
                    active = true
                }};
                return owner;
            }}
            func forward(pos owner: Guard?)
                requires(trusted held(owner = &owner))
            {{
            }}
            func caller(pos fail: bool)
            {{
                {body}
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
    }
}

#[test]
fn fallible_pair_factory_keeps_each_complete_owner() {
    let source = r#"
        trusted module app;
        struct Marker
        {
        }
        struct Owner
        {
            instance: Marker;
        }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Owner
            ensures(trusted live(&result.instance))
        {
            return
            {
                instance = Marker {}
            };
        }
        const func first(pos value: &Result<(Owner, Owner), u32>) -> &Marker
            requires(value matches Ok(_)) executes(pure, total)
        {
            match value
            {
                case Ok((owner, _))
                {
                    return &owner.instance;
                }
                case Error(_)
                {
                    panic("absent owner");
                }
            }
        }
        const func second(pos value: &Result<(Owner, Owner), u32>) -> &Marker
            requires(value matches Ok(_)) executes(pure, total)
        {
            match value
            {
                case Ok((_, owner))
                {
                    return &owner.instance;
                }
                case Error(_)
                {
                    panic("absent owner");
                }
            }
        }
        func pair(pos fail: bool) -> Result<(Owner, Owner), u32>
            ensures(!(result matches Ok(_)) || trusted live(first(&result)), !(result matches Ok(_)) || trusted live(second(&result)))
        {
            if fail
            {
                return Error(1);
            }
            return Ok((make(), make()));
        }
        func observe(pos owner: &Owner)
            requires(trusted live(&owner.instance))
        {
        }
        func caller(pos fail: bool) -> Result<unit, u32>
        {
            let (left, right) = try pair(fail);
            observe(&left);
            observe(&right);
            return Ok(unit);
        }
    "#;

    for valid in [true, false] {
        let source = if valid {
            source.to_owned()
        } else {
            source
                .replace(
                    "return Ok((make(), make()));",
                    "return Ok((make(), Owner { instance = Marker {} }));",
                )
                .replace(
                    ", !(result matches Ok(_)) || trusted live(second(&result))",
                    "",
                )
        };

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn forwarding_a_union_owner_preserves_the_checked_payload_projection() {
    for (operation, valid) in [("", true), ("replace(&mut owner);", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;

            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
                pointer: DataPointer<u8>;
            }}
            union Backing
            {{
                Empty;
                Owned(pos owner: Owner);
            }}
            trusted predicate live(owner: &Marker);
            const func backing_owner(pos backing: &Backing) -> &Owner
                requires(backing matches Backing.Owned(_))
                executes(pure, total)
            {{
                return match backing
                {{
                    case Backing.Owned(value)
                    {{
                        yield &value;
                    }}
                    case Backing.Empty
                    {{
                        panic("empty backing has no owner");
                    }}
                }};
            }}
            predicate retained(owner: &Backing) = owner matches Backing.Owned(_) && live(&backing_owner(owner).marker);
            extern trusted func native_allocate() -> DataPointer<u8>
                ensures(trusted core.memory.owned_allocation(pointer = result, bytes = 1, align = 1));
            trusted func allocate(pos owner: &Backing) -> DataPointer<u8>
                requires(trusted retained(owner))
                ensures(trusted retained(owner), trusted core.memory.owned_allocation(pointer = result, bytes = 1, align = 1))
            {{
                return trusted native_allocate();
            }}
            extern func replace(pos owner: &mut Backing);
            func consume_owner(pos owner: Backing)
                requires(trusted retained(&owner))
            {{
            }}
            trusted func caller(pos mut owner: Backing)
                requires(trusted retained(&owner))
                uses(manual_alloc)
            {{
                let allocation = trusted allocate(&owner);
                {operation}
                consume_owner(owner);
                trusted core.memory.deallocate(allocation, bytes = 1, align = 1);
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
    }
}

#[test]
fn returned_owner_facts_follow_only_the_active_result_payload() {
    for (mutation, valid) in [("", true), ("if change { owner.active = false; }", false)] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Owner
            {{
                mut active: bool;
                pointer: DataPointer<u8>;
            }}
            trusted predicate held(owner: &Owner);
            extern trusted func opaque(pos owner: &Owner) -> DataPointer<u8>
                requires(trusted held(owner))
                ensures(trusted held(owner));
            func forward(pos mut owner: Owner, pos reject: bool, pos change: bool) -> Result<u8, Owner>
                requires(trusted held(&owner))
            {{
                _ = trusted opaque(&owner);

                if reject
                {{
                    {mutation}
                    return Error(owner);
                }}

                return Ok(0);
            }}
            func use_owner(pos owner: Owner)
                requires(trusted held(&owner))
            {{
            }}
            func caller(pos owner: Owner, pos reject: bool, pos change: bool)
                requires(trusted held(&owner))
            {{
                match consume forward(owner, reject, change)
                {{
                    case Ok(_)
                    {{
                    }}
                    case Error(rejected)
                    {{
                        use_owner(rejected);
                    }}
                }}
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
    }
}

#[test]
fn ownership_guarantees_precede_fallible_wrapping() {
    for (initializer, valid) in [
        ("introduce()", true),
        (r"Owner
            {
                marker = Marker
                {
                },
            }", false),
    ] {
        let source = r#"
        trusted module app;

        struct Marker
        {
        }

        trusted predicate live(marker: &Marker);

        struct Owner
        {
            marker: Marker;

            trusted destruct()
                requires(trusted live(&self.marker))
            {
            }
        }

        struct Wrapped
        {
            owner: Owner;
        }

        trusted func introduce() -> Owner
            ensures(trusted live(&result.marker))
        {
            return
            {
                marker = Marker
                {
                },
            };
        }

        func inspect(pos fail: bool)
        {
            if fail
            {
                panic("owner wrapping probe");
            }
        }

        trusted func wrap(pos fail: bool) -> Wrapped
            ensures(trusted live(&result.owner.marker))
        {
            let owner: Owner = INITIAL_VALUE;

            inspect(fail);

            return
            {
                owner = owner,
            };
        }
        "#.replace("INITIAL_VALUE", initializer);
        let compilation = crate::test_support::compilation(&source);

        assert_eq!(!compilation.check_diagnostics().has_errors(), valid, "{source}: {:?}", compilation.check_diagnostics());

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn production_completion_predicates_match_returned_booleans() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");
    let symbols = compilation.symbol_graph().expect("production symbols are available");
    let mut failures = Vec::new();
    let mut bodies = 0;

    for key in compilation.declared_unit_keys_for_test().expect("production bodies are declared") {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || !symbols.symbol_for_key(key.declared_owner()).and_then(|owner| symbols.member_name(owner))
                .is_some_and(|name| name.as_str() == "is_complete") {
            continue;
        }

        bodies += 1;
        let properties = compilation.execution_properties(key.clone()).expect("completion predicate is certified");
        let body = compilation.lowered_unit(key.clone()).expect("completion predicate lowers");

        if properties.diagnostics().has_errors() || body.diagnostics().has_errors() {
            failures.push((key, properties.diagnostics().clone(), body.diagnostics().clone()));
        }
    }

    assert!(bodies > 0, "actual completion predicates must be selected");
    assert!(failures.is_empty(), "{failures:?}");
}
