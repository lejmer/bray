use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
fn borrowed_completion_keeps_the_selected_static_instance() {
    for (observed, valid) in [("u32", true), ("u64", false)] {
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
                storage: Uninit<u32>;
            }}
            static OWNED<T>: Owner = {{ marker = Marker {{}}, storage = core.memory.uninit<u32>() }};
            trusted predicate held(owner: &Marker);
            trusted func grant(pos owner: &Owner)
                ensures(trusted held(owner = &owner.marker)) executes(pure, total) {{}}
            func observe(pos owner: &Owner)
                requires(trusted held(owner = &owner.marker))
            {{
            }}
            func caller()
            {{
                grant(&OWNED<u32>);
                observe(&OWNED<{observed}>);
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
fn equal_const_results_retain_independent_owner_authority() {
    for (observed, valid) in [("first", true), ("second", false)] {
        let application = compilation(&format!(r#"
            trusted module app;

            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
            }}

            trusted predicate active(marker: &Marker);

            const func create() -> Owner
                executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}

            trusted func activate(pos owner: &Owner)
                ensures(trusted active(&owner.marker))
                executes(pure, total)
            {{}}

            func observe(pos owner: &Owner)
                requires(trusted active(&owner.marker))
            {{}}

            func caller()
            {{
                let first = create();
                let second = create();
                trusted activate(&first);
                observe(&{observed});
            }}
        "#));
        let body = application.lowered_unit(source_function_body_key(&application, "caller"))
            .expect("const-owner fixture lowers");
        assert_eq!(!body.diagnostics().has_errors(), valid, "{observed}: {:?}", body.diagnostics());

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(body.diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn static_initializers_preserve_only_established_owner_authority() {
    for (initializer, mutation, valid) in [
        ("create()", "", true),
        ("{ let prepared = create(); yield prepared; }", "", true),
        ("Owner { marker = Marker {} }", "", false),
        ("create()", "change();", true),
        (
            "create()",
            "let address = core.memory.address_of<Owner>(&OWNER); change();",
            true,
        ),
        (
            "create()",
            "let address = core.memory.address_of<Owner>(&OWNER); publish(address); change();",
            false,
        ),
    ] {
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
            }}
            trusted predicate held(marker: &Marker);
            trusted const func create() -> Owner
                ensures(trusted held(&result.marker)) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            static OWNER: Owner = {initializer};
            func observe(pos owner: &Owner)
                requires(trusted held(&owner.marker))
            {{
            }}
            extern func change();
            extern func publish(pos pointer: DataPointer<Owner>);
            trusted func caller()
                uses(raw_memory)
            {{
                {mutation} observe(&OWNER);
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn static_marker_preservation_keeps_selected_instances_distinct() {
    for (expected, valid) in [(1, true), (2, false)] {
        let application = compilation(&format!(
            r#"
            trusted module app;
            struct Marker
            {{
            }}
            struct Owner
            {{
                marker: Marker;
            }}
            trusted predicate held(marker: &Marker, identity: u32);
            trusted const func create(pos identity: u32) -> Owner
                ensures(trusted held(&result.marker, identity)) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            static OWNER<const N: u32>: Owner = create(N);
            extern func opaque();
            func observe(pos owner: &Owner, pos identity: u32)
                requires(trusted held(&owner.marker, identity)) {{}}
            func caller()
            {{
                observe(&OWNER<1>, 1);
                observe(&OWNER<2>, 2);
                opaque();
                observe(&OWNER<1>, {expected});
                observe(&OWNER<2>, 2);
            }}
        "#
        ));

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        if valid {
            assert!(
                !checked.diagnostics().has_errors(),
                "{:?}",
                checked.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn production_process_context_preserves_static_once_authority() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");
    let mut failures = Vec::new();

    for name in ["argument_context_table", "environment_context_table", "directory_context"] {
        let body = compilation.lowered_unit(source_function_body_key(&compilation, name))
            .expect("production process-context getter lowers");

        if body.diagnostics().has_errors() {
            failures.push((name, body.diagnostics().clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
#[ignore = "tier:proof"]
fn production_host_statics_require_mutation_authority() {
    for operation in [
        "core.atomic.store<u64, 2>(&bray.runtime.bootstrap.host.SYNCHRONIZATION, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.lock, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.key, 0);",
        "let storage = &bray.runtime.bootstrap.host.SYNCHRONIZATION; core.atomic.store<u64, 2>(&storage.main, 0);",
    ] {
        let caller = format!(
            r#"
                module app; using core.atomic; using internal bray.runtime.bootstrap.host; func production_probe()
                {{
                    {operation}
                }}
            "#
        );

        let compilation = production_host_compilation(&caller);

        let key = source_function_body_key(&compilation, "production_probe");
        let source_id = key.source().syntax().source_id();

        let checked = compilation
            .semantic_unit_diagnostics_with_cancellation(key, &compilation.state.cancellation)
            .expect("the production caller retains its semantic diagnostics");

        let diagnostics = checked
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .primary_span()
                    .is_some_and(|span| span.source_id() == source_id)
            })
            .collect::<Vec<_>>();

        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic.kind() != DiagnosticKind::BindingUndeclaredDependency
            }),
            "{diagnostics:?}"
        );

        assert!(
            !diagnostics.is_empty(),
            "safe internal access must not mutate protected storage: {operation}: {:?}",
            diagnostics
        );
    }
}
