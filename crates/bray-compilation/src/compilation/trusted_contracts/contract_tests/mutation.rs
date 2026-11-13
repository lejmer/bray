use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn sibling_mutation_preserves_a_returned_component_witness() {
    for (mutation, valid) in [
        ("", true),
        ("access.payload = 1;", true),
        ("access.lease.active = false;", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Lease);
            struct Lease
            {{
                mut active: bool;
            }}
            struct Access
            {{
                mut payload: u64;
                mut lease: Lease;
            }}
            trusted func acquire() -> Access
                ensures(trusted live(&result.lease))
            {{
                return
                {{
                    payload = 0, lease =
                    {{
                        active = true
                    }}
                }};
            }}
            func observe(pos lease: &Lease)
                requires(trusted live(lease))
                ensures(trusted live(lease)) executes(pure, total) {{}}
            func caller()
            {{
                let mut access = acquire();
                observe(&access.lease);
                {mutation}
                observe(&access.lease);
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
fn extracted_fields_survive_sibling_extraction_and_parent_replacement() {
    for (continuation, valid) in [
        ("let right = trusted take_marker(pair, true); consume_marker(right);", true),
        ("pair = replacement;", true),
        ("trusted change_owner(&mut left);", false),
    ] {
        let source = format!(r#"
            trusted module app;
            using std.memory;

            @layout(c)
            struct Marker
            {{
            }}

            callable Enter = @abi(c) trusted func(pos owner: Owner) -> Owner
                requires(trusted acquired(owner = &owner.marker))
                ensures(trusted acquired(owner = &result.marker));

            @layout(c)
            struct Owner
            {{
                marker: Marker;
                bind: Enter;
            }}

            struct Pair
            {{
                mut left: Owner?;
                mut right: Owner?;
            }}

            trusted predicate acquired(owner: &Marker);

            const func present_owner(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_)
                executes(pure, total)
            {{
                return match owner
                {{
                    case ?value
                    {{
                        yield &value;
                    }}
                    case none
                    {{
                        panic("missing owner");
                    }}
                }};
            }}

            predicate optional_owner(owner: &(Owner?)) = owner matches none || acquired(owner = &present_owner(owner).marker);

            extern trusted func change_owner(pos owner: &mut(Owner?));

            func consume_marker(pos marker: Owner?)
                requires(trusted optional_owner(owner = &marker))
            {{
            }}

            trusted func take_marker(pos pair: &mut Pair, pos right: bool) -> Owner?
                requires(trusted optional_owner(owner = &pair.left), trusted optional_owner(owner = &pair.right))
                ensures(
                    trusted optional_owner(owner = &result),
                    trusted optional_owner(owner = &pair.left),
                    trusted optional_owner(owner = &pair.right)
                )
            {{
                if right
                {{
                    return std.memory.replace_value<(Owner?)>(&mut pair.right, none);
                }}

                return std.memory.replace_value<(Owner?)>(&mut pair.left, none);
            }}

            trusted func caller(pos pair: &mut Pair, pos replacement: Pair)
                requires(trusted optional_owner(owner = &pair.left), trusted optional_owner(owner = &pair.right))
            {{
                let mut left = trusted take_marker(pair, false);
                {continuation}
                consume_marker(left);
            }}
        "#);

        let application = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::baseline(), &source);

        let checked = application.lowered_unit(source_function_body_key(&application, "caller"))
            .expect("extracted field caller reaches lowering");

        assert_eq!(!checked.diagnostics().has_errors(), valid, "{continuation}: {:#?}", checked.diagnostics());

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn replacement_guarantees_follow_the_remaining_partition() {
    for (moved, valid) in [("interference", true), ("producer", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            struct Producer
            {{
                tag: u8;
                trusted destruct() ensures(trusted ready()) executes(total) {{}}
            }}
            struct Interference
            {{
                tag: u8;
                destruct() executes(total) {{}}
            }}
            struct Pair
            {{
                interference: Interference;
                producer: Producer;
            }}
            func observe()
                requires(trusted ready())
                executes(pure, total)
            {{
            }}
            func caller(pos mut value: Pair, pos replacement: Pair)
            {{
                let moved = value.{moved};
                value = replacement;
                observe();
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
fn input_postconditions_retain_the_complete_owner_after_replacement() {
    for (input, replacement, subject) in [
        ("Parent", "replace_child(&mut parent)", "parent.child"),
        ("Child", "replace_root(&mut parent)", "parent"),
    ] {
        for (output, expression, valid) in [
            ("Child", subject.to_owned(), true),
            ("Marker", format!("{subject}.marker"), false),
        ] {
            let source = format!(r#"
                trusted module app;
                struct Marker
                {{
                }}
                struct Child
                {{
                    marker: Marker;
                    number: u32;
                }}
                struct Parent
                {{
                    mut child: Child;
                }}
                trusted predicate held(marker: &Marker);
                trusted func replace_child(pos parent: &mut Parent)
                    ensures(trusted held(marker = &parent.child.marker))
                {{
                    parent.child = Child
                    {{
                        marker = Marker {{}}, number = 0
                    }};
                }}
                extern trusted func replace_root(pos child: &mut Child)
                    ensures(trusted held(marker = &child.marker));
                func caller(pos mut parent: {input}) -> {output}
                {{
                    trusted {replacement};
                    return {expression};
                }}
            "#);

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
                );
            }
        }
    }
}

#[test]
fn field_mutation_preserves_only_independent_carrier_observations() {
    for (ty, result, input, valid) in [
        ("Marker", "&result.marker", "&guard.marker", true),
        ("Guard", "&result", "guard", false),
    ] {
        for mutation in [
            "guard.payload = true;",
            "if flag { guard.payload = true; }",
            "if flag { guard.payload = true; } else { guard.payload = false; }",
        ] {
            let source = format!(
                r#"
            trusted module app;
            struct Marker
            {{
            }}
            struct Guard
            {{
                marker: Marker;
                mut payload: bool;
            }}
            trusted predicate held(value: &{ty});
            trusted func acquire() -> Guard
                ensures(trusted held({result})) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}, payload = false
                }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held({input})) executes(pure, total) {{}}
            func caller(pos flag: bool)
            {{
                let mut guard = acquire();
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
}

#[test]
fn joined_mutated_values_do_not_inherit_their_entry_contents() {
    for (mutation, valid) in [
        ("if flag { value.ready = false; }", false),
        (
            "if flag { value.ready = false; } else { value.ready = false; }",
            false,
        ),
        ("if flag { value.ready = true; }", true),
    ] {
        let source = format!(
            r#"
            module app;
            @copy struct State
            {{
                mut ready: bool;
            }}
            func make() -> State
                ensures(result.ready)
            {{
                return
                {{
                    ready = true
                }};
            }}
            func update(pos flag: bool) -> State
                ensures(result.ready)
            {{
                let mut value = make();
                {mutation}
                return value;
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .execution_properties(source_function_body_key(&application, "update"))
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:?}"
        );
    }
}

#[test]
fn input_referent_mutations_preserve_only_proven_disjoint_component_witnesses() {
    for (mutated, right_type, field, valid) in [
        ("left", "&mut Pair", "first", true),
        ("right", "&mut Pair", "first", true),
        ("left", "&mut Pair", "second", false),
        ("right", "&mut Pair", "second", true),
        ("right", "&mut (&mut Pair)", "first", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Cell
            {{
                mut value: bool;
            }}
            struct Pair
            {{
                mut first: Cell;
                mut second: Cell;
            }}
            trusted predicate held(value: &Cell);
            func observe(pos value: &Cell)
                requires(trusted held(value)) executes(pure, total) {{}}
            func caller(pos left: &mut Pair, pos right: {right_type})
                requires(trusted held(&left.second))
            {{
                {mutated}.{field}.value = false;
                observe(&left.second);
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
