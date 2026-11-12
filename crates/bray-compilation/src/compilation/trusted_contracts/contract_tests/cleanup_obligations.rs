use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn scoped_exit_must_explicitly_transfer_trusted_authority() {
    for (exit_contract, borrowed, valid) in [
        ("", false, false),
        ("ensures(trusted live(lease.tag))", false, true),
        ("ensures(trusted live(&lease.tag))", true, false),
    ] {
        let observation = if borrowed { "&" } else { "" };

        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Resource
            {{
                tag: bool;
            }}
            struct Lease
            {{
                tag: bool;
            }}
            trusted predicate live(value: {observation}bool);
            impl Resource
            {{
                trusted enter() -> Lease
                    ensures(result.tag == self.tag, trusted live({observation}result.tag))
                    executes(pure)
                {{
                    return
                    {{
                        tag = self.tag
                    }};
                }}
                trusted exit(pos lease: Lease) {exit_contract} executes(pure)
                {{
                }}
            }}
            func observe(pos value: {observation}bool)
                requires(trusted live(value)) executes(pure) {{}}
            func caller(pos resource: &Resource)
            {{
                with lease = resource {{ let copied = lease.tag; _ = copied; }};
                observe({observation}resource.tag);
            }}
            "#,
        ));

        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{exit_contract}: {diagnostics:?}"
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
fn borrowed_completion_preserves_the_reached_owner_for_cleanup() {
    for (body, valid) in [
        ("observe(&owner);", true),
        ("let alias = &owner; observe(alias);", true),
        ("observe(&owner); observe(&owner);", true),
        ("let alias = &owner; observe(alias); observe(&owner);", true),
        ("observe_mut(&mut owner);", true),
        ("panic(\"stopped\");", true),
        ("if owner.epoch == 1 { panic(\"stopped\"); }", true),
        ("while owner.epoch == 1 { panic(\"stopped\"); }", true),
        ("{ let message = \"stopped\"; panic(message); }", true),
        (
            "{ let mut message = \"first\"; message = \"stopped\"; panic(message); }",
            true,
        ),
        (
            "let pending: Cleanup = { tag = 0 }; let mut index: usize = 0; index = 1; index += 1; { let released: Owner = owner; };",
            true,
        ),
        ("observe(&owner); owner.epoch = 2;", false),
        (
            "let alias = &owner; observe(alias); owner.epoch = 2;",
            false,
        ),
        ("forget(&owner);", true),
        ("may_fail(&owner);", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
                trusted destruct() requires(trusted live(&self))
                {{
                }}
            }}
            trusted predicate live(value: &Owner);
            struct Cleanup
            {{
                tag: u8;
                destruct()
                {{
                }}
            }}
            trusted func make() -> Owner
                ensures(trusted live(&result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            trusted func observe(pos value: &Owner)
                requires(trusted live(value)) ensures(trusted live(value)) executes(total) {{}}
            trusted func may_fail(pos value: &Owner)
                requires(trusted live(value)) ensures(trusted live(value))
            {{
                if value.epoch == 2 {{ panic("stopped"); }}
            }}
            trusted func observe_mut(pos value: &mut Owner)
                requires(trusted live(&value)) ensures(trusted live(&value)) executes(total) {{}}
            func forget(pos value: &Owner)
            {{
            }}
            func caller()
            {{
                let mut owner = make();
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
fn cleanup_checks_destructor_against_finalizer_completion() {
    for (guarantee, execution, body, valid) in [
        (
            "ensures(trusted finished(&self))",
            "executes(total)",
            "",
            true,
        ),
        ("ensures(trusted finished(&self))", "", "", true),
        (
            "ensures(trusted finished(&self))",
            "",
            "if self.epoch == 2 { panic(\"stopped\"); }",
            false,
        ),
        ("", "executes(total)", "", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate finished(value: &Owner);

            struct Owner
            {{
                epoch: u64;

                trusted finalize() {execution} {guarantee} {{ {body} }}

                trusted destruct() requires(trusted finished(&self))
                {{
                }}
            }}

            func caller()
            {{
                let value: Owner = {{ epoch = 1 }};
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
fn represented_child_cleanup_checks_its_trusted_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: &Child);
        struct Child
        {
            byte: u8;
            trusted destruct() requires(trusted live(&self))
            {
            }
        }
        struct Parent
        {
            child: Child;
        }
        func caller()
        {
            let parent: Parent =
            {
                child =
                {
                    byte = 1
                }
            };
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn recursive_owned_cleanup_does_not_require_infinite_type_expansion() {
    let compilation = compilation(
        r#"
        module app;
        struct Node
        {
            next: box[Heap] Node;
        }
        func dispose(pos node: Node)
        {
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
fn finalizer_effects_determine_destructor_evidence() {
    for (body, valid) in [("", true), ("self.epoch = 2;", false)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: &Owner);
            struct Owner
            {{
                mut epoch: u64;
                finalize() {{ {body} }}
                trusted destruct() requires(trusted live(&self))
                {{
                }}
            }}
            trusted func owner() -> Owner
                ensures(trusted live(&result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func caller()
            {{
                let value = owner();
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

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
