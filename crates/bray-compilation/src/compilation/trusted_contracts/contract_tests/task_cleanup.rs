use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn task_cleanup_revokes_preceding_guarantees() {
    for (pending, valid) in [("", true), ("let pending = work().start();", false)] {
        let source = format!(
            r#"
            trusted module app;

            using core.atomic;
            trusted predicate ready(state: &core.atomic.Atomic<u64>);
            async func work()
            {{
            }}
            trusted func establish(pos state: &core.atomic.Atomic<u64>)
                ensures(trusted ready(state))
                executes(total)
            {{
            }}
            func observe(pos state: &core.atomic.Atomic<u64>)
                requires(trusted ready(state))
                executes(pure, total)
            {{
            }}
            async func caller(pos state: &core.atomic.Atomic<u64>)
            {{
                {{
                    {pending} establish(state);
                }} observe(state);
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
fn loop_cleanup_keeps_independent_complete_carriers() {
    for operation in [
        "opaque();",
        "_ = take<T>(&mut pending);",
        "await suspend();",
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker
            {{
            }}
            trusted predicate live(marker: &Marker);
            struct Owner
            {{
                instance: Marker;
            }}
            struct Anchor
            {{
            }}
            extern trusted func borrow(pos anchor: &Anchor) -> &Owner
                ensures(trusted live(&result.instance))
                executes(pure, total);
            struct Pending<T>
            {{
                mut value: T?;
            }}
            struct Guard
            {{
                instance: Marker;
                owner: &Owner;
                trusted destruct() requires(trusted live(&self.instance))
                {{
                }}
            }}
            trusted func make() -> Owner
                ensures(trusted live(&result.instance))
            {{
                return
                {{
                    instance = Marker {{}}
                }};
            }}
            trusted func lock(pos owner: &Owner) -> Guard
                requires(trusted live(&owner.instance))
                ensures(trusted live(&owner.instance), trusted live(&result.instance))
            {{
                return
                {{
                    instance = Marker {{}}, owner = owner
                }};
            }}
            struct Registration
            {{
                instance: Marker;
                owner: &Owner;
                mut active: bool;
                trusted destruct() requires(!self.active || (trusted live(&self.instance) && trusted live(&self.owner.instance)))
                    when(!self.active) {{ executes(pure, total) }} {{ if self.active {{ opaque(); }} }}
                consume mut func complete()
                {{
                    self.active = false;
                }}
            }}
            trusted func register(pos owner: &Owner) -> Registration
                requires(trusted live(&owner.instance))
                ensures(trusted live(&result.instance), trusted live(&result.owner.instance))
            {{
                return
                {{
                    instance = Marker {{}}, owner = owner, active = true
                }};
            }}
            extern trusted func stop() -> bool;
            extern trusted func opaque();
            extern trusted func value<T>() -> T;
            extern trusted func take<T>(pos pending: &mut Pending<T>) -> T;
            extern trusted func get<T>(pos guard: &mut Guard) -> &mut T
                requires(trusted live(&guard.instance)) ensures(trusted live(&guard.instance)) executes(pure, total);
            extern trusted async func suspend();
            async func caller<T>(pos anchor: &Anchor, pos mut pending: Pending<T>)
            {{
                let owner = borrow(anchor);
                let mut registration = register(owner);
                while true
                {{
                    let generation: u32 =
                    {{
                        let mut guard = lock(owner);
                        let payload: &mut T = get<T>(&mut guard);
                        if stop()
                        {{
                            registration.complete();
                            return unit;
                        }}
                        {operation}
                        yield 0;
                    }};
                    await suspend();
                }}
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        assert!(
            !checked.diagnostics().has_errors(),
            "{operation}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
fn opaque_drivers_retain_the_borrows_held_by_deferred_values() {
    for (parameter, initialize, argument) in [
        ("Future<unit>", "work(&mut owner)", "pending"),
        ("&mut Future<unit>", "work(&mut owner)", "&mut pending"),
        (
            "&mut (Future<unit>, i32)",
            "(work(&mut owner), 0)",
            "&mut pending",
        ),
        ("Task<unit>", "work(&mut owner).start()", "pending"),
        (
            "&mut Task<unit>",
            "work(&mut owner).start()",
            "&mut pending",
        ),
    ] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker
            {{
            }}
            struct Owner
            {{
                mut marker: Marker;
            }}
            trusted predicate live(marker: &Marker);
            trusted func make() -> Owner
                ensures(trusted live(&result.marker))
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            extern func replace(pos owner: &mut Owner);
            async func work(pos owner: &mut Owner)
            {{
                replace(owner);
            }}
            extern func drive(pos pending: {parameter});
            func observe(pos owner: &Owner)
                requires(trusted live(&owner.marker))
            {{
            }}
            async func caller()
            {{
                let mut owner = make();
                {{
                    let mut pending = {initialize};
                    drive({argument});
                }}
                observe(&owner);
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType)
                .next()
                .is_none(),
            "{parameter}: {diagnostics:?}"
        );

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingCannotInferExpressionType)
                .next()
                .is_none(),
            "{parameter}: {diagnostics:?}"
        );

        assert!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven)
                .next()
                .is_some(),
            "{parameter}: {diagnostics:?}"
        );
    }
}

#[test]
fn task_observation_preserves_the_shared_owner_marker() {
    let application = compilation(
        r#"
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

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return
            {
                marker = Marker {},
            };
        }

        extern async func suspend();

        async func work(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            await suspend();
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
        }

        async func caller()
        {
            let owner = make();
            let task = work(&owner).start();

            try await task.join();
            observe(&owner);
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn task_cleanup_retains_loans_until_the_task_is_consumed() {
    for (body, valid) in [
        (
            "let task = work(&value).start(); opaque(); value = 1;",
            false,
        ),
        ("_ = work(&value).start(); opaque(); value = 1;", false),
        (
            "let task = work(&value).start(); let moved = task; opaque(); value = 1;",
            false,
        ),
        (
            "let task = work(&value).start(); try await task.join(); value = 1;",
            true,
        ),
        (
            "{ let task = work(&value).start(); opaque(); } value = 1;",
            true,
        ),
        (
            "let task = work(&value).start(); try await task.cancel(); value = 1;",
            true,
        ),
        (
            "let task = work(&value).start(); if flag { try await task.join(); } value = 1;",
            false,
        ),
        (
            "let task = work(&value).start(); if flag { try await task.join(); } else { try await task.cancel(); } value = 1;",
            true,
        ),
        (
            "let owner = { let task = work(&value).start(); yield Holder { task = task }; }; opaque(); value = 1;",
            false,
        ),
        (
            "{ let owner = Holder { task = work(&value).start() }; opaque(); } value = 1;",
            true,
        ),
        (
            "let mut task = work(&value).start(); task = work(&value).start(); value = 1;",
            false,
        ),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            extern async func work(pos value: &u32);
            extern func opaque();
            struct Holder
            {{
                task: Task<unit>;
            }}
            async func caller(pos flag: bool)
            {{
                let mut value: u32 = 0;
                {body}
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}
