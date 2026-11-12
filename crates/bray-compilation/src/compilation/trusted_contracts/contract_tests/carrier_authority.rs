use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn immutable_guard_fields_keep_acquisition_permits_paired() {
    for (name, body, expected) in [
        (
            "acquired",
            "let guard = acquire(&first); observe(&guard);",
            None,
        ),
        (
            "fabricated",
            "let guard: Guard = { owner = &first, permit = Permit {} }; observe(&guard);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "detached",
            "let original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "replaced source",
            "let mut original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; original = acquire(&first); observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "whole move",
            "let original = acquire(&first); let guard = original; observe(&guard);",
            None,
        ),
        (
            "fresh detached",
            "let guard: Guard = { owner = &second, permit = acquire(&first).permit }; observe(&guard);",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
        (
            "fabricated alongside live",
            "let original = acquire(&first); let guard: Guard = { owner = &second, permit = Permit {} }; observe(&guard); observe(&original);",
            Some(DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "restore immutable field",
            "let mut original = acquire(&first); let guard: Guard = { owner = &second, permit = original.permit }; original.permit = Permit {}; observe(&guard);",
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
        (
            "consume pattern",
            "let original = acquire(&first); match consume original { case { owner, permit } { let guard: Guard = { owner = &second, permit = permit }; observe(&guard); } }",
            Some(DiagnosticKind::CheckingIncompleteLifecycleStorage),
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Lock
            {{
                tag: u32;
            }}
            struct Permit
            {{
            }}
            trusted predicate held(permit: &Permit);
            struct Guard
            {{
                owner: &Lock;
                permit: Permit;
                destruct() requires(trusted held(&self.permit)) executes(pure, total)
                {{
                }}
            }}
            trusted func acquire(pos owner: &Lock) -> Guard
                ensures(trusted held(&result.permit))
            {{
                return
                {{
                    owner = owner, permit = Permit {{}}
                }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.permit)) executes(pure, total) {{}}
            func caller()
            {{
                let first: Lock = {{ tag = 1 }};
                let second: Lock = {{ tag = 2 }};
                {body}
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            expected.is_none(),
            "{name}: {source}: {diagnostics:#?}"
        );

        if let Some(expected) = expected {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
        }
    }
}

#[test]
fn acquisition_authority_belongs_to_the_complete_produced_carrier() {
    for (name, body, valid) in [
        (
            "whole fresh",
            "let guard = acquire(&first); observe(&guard);",
            true,
        ),
        (
            "whole moved",
            "let original = acquire(&first); let guard = original; observe(&guard);",
            true,
        ),
        (
            "fresh extracted",
            "let marker = acquire(&first).marker;",
            false,
        ),
        (
            "named extracted",
            "let guard = acquire(&first); let marker = guard.marker;",
            false,
        ),
        (
            "fresh reassembled",
            "let guard: Guard = { owner = &second, marker = acquire(&first).marker }; observe(&guard);",
            false,
        ),
        (
            "named reassembled",
            "let original = acquire(&first); let guard: Guard = { owner = &second, marker = original.marker }; observe(&guard);",
            false,
        ),
        (
            "fresh destructured",
            "let { owner, marker } = acquire(&first);",
            false,
        ),
        (
            "extract before divergence",
            "let marker = acquire(&first).marker; loop {}",
            false,
        ),
        (
            "fabricated",
            "let guard: Guard = { owner = &first, marker = Marker {} }; observe(&guard);",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Lock
            {{
                tag: u32;
            }}
            struct Marker
            {{
            }}
            struct Guard
            {{
                owner: &Lock;
                marker: Marker;
            }}
            trusted predicate held(marker: &Marker);
            trusted func acquire(pos owner: &Lock) -> Guard
                ensures(trusted held(&result.marker)) executes(pure, total)
            {{
                return
                {{
                    owner = owner, marker = Marker {{}}
                }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.marker)) executes(pure, total) {{}}
            func caller()
            {{
                let first: Lock = {{ tag = 1 }};
                let second: Lock = {{ tag = 2 }};
                {body}
            }}
        "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{name}: {source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                if name == "fabricated" {
                    DiagnosticKind::CheckingTrustedObligationNotProven
                } else {
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven
                },
            );
        }
    }
}

#[test]
fn independent_tuple_carriers_remain_whole_after_destructuring() {
    for (body, valid) in [
        (
            "let (first, second) = pair(); observe(&first); observe(&second);",
            true,
        ),
        ("let first = pair().0; observe(&first);", true),
        (
            "let first = { let owners = pair(); yield owners.0; }; observe(&first);",
            true,
        ),
        ("let marker = pair().0.marker;", false),
        ("let original = make_stamp(); let copied = original;", false),
        ("let original = make_stamp(); let copied = (0, original);", false),
        ("let original = make_stamp(); discard_stamp(original);", false),
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
            }}
            @copy struct Stamp
            {{
                epoch: u32;
            }}
            trusted predicate held(marker: &Marker);
            trusted predicate stamped(owner: &Stamp);
            trusted func make_stamp() -> Stamp
                ensures(trusted stamped(&result)) executes(pure, total)
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func discard_stamp(pos value: Stamp)
            {{
            }}
            trusted func pair() -> (Guard, Guard)
                ensures(trusted held(&result.0.marker), trusted held(&result.1.marker))
                executes(pure, total)
            {{
                return (
                    {{
                        marker = Marker {{}}
                    }},
                    {{
                        marker = Marker {{}}
                    }}
                );
            }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.marker)) executes(pure, total) {{}}
            func caller()
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn input_authority_uses_the_same_complete_carrier_as_the_producer() {
    for (input, subject) in [("Guard", "guard"), ("Envelope", "guard.inner")] {
        for (result, body, valid) in [
            ("Guard", "return guard;", true),
            ("Marker", "return guard.marker;", false),
            (
                "Guard",
                "return { owner = other, marker = guard.marker };",
                false,
            ),
        ] {
            let body = body.replace("guard", subject);

            let borrowed_subject = subject.replace("guard", "whole");

            let source = format!(
                r#"
                trusted module app;
                struct Lock
                {{
                    tag: u32;
                }}
                struct Marker
                {{
                }}
                struct Guard
                {{
                    owner: &Lock;
                    marker: Marker;
                }}
                struct Envelope
                {{
                    inner: Guard;
                    label: u32;
                }}
                trusted predicate held(marker: &Marker);
                func preserve(pos whole: &{input})
                    requires(trusted held(&{borrowed_subject}.marker))
                    ensures(trusted held(&{borrowed_subject}.marker))
                    executes(pure, total) {{}}
                func transfer(pos guard: {input}, pos other: &Lock) -> {result}
                    requires(trusted held(&{subject}.marker)) {{ preserve(&guard); {body} }}
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
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
                );
            }
        }
    }
}

#[test]
fn forwarded_nested_carrier_keeps_its_original_authority_boundary() {
    for (body, valid) in [
        (
            "let wrapper = wrap(); let guard = wrapper.guard; observe(&guard);",
            true,
        ),
        ("let marker = wrap().guard.marker;", false),
        (
            "let wrapper: Wrapper = { guard = Guard { marker = Marker {} } }; observe(&wrapper.guard);",
            false,
        ),
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
            }}
            struct Wrapper
            {{
                guard: Guard;
            }}
            trusted predicate held(marker: &Marker);
            trusted func acquire() -> Guard
                ensures(trusted held(&result.marker)) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            func wrap() -> Wrapper
                ensures(trusted held(&result.guard.marker)) executes(pure, total)
            {{
                return
                {{
                    guard = acquire()
                }};
            }}
            func observe(pos guard: &Guard)
                requires(trusted held(&guard.marker)) executes(pure, total) {{}}
            func caller()
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
