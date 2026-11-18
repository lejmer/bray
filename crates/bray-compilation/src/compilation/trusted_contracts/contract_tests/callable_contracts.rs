use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{callable_dependencies};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_host_compilation, production_sync_compilation};

#[test]
fn callable_fields_borrow_their_owning_context_in_contracts() {
    for (predicate, subject) in [
        ("trusted predicate held(owner: &Owner);", "owner"),
        ("trusted predicate held(owner: &Token);", "&owner.token"),
    ] {
        let retirement_subject = if subject == "owner" {
            "&owner"
        } else {
            subject
        };

        for (body, valid) in [
            ("owner.bind(owner);", true),
            ("owner.bind(other);", false),
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Token
                {{
                }}
                {predicate}
                callable Bind = trusted func(pos owner: &Owner)
                    requires(trusted held(owner = {subject}))
                    ensures(trusted held(owner = {subject}));
                callable Retire = trusted func(pos owner: Owner)
                    requires(trusted held(owner = {retirement_subject}));
                struct Owner
                {{
                    token: Token;
                    bind: Bind;
                    retire: Retire;
                }}
                trusted func caller(pos owner: &Owner, pos other: &Owner)
                    requires(trusted held(owner = {subject}))
                    ensures(trusted held(owner = {subject}))
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
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    for (body, valid) in [
        ("let retire = owner.retire; retire(owner);", true),
        ("let retire = owner.retire; retire(other);", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate held(owner: &Owner);
            callable Retire = trusted func(pos owner: Owner)
                requires(trusted held(owner = &owner));
            struct Owner
            {{
                retire: Retire;
            }}
            trusted func caller(pos owner: Owner, pos other: Owner)
                requires(trusted held(owner = &owner))
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
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn invoking_a_callable_does_not_publish_its_code_address() {
    let application = production_sync_compilation(
        r#"
        trusted module app;
        using core.memory;
        using std.memory;
        callable Callback = @abi(c) func();
        trusted func invoke(pos pointer: FunctionPointer<Callback>)
            requires(trusted core.memory.callable_address_valid<Callback>(pointer = pointer))
            uses(layout_reinterpret, foreign_call)
        {
            let operation = trusted std.memory.callable_from_pointer<Callback>(pointer);
            operation();
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "invoke"))
        .expect("callable invocation must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    let callable = crate::test_support::source_function(&application, "invoke");
    let template = callable_dependencies(&application, callable.into());

    assert!(
        !template.effects().iter().any(|effect| matches!(
            effect,
            bray_symbols::DependencyStorageEffect::Escape
                | bray_symbols::DependencyStorageEffect::Direct {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
                | bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
        )),
        "capture-free invocation must not publish its code address: {template:#?}"
    );

    assert!(
        !template.preserves_storage_on_normal_return(),
        "foreign invocation still has opaque effects"
    );
}

#[test]
fn callable_fields_do_not_expose_their_containing_owner() {
    for abi in ["", "@abi(c)"] {
        for (operation, valid) in [("touch(&owner);", true), ("mutate(&mut owner);", false)] {
            let application = compilation(&format!(
                r#"
                trusted module app;
                struct Marker
                {{
                }}
                trusted predicate live(marker: &Marker);
                callable Callback = {abi} func();
                struct Owner
                {{
                    marker: Marker;
                    callback: Callback;
                    trusted destruct() requires(trusted live(&self.marker))
                    {{
                    }}
                }}
                {abi} func inert()
                {{
                }}
                trusted func make() -> Owner
                    ensures(trusted live(&result.marker))
                {{
                    return
                    {{
                        marker = Marker {{}}, callback = inert
                    }};
                }}
                extern func publish(pos callback: Callback);
                extern func mutate(pos owner: &mut Owner);
                func touch(pos owner: &Owner)
                {{
                    publish(owner.callback);
                }}
                func caller()
                {{
                    let mut owner = make();
                    {operation}
                }}
            "#
            ));

            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{abi} {operation}: {diagnostics:?}"
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
fn conditional_callable_types_preserve_trusted_guarantees() {
    let compilation = compilation(
        r#"
        trusted module app;
        struct Owner
        {
            byte: u8;
        }
        trusted predicate live(value: &Owner);
        callable Producer = trusted func(pos ready: bool) -> Owner
            when(ready) { ensures(trusted live(&result)) };
        trusted func owner(pos ready: bool) -> Owner
            when(ready)
        {
            ensures(trusted live(&result))
        }
        {
            return
            {
                byte = 1
            };
        }
        func observe(pos value: &Owner)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let produce: Producer = owner;
            let value = produce(true);
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
fn callable_conversion_cannot_erase_predicate_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        func use_owner(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let erased: func(pos value: u64) = use_owner;
            erased(1);
        }
    "#,
    );

    assert!(compilation.check_diagnostics().has_errors());
}

#[test]
fn callable_conversion_preserves_trusted_predicate_authority() {
    let compilation = compilation(
        r#"
        trusted module app;
        trusted predicate live(value: u64);
        callable Ordinary = func(pos value: u64) requires(live(value));
        func obligated(pos value: u64)
            requires(trusted live(value))
        {
        }
        func caller()
        {
            let erased: Ordinary = obligated;
        }
    "#,
    );

    assert!(compilation.check_diagnostics().has_errors());
}

#[test]
fn named_callable_contracts_preserve_and_enforce_obligations() {
    for (enclosing, valid) in [("", false), ("requires(trusted live(value))", true)] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            callable Action = func(pos value: u64) requires(trusted live(value));
            func observe(pos value: u64)
                requires(trusted live(value))
            {{
            }}
            func caller(pos action: Action, pos value: u64) {enclosing}
            {{
                action(value);
            }}
            func convert() -> Action
            {{
                return observe;
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
fn a_trust_boundary_does_not_hide_wrapper_requirements() {
    for tail in ["", "observe(&value);"] {
        let source = format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
            }}
            trusted predicate live(value: &Owner);
            func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}
            func caller()
            {{
                let value: Owner = {{ epoch = 1 }};
                trusted {{ observe(&value); observe(&value); }};
                {tail}
            }}
        "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        if tail.is_empty() {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");

            let diagnostic = diagnostics.iter().next().expect("the unacknowledged call fails");

            assert_eq!(diagnostic.kind(), DiagnosticKind::CheckingTrustedObligationNotProven);

            assert!(diagnostic.primary_span().is_some_and(|span| {
                crate::test_support::diagnostic_at_call(diagnostic, span.source_id(), &source, "observe(&value)")
            }), "{diagnostic:?}");
        }
    }
}

#[test]
fn anonymous_calls_preserve_trusted_requirements() {
    for (requirement, valid) in [("", false), ("requires(trusted live(value))", true)] {
        let source = format!(
            r#"
            trusted module app;

            trusted predicate live(value: u64);

            func caller(pos value: u64) {requirement}
            {{
                let action = lambda (pos value: u64)
                    requires(trusted live(value))
                {{}};

                action(value);
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
fn anonymous_bodies_receive_their_declared_trusted_requirements() {
    let compilation = compilation(
        r#"
        trusted module app;

        trusted predicate live(value: u64);

        trusted func observe(pos value: u64)
            requires(trusted live(value))
        {
        }

        func caller(pos entry: u64)
            requires(trusted live(entry))
        {
            let action = lambda (pos value: u64) requires(trusted live(value))
            {
                observe(value);
            };

            action(entry);
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
fn anonymous_contracts_resolve_qualified_constants() {
    let application = compilation(
        r#"
        module app;
        const SUCCESS: u32 = 0;
        func run()
        {
            let inspect = lambda(pos status: u32)
                requires(status == app.SUCCESS)
                executes(pure, total)
            {};
            inspect(0);
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn default_mutation_retires_authority_before_the_selected_call_entry() {
    for (default, valid) in [("0", true), ("invalidate(value)", false)] {
        let source = r#"
            trusted module app;

            struct Owner
            {
                mut epoch: u8;
            }
            trusted predicate live(value: &u8);

            trusted func acquire() -> Owner
                ensures(trusted live(&result.epoch))
            {
                return Owner
                {
                    epoch = 0
                };
            }

            func invalidate(pos value: &mut Owner) -> u8
            {
                value.epoch = 1;
                return 0;
            }

            func selected(pos value: &mut Owner, marker: u8 = DEFAULT)
                requires(trusted live(&value.epoch))
            {
            }

            func caller()
            {
                let mut owner = acquire();
                selected(&mut owner);
            }
        "#
        .replace("DEFAULT", default);

        let checked = compilation(&source);
        let diagnostics = checked.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn callback_reconstruction_requires_the_actual_callable_address_evidence() {
    for (requirement, valid) in [
        (
            "requires(trusted core.memory.callable_address_valid<Callback>(pointer = pointer))",
            true,
        ),
        ("", false),
    ] {
        let source = format!(
            r#"trusted internal module bray.runtime.bootstrap.host;

using core.memory;
using std.memory;

callable Callback = @abi(c) func() -> u32;

trusted func production_probe(pos pointer: FunctionPointer<Callback>) -> Callback
    {requirement}
    executes(pure, total)
    uses(layout_reinterpret)
{{
    return std.memory.callable_from_pointer<Callback>(pointer);
}}
"#
        );

        let compilation = production_host_compilation(&source);
        let key = source_function_body_key(&compilation, "production_probe");

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("callback reconstruction must retain its checked result");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{requirement}: {checked:#?}"
        );

        if valid {
            let properties = compilation
                .execution_properties(key)
                .expect("valid callback projection must certify its guarantees");

            assert!(!properties.diagnostics().has_errors(), "{properties:#?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}
