use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn borrowed_invariants_survive_loops_only_when_every_back_edge_preserves_them() {
    for (body, valid) in [
        ("trusted advance(&mut owner);", true),
        ("if choose { trusted advance(&mut owner); }", true),
        (
            "if choose { trusted advance(&mut owner); } else { owner.value += 1; }",
            false,
        ),
        ("owner.value += 1;", false),
        ("trusted advance(&mut owner); owner.value += 1;", false),
        (
            "if choose { trusted advance(&mut owner); } else { trusted advance(&mut owner); } owner.value += 1;",
            false,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner
            {{
                mut value: usize;
            }}
            trusted predicate valid(owner: &Owner);
            extern trusted func observe(pos owner: &Owner)
                requires(trusted valid(owner = owner))
                ensures(trusted valid(owner = owner));
            extern trusted func advance(pos owner: &mut Owner)
                requires(trusted valid(owner = &owner))
                ensures(trusted valid(owner = &owner));
            trusted func caller(pos mut owner: Owner, pos choose: bool)
                requires(trusted valid(owner = &owner))
            {{
                loop
                {{
                    trusted observe(&owner);
                    {body}
                }}
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn cursor_witness_survives_observation_and_independent_loop_counters() {
    let source = r#"
        trusted module app;

        using core.memory;
        struct Cursor
        {
            first: DataPointer<u8>;
            limit: usize;
            mut current: DataPointer<u8>;
            mut remaining: usize;
        }
        trusted predicate valid(cursor: &Cursor);
        trusted func observe(pos cursor: &Cursor) -> (DataPointer<u8>, usize)
            requires(trusted valid(cursor = cursor))
            ensures(
                trusted valid(cursor = cursor),
                core.memory.is_null<u8>(result.0) || trusted core.memory.valid_read<u8>(pointer = result.0, count = result.1),
                core.memory.is_null<u8>(result.0) || trusted core.memory.initialized_range_as<u8>(pointer = result.0, count = result.1),
            )
        {
            return (cursor.current, cursor.remaining);
        }
        trusted func advance(pos cursor: &mut Cursor)
            requires(trusted valid(cursor = &cursor))
            ensures(trusted valid(cursor = &cursor))
        {
            cursor.remaining -= 1;
        }
        trusted func caller(pos mut cursor: Cursor) -> usize
            requires(trusted valid(cursor = &cursor))
        {
            let mut total: usize = 0;
            let mut length: usize = 0;

            loop
            {
                let observed: (DataPointer<u8>, usize) = trusted observe(&cursor);

                if core.memory.is_null<u8>(observed.0)
                {
                    break;
                }

                if total > 1000 - observed.1 - 1
                {
                    panic("too large");
                }

                total += observed.1 + 1;
                length += 1;
                trusted advance(&mut cursor);
            }

            return total + length;
        }
    "#;

    let mut results = Vec::new();

    for observe in [false, true] {
        for advance in [false, true] {
            let mut source = source.to_owned();

            if !observe {
                source = source
                    .replace("trusted func observe", "extern trusted func observe")
                    .replace(
                        "{\n            return (cursor.current, cursor.remaining);\n        }",
                        ";",
                    );
            }

            if !advance {
                source = source
                    .replace("trusted func advance", "extern trusted func advance")
                    .replace("{\n            cursor.remaining -= 1;\n        }", ";");
            }

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            results.push((observe, advance, !diagnostics.has_errors()));
        }
    }

    assert_eq!(
        results,
        vec![
            (false, false, true),
            (false, true, true),
            (true, false, true),
            (true, true, true)
        ]
    );
}

#[test]
fn terminating_branches_do_not_invalidate_authority_on_returning_paths() {
    for (helper, valid) in [
        (
            r#"
                trusted func boundary(pos stop: bool)
                    uses(intrinsic)
                {
                    if stop
                    {
                        trusted core.target.abort();
                    }
                }
            "#,
            true,
        ),
        ("extern trusted func boundary(pos stop: bool);", false),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;

            using core.target;
            struct Owner
            {{
                pointer: DataPointer<u8>;
            }}
            trusted predicate live(owner: &Owner);
            {helper}
            func observe(pos owner: &Owner)
                requires(trusted live(owner))
            {{
            }}
            func caller(pos owner: &Owner, pos stop: bool)
                requires(trusted live(owner))
            {{
                trusted boundary(stop);
                observe(owner);
            }}
            "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{helper}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn yielded_owners_preserve_only_common_witnesses() {
    for (expression, intervening, valid) in [
        (
            "if flag { yield produce(1); } else { yield produce(2); }",
            "",
            true,
        ),
        (
            "if flag { let (owner, _) = produce(1); yield (owner, 0); } else { let (owner, _) = produce(2); yield (owner, 0); }",
            "",
            true,
        ),
        (
            "{ yield if flag { yield produce(1); } else { yield produce(2); }; }",
            "",
            true,
        ),
        (
            "loop { if flag { break produce(1); } break produce(2); }",
            "",
            true,
        ),
        (
            "match flag { case true { yield produce(1); } case false { yield produce(2); } }",
            "",
            true,
        ),
        (
            "if flag { yield produce(1); } else { yield (Owner { value = 2 }, 0); }",
            "",
            false,
        ),
        (
            "if flag { yield produce(1); } else { yield produce(2); }",
            "owner.value = 3;",
            false,
        ),
        (
            "if flag { yield produce(1); } else { yield produce(2); }",
            "let forged = Owner { value = owner.value }; observe(&forged);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                mut value: u32;
            }}
            trusted predicate live(owner: &Owner);
            trusted func produce(pos value: u32) -> (Owner, u32)
                ensures(trusted live(owner = &result.0))
            {{
                return (Owner
                {{
                    value = value
                }}, 0);
            }}
            func observe(pos owner: &Owner)
                requires(trusted live(owner = owner))
            {{
            }}
            func caller(pos flag: bool)
            {{
                let (mut owner, _) = {expression};
                {intervening}
                observe(&owner);
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn yielded_pairs_do_not_combine_unrelated_owner_authority() {
    for (alternative, valid) in [
        ("yield produce(2);", true),
        (
            "let (left, _) = produce(2); let (_, right) = produce(3); yield (left, right);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                value: u32;
            }}
            trusted predicate paired(left: &Owner, right: &Owner);
            trusted func produce(pos value: u32) -> (Owner, Owner)
                ensures(trusted paired(left = &result.0, right = &result.1))
            {{
                return (Owner
                {{
                    value = value
                }}, Owner
                {{
                    value = value
                }});
            }}
            func observe(pos left: &Owner, pos right: &Owner)
                requires(trusted paired(left = left, right = right)) {{}}
            func caller(pos flag: bool)
            {{
                let (left, right) = if flag {{ yield produce(1); }} else {{ {alternative} }};
                observe(&left, &right);
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn yielded_generic_ownership_does_not_destroy_the_returned_value() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            mut value: u32;
        }
        trusted predicate live(owner: &Owner);
        func take<T>(pos pending: T?) -> T
        {
            return match consume pending
            {
                case ?value
                {
                    yield value;
                }
                case none
                {
                    panic("empty pending value");
                }
            };
        }
        func observe(pos owner: &Owner)
            requires(trusted live(owner = owner))
        {
        }
        func caller<T>(pos pending: T?, pos owner: &Owner) -> T
            requires(trusted live(owner = owner))
        {
            let value = take<T>(pending);
            observe(owner);
            return value;
        }
    "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn construction_inputs_preserve_authority_without_forgery_or_repairing_pairs() {
    for (left, right, valid) in [
        ("left", "right", true),
        ("Owner { value = left.value }", "right", false),
        ("left", "other_right", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                value: u32;
            }}
            struct Pending
            {{
                left: Owner?;
                right: Owner?;
            }}
            trusted predicate paired(left: &Owner, right: &Owner);
            trusted func produce() -> (Owner, Owner)
                ensures(trusted paired(left = &result.0, right = &result.1))
            {{
                return (Owner
                {{
                    value = 1
                }}, Owner
                {{
                    value = 2
                }});
            }}
            func observe(pos left: &Owner, pos right: &Owner)
                requires(trusted paired(left = left, right = right)) {{}}
            func caller()
            {{
                let (left, right) = produce();
                let (_, other_right) = produce();
                let pending = Pending {{ left = {left}, right = {right} }};
                let held_left = match consume pending.left
                {{
                    case ?value
                    {{
                        yield value;
                    }}
                    case none
                    {{
                        panic("missing left");
                    }}
                }};
                let held_right = match consume pending.right
                {{
                    case ?value
                    {{
                        yield value;
                    }}
                    case none
                    {{
                        panic("missing right");
                    }}
                }};
                observe(&held_left, &held_right);
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}\n{diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
