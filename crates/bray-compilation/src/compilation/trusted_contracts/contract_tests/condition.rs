use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn ordinary_consequences_of_mixed_guarantees_do_not_restore_revoked_authority() {
    for (intervening, valid) in [("", true), ("opaque(&mut owner);", false)] {
        let source = format!(
            r#"
            trusted module app;

            struct Owner
            {{
                mut ready: bool;
            }}
            trusted predicate valid(owner: &Owner);

            trusted func acquire() -> Owner
                ensures(result.ready || trusted valid(owner = &result))
            {{
                return
                {{
                    ready = false
                }};
            }}

            extern func opaque(pos owner: &mut Owner);

            func use_value(pos owner: &Owner)
                requires(trusted valid(owner = owner))
            {{
            }}

            func caller()
            {{
                let mut owner = trusted acquire();
                if !owner.ready
                {{
                    {intervening}
                    use_value(&owner);
                }}
            }}
        "#
        );

        let checked = compilation(&source);
        let diagnostics = checked.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            assert!(
                diagnostics.iter().all(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingTrustedObligationNotProven),
                "{source}: {diagnostics:#?}"
            );
        }
    }
}

#[test]
fn recovering_source_retains_its_diagnostic_inside_a_refined_payload() {
    let application = compilation(
        r#"
        module app;

        func selected(pos input: u32?)
        {
            if let ?value = input
            {
                missing();
                let observed: u32 = value;
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::BindingUnresolvedName,
    );
}

#[test]
fn conditional_trusted_guarantees_require_the_selected_entry_guard() {
    for (flag, valid) in [("true", true), ("false", false)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner
            {{
                byte: u8;
            }}
            trusted predicate live(value: &Owner);
            trusted func owner(pos flag: bool) -> Owner
                when(flag) {{ ensures(trusted live(&result)) }}
            {{
                return
                {{
                    byte = 1
                }};
            }}
            func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}
            func caller()
            {{
                let value = owner({flag});
                observe(&value);
            }}
        "#
        ));

        let diagnostics = compilation.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");
    }
}

