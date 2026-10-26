use crate::test_support::compilation;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn terminal_destructor_rejects_installation_of_graceful_owner() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            mut child: Child;

            destruct()
            {
                self.child = Child
                {
                };
            }
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingDestructorOwnershipIntroduction,
    );
}

#[test]
fn terminal_destructor_rejects_helper_installation_before_abnormal_exit() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        func replace(pos value: &mut Child, pos fail: bool)
        {
            value = Child
            {
            };

            if fail
            {
                assert(false);
            }
        }

        struct Owner
        {
            mut child: Child;
            fail: bool;

            destruct()
            {
                replace(&mut self.child, self.fail);
            }
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingDestructorOwnershipIntroduction,
    );
}

#[test]
fn terminal_destructor_allows_scalar_teardown_through_helper() {
    let compilation = compilation(
        r#"
        module app;

        func clear(pos value: &mut i32)
        {
            value = 0;
        }

        struct Owner
        {
            mut value: i32;

            finalize() {}

            destruct()
            {
                clear(&mut self.value);
            }
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn terminal_destructor_allows_destruction_only_replacement() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            destruct() {}
        }

        struct Owner
        {
            mut child: Child;

            destruct()
            {
                self.child = Child
                {
                };
            }
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn terminal_destructor_rejects_new_currently_completed_owner() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize()
                executes(pure, total) {}
        }

        struct Owner
        {
            mut child: Child;

            destruct()
            {
                self.child = Child
                {
                };
            }
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingDestructorOwnershipIntroduction,
    );
}

#[test]
fn terminal_destructor_distinguishes_graceful_dependencies_from_external_ownership() {
    for (finalizer, accepted) in [("", true), ("finalize() {}", false)] {
        let compilation = compilation(&format!(
            r#"
            module app;

            struct Child
            {{
                finalize() {{}}
            }}

            struct Owner
            {{
                child: &mut Child;

                {finalizer}

                destruct()
                {{
                    self.child = Child {{}};
                }}
            }}
            "#
        ));

        let diagnostics = compilation.check_diagnostics();

        if accepted {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingDestructorOwnershipIntroduction,
            );
        }
    }
}

#[test]
fn terminal_destructor_keeps_ordinary_local_cleanup() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            destruct()
            {
                let local = Child
                {
                };
            }
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn terminal_destructor_retains_introduction_even_if_helper_consumes_it_later() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        func replace_then_clear(pos value: &mut Child?)
        {
            value = Child
            {
            };

            value = none;
        }

        struct Owner
        {
            mut child: Child?;

            destruct()
            {
                replace_then_clear(&mut self.child);
            }
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingDestructorOwnershipIntroduction,
    );
}

#[test]
fn terminal_destructor_checks_branch_aliases() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            mut left: Child;
            mut right: Child;
            choose: bool;

            destruct()
            {
                let selected: &mut Child = if self.choose
                {
                    yield &mut self.left;
                }
                else
                {
                    yield &mut self.right;
                };

                selected = Child
                {
                };
            }
        }
        "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(
        diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind(),
            DiagnosticKind::CheckingDestructorOwnershipIntroduction
                | DiagnosticKind::CheckingUnprovedDestructorEffects
        )),
        "{diagnostics:?}"
    );
}

#[test]
fn terminal_destructor_keeps_generic_inherited_tail_and_consumed_local_distinct() {
    for body in ["", "let local: T = self.value;"] {
        let compilation = compilation(&format!(
            r#"
            module app;

            struct Owner<T>
            {{
                value: T;

                destruct()
                {{
                    {body}
                }}
            }}
            "#
        ));

        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{body}: {diagnostics:?}");
    }
}

#[test]
fn terminal_destructor_can_clear_an_inherited_nullable_owner() {
    for body in ["self.child = none;", "clear(&mut self.child);"] {
        let compilation = compilation(&format!(
            r#"
            module app;

            struct Child
            {{
                finalize() {{}}
            }}

            func clear(pos child: &mut Child?)
            {{
                child = none;
            }}

            struct Owner
            {{
                mut child: Child?;

                destruct()
                {{
                    {body}
                }}
            }}
            "#
        ));

        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{body}: {diagnostics:?}");
    }
}

#[test]
fn terminal_destructor_checks_local_cleanup_that_retains_a_receiver_alias() {
    let compilation = compilation(
        r#"
        module app;

        struct Child
        {
            finalize() {}
        }

        struct Later
        {
            child: &mut Child;

            finalize()
            {
                self.child = Child
                {
                };
            }
        }

        struct Owner
        {
            mut child: Child;

            destruct()
            {
                let later = Later
                {
                    child = &mut self.child
                };
            }
        }
        "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(
        diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind(),
            DiagnosticKind::CheckingDestructorOwnershipIntroduction
                | DiagnosticKind::CheckingUnprovedDestructorEffects
        )),
        "{diagnostics:?}"
    );
}

#[test]
fn terminal_destructor_keeps_builtin_operations_and_explicit_construction_effects() {
    let compilation = compilation(
        r#"
        module app;

        struct Snapshot
        {
            value: u32;
        }

        func snapshot(pos value: &Owner) -> Snapshot
        {
            return Snapshot
            {
                value = value.value >> 1
            };
        }

        struct Owner
        {
            mut value: u32;
            ticket: u32?;

            finalize() {}

            destruct()
            {
                _ = self.ticket.is_present();
                _ = self.ticket.is_absent();
                let previous = snapshot(&self);

                self.value = previous.value;
            }
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}
