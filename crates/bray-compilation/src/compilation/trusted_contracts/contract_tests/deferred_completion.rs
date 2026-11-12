use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn deferred_producers_require_live_entry_evidence_when_their_body_begins() {
    for (intervening, valid) in [
        ("", true),
        ("unchanged();", true),
        ("if flag { unchanged(); }", true),
        ("opaque();", false),
        ("if flag { opaque(); }", false),
        ("epoch = 2;", true),
        ("epoch = 2; _ = await owner(epoch);", false),
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;

            struct Owner
            {{
                epoch: u64;
            }}

            trusted predicate admitted(epoch: u64);
            trusted predicate live(value: &Owner);

            trusted async func owner(pos epoch: u64) -> Owner
                requires(trusted admitted(epoch))
                ensures(trusted live(&result))
            {{
                return {{ epoch = epoch }};
            }}

            func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}

            func unchanged()
            {{
            }}

            extern func opaque();

            async func caller(pos mut epoch: u64, flag: bool)
                requires(trusted admitted(epoch))
            {{
                let future = owner(epoch);
                {intervening}
                let value = await future;
                observe(&value);
            }}
        "#
        ));

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{intervening}: {:?}",
            compilation.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn deferred_producers_publish_witnesses_after_await() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner { epoch: u64; }
        trusted predicate live(value: &Owner);
        trusted async func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
        trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
        async func caller() { let future = owner(); let value = await future; observe(&value); }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn deferred_body_effects_apply_when_driven_and_harmless_await_preserves_evidence() {
    for (body, observation, valid) in [
        ("", "await future; observe(&value);", true),
        ("opaque();", "observe(&value); await future;", true),
        ("opaque();", "await future; observe(&value);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;
            struct Owner {{ epoch: core.atomic.Atomic<u64>; }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = core.atomic.initialize<u64>(1) }}; }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            extern func opaque();
            async func work() {{ {body} }}
            async func caller() {{
                let value = owner();
                let future = work();
                {observation}
            }}
        "#
        );

        let compilation = compilation(&source);

        assert_eq!(
            !compilation.check_diagnostics().has_errors(),
            valid,
            "{source}\n{:?}",
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
fn deferred_completion_guarantees_do_not_escape_a_caught_panic() {
    for (body, valid) in [
        ("await produce(&value);", true),
        ("_ = catch { await produce(&value); };", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner {{ tag: bool; }}
            trusted predicate live(value: &Owner);
            extern func opaque();
            trusted async func produce(pos value: &Owner) ensures(trusted live(value)) {{ opaque(); }}
            func observe(pos value: &Owner) requires(trusted live(value)) {{}}
            async func caller() {{
                let value = Owner {{ tag = true }};
                {body}
                observe(&value);
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
fn failing_cleanup_does_not_publish_normal_completion_guarantees_to_a_catch() {
    for (body, valid) in [
        ("{ let value = Producer {}; };", true),
        ("_ = catch { let value = Producer {}; };", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            extern func opaque();
            struct Producer {{ trusted destruct() ensures(trusted ready()) {{ opaque(); }} }}
            func observe() requires(trusted ready()) {{}}
            func caller() {{ {body} observe(); }}
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
fn failing_scoped_exit_does_not_publish_guarantees_to_a_catch() {
    for (body, valid) in [
        ("with lease = resource {};", true),
        ("_ = catch { with lease = resource {}; };", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate ready();
            extern func opaque();
            struct Resource {{}}
            struct Lease {{}}
            impl Resource {{
                enter() -> Lease {{ return Lease {{}}; }}
                trusted exit(pos lease: Lease) ensures(trusted ready()) {{ opaque(); }}
            }}
            func observe() requires(trusted ready()) {{}}
            func caller(pos resource: &Resource) {{ {body} observe(); }}
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