#[test]
fn scoped_guarantees_and_requirements_use_the_actual_capability() {
    for (enter_mode, exit_mode, caller_mode) in [
        ("", "", ""),
        ("async ", "", "async "),
        ("", "async ", "async "),
        ("async ", "async ", "async "),
    ] {
        for (enter_contract, exit_contract, body, valid) in [
            (
                "ensures(trusted live(&result))",
                "",
                "observe(&lease);",
                true,
            ),
            ("", "", "observe(&lease);", false),
            (
                "ensures(trusted live(&result))",
                "requires(trusted live(&lease))",
                "",
                true,
            ),
            ("", "requires(trusted live(&lease))", "", false),
        ] {
            let compilation = compilation(&format!(
                r#"
            trusted module app;
            struct Resource
            {{
            }}
            struct Lease
            {{
                tag: bool;
            }}
            trusted predicate live(value: &Lease);
            impl Resource
            {{
                trusted {enter_mode}enter() -> Lease {enter_contract}
                {{
                    return
                    {{
                        tag = true
                    }};
                }}
                {exit_mode}exit(pos lease: Lease) {exit_contract}
                {{
                }}
            }}
            func observe(pos value: &Lease)
                requires(trusted live(value))
            {{
            }}
            {caller_mode}func caller(pos resource: Resource)
            {{
                with lease = resource {{ {body} }};
            }}
            "#,
            ));

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "enter {enter_mode:?}, exit {exit_mode:?}: {diagnostics:?}"
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
fn ordinary_predicate_assumptions_cannot_discharge_trusted_disjunctions() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func observe(pos value: u64, pos flag: bool)
            requires((trusted live(value)) || flag)
        {
        }
        func caller(pos value: u64, pos flag: bool)
            requires(live(value))
        {
            observe(value, flag);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn mixed_boolean_requirements_preserve_trusted_occurrences_and_ordinary_guards() {
    for (requirement, precondition, valid) in [
        ("(trusted live(value)) || flag", "live(value)", false),
        ("flag || trusted live(value)", "live(value)", false),
        ("(trusted live(value)) && flag", "live(value), flag", false),
        ("!(trusted live(value)) || flag", "!live(value)", false),
        ("flag || trusted live(value)", "flag", true),
        ("(trusted live(value)) || flag", "flag", true),
        ("ready(value) || trusted live(value)", "ready(value)", true),
        (
            "!(trusted live(value)) || ready(value)",
            "ready(value)",
            true,
        ),
        ("(trusted live(value)) || flag", "trusted live(value)", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            func observe(pos value: u64, pos flag: bool)
                requires({requirement})
            {{
            }}
            func caller(pos value: u64, pos flag: bool)
                requires({precondition})
            {{
                observe(value, flag);
            }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn selecting_an_ordinary_disjunct_does_not_grant_its_trusted_qualification() {
    for requirement in [
        "live(value) || trusted ready(value)",
        "(trusted ready(value)) || live(value)",
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            func observe(pos value: u64)
                requires(trusted live(value))
            {{
            }}
            func caller(pos value: u64)
                requires({requirement}, !(trusted ready(value)))
            {{
                observe(value);
            }}
        "#
        );

        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn trusted_disjunctions_retain_evidence_on_either_side() {
    for requirement in [
        "(trusted live(value)) || flag",
        "flag || trusted live(value)",
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func observe(pos value: u64)
                requires(trusted live(value))
            {{
            }}
            func caller(pos value: u64, pos flag: bool)
                requires({requirement}, !flag)
            {{
                observe(value);
            }}
        "#
        );

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn ordinary_predicate_equalities_cannot_rename_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        trusted predicate other(value: u64);
        func observe(pos value: u64)
            requires(trusted other(value))
        {
        }
        func caller(pos value: u64)
            requires(trusted live(value), other(value) == live(value))
        {
            observe(value);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn ordinary_predicate_guards_select_trusted_conditional_guarantees() {
    for (precondition, valid) in [("requires(ready(value))", true), ("", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate ready(value: u64);
            trusted func make(pos value: u64) -> u64
                when(ready(value))
            {{
                ensures(trusted live(result))
            }}
            {{
                return value;
            }}
            func observe(pos value: u64)
                requires(trusted live(value))
            {{
            }}
            func caller(pos value: u64) {precondition}
            {{
                observe(make(value));
            }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn generic_trait_fulfillments_preserve_exact_trusted_predicates() {
    for (predicate, valid) in [("live", true), ("other", false)] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            trusted predicate live<T>(value: &T);
            trusted predicate other<T>(value: &T);
            trait Observer<T>
            {{
                static func observe(pos value: &T) requires(trusted live<T>(value));
            }}
            struct Watcher
            {{
            }}
            impl Watcher(Observer<u64>)
            {{
                static func observe(pos value: &u64) requires(trusted {predicate}<u64>(value))
                {{
                }}
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn trusted_caller_obligations_require_matching_evidence() {
    for (requirement, invocation, valid) in [
        ("", "use_owner(value);", false),
        ("", "trusted use_owner(value);", false),
        ("requires(trusted live(value))", "use_owner(value);", true),
        ("requires(trusted other(value))", "use_owner(value);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate live(value: u64);
            trusted predicate other(value: u64);

            trusted func use_owner(pos value: u64)
                requires(trusted live(value)) {{}}

            func caller(pos value: u64) {requirement}
            {{
                {invocation}
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
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn safe_guarantees_cannot_fabricate_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func fabricated() -> u64
            ensures(trusted live(result))
        {
            return 1;
        }
        trusted func use_owner(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            use_owner(fabricated());
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn unknown_observations_do_not_merge_unrelated_value_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        @copy struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        trusted func establish(pos value: &Owner)
            executes(pure, total)
            ensures(trusted live(value)) {}
        func caller()
        {
            let value: Owner = { epoch = 1 };
            let unrelated: Owner = { epoch = 2 };
            establish(&value);
            let copied = unrelated;
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn ordinary_conditions_do_not_create_trusted_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func ordinary(pos value: u64)
            requires(live(value))
        {
        }
        trusted func observe(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller(pos value: u64)
            requires(live(value))
        {
            observe(value);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn runtime_checked_ordinary_requirements_establish_completion_guards() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            epoch: u64;
        }
        trusted predicate live(value: &Owner);
        trusted func owner(pos epoch: u64) -> Owner
            requires(epoch == 1)
            when(epoch == 1) { ensures(trusted live(&result)) }
        {
            return
            {
                epoch = epoch
            };
        }
        func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller(pos epoch: u64)
        {
            let value = owner(epoch);
            observe(&value);
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn ordinary_equalities_match_existing_trusted_conditions() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        trusted func observe(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller(pos left: u64, pos right: u64)
            requires(left == right, trusted live(left)) { observe(right); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn nested_trust_in_requirements_cannot_hide_obligations() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func observe(pos value: u64)
            requires((trusted live(value)) && true)
        {
        }
        func caller()
        {
            observe(1);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn trusted_predicate_guarantees_preserve_execution_property_proofs() {
    for (body, valid) in [("return { epoch = 1 };", true), ("loop {}", false)] {
        let source = format!(
            r#"
            trusted module app;

            struct Owner
            {{
                epoch: u64;
            }}

            trusted predicate live(value: &Owner);

            trusted func owner() -> Owner
                executes(pure, total)
                ensures(trusted live(&result))
            {{
                {body}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn established_observation_free_predicates_survive_opaque_execution() {
    for (arguments, values) in [("", ""), ("value: u64", "7")] {
        for established in [false, true] {
            let requirement = if established {
                format!("requires(trusted permitted({values}))")
            } else {
                String::new()
            };

            let source = format!(
                r#"
                trusted module app;
                trusted predicate permitted({arguments});
                extern func opaque();
                func restricted()
                    requires(trusted permitted({values}))
                    executes(pure, total)
                {{
                }}
                func run() {requirement}
                {{
                    opaque();
                    restricted();
                }}
                "#,
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                established,
                "{source}: {diagnostics:#?}"
            );

            if !established {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}
