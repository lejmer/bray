use bray_diagnostics::DiagnosticKind;

use crate::test_support::compilation;

#[test]
fn synchronous_generic_cleanup_rejects_suspending_instances() {
    for body in [
        "func discard<T>(pos value: T) {}",
        "func discard<T>(pos value: T) { panic(\"stop\"); }",
        "func sink<T>(pos value: T) {} func discard<T>(pos value: T) { sink<T>(value); }",
        "func sink<T>(pos value: T) {} func discard<T>(pos value: T) { let f: func(pos value: T) = sink<T>; f(value); }",
    ] {
        let source = format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner
            {{
                value: i32;
                async finalize() {{ await pause(); }}
            }}
            {body}
            async func main() {{ discard<Owner>(Owner {{ value = 1 }}); }}
            "#
        );

        let application = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
        );
    }
}

#[test]
fn generic_cleanup_preserves_synchronous_and_transferred_owners() {
    for body in [
        "func discard<T>(pos value: T) {} async func main() { discard<i32>(1); }",
        "func transfer<T>(pos value: T) -> T { return value; } async func main() { let value = transfer<Owner>(Owner { value = 1 }); }",
        "async func discard<T>(pos value: T) {} async func main() { await discard<Owner>(Owner { value = 1 }); }",
    ] {
        let source = format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner
            {{
                value: i32;
                async finalize() {{ await pause(); }}
            }}
            {body}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
    }
}

#[test]
fn generic_cleanup_keeps_the_synchronous_callee_boundary() {
    for body in [
        "func sink<T>(pos value: T) {} async func discard<T>(pos value: T) { sink<T>(value); }",
        "func sink<T>(pos value: T, pos stop: bool) { if stop { return; } sink<(T,)>((value,), stop); } async func discard<T>(pos value: T) { sink<T>(value, true); }",
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            {body}
            async func main() {{ await discard<Owner>(Owner {{}}); }}
        "#
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
        );
    }
}

#[test]
fn generic_cleanup_preserves_certified_completion() {
    let application = compilation(
        r#"
        module app;
        struct Owner { async finalize() executes(pure, total) {} }
        func discard<T>(pos value: T) {}
        func main() { discard<Owner>(Owner {}); }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn generic_cleanup_checks_represented_parts() {
    let application = compilation(
        r#"
        module app;
        async func pause() {}
        struct Owner { async finalize() { await pause(); } }
        func discard<T>(pos value: T) {}
        async func main() { discard<(Owner,)>((Owner {},)); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
    );
}

#[test]
fn generic_destructor_cleanup_distinguishes_body_owners_from_receiver_tails() {
    for (body, valid) in [("", true), ("let local = self.value;", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            struct Container<T> {{ value: T; destruct() {{ {body} }} }}
            async func main() {{ let container: Container<Owner> = {{ value = Owner {{}} }}; }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn generic_named_cleanup_defers_unknown_receiver_tails() {
    for (ty, value, valid) in [("i32", "1", true), ("Owner", "Owner {}", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            struct Container<T> {{ value: T; destruct() {{}} }}
            func discard<T>(pos value: Container<T>) {{}}
            async func main() {{ let container: Container<{ty}> = {{ value = {value} }}; discard<{ty}>(container); }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn generic_callable_references_require_explicit_arguments() {
    let application = compilation(
        r#"
        module app;
        func discard<T>(pos value: T) {}
        func main() { let f: func(pos value: i32) = discard; f(1); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::BindingGenericArgumentCountMismatch,
    );
}

#[test]
fn generic_cleanup_survives_deferred_trait_dispatch() {
    let application = compilation(
        r#"
        module app;
        async func pause() {}
        struct Owner { async finalize() { await pause(); } }
        trait Dispose<T> { func dispose(pos value: T); }
        struct Sink<T> { marker: i32; }
        impl SinkDispose = Sink<T>(Dispose<T>) {
            func dispose(pos value: T) {}
        }
        async func forward<S, T>(pos sink: &S, pos value: T) with(S: Dispose<T>) {
            sink.dispose(value);
        }
        async func main() {
            let sink: Sink<Owner> = { marker = 0 };
            await forward<Sink<Owner>, Owner>(&sink, Owner {});
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
    );
}

#[test]
fn generic_cleanup_checks_callable_values_before_their_source_is_erased() {
    for body in [
        "callable Action<T> = func(pos value: T); func discard<T>(pos value: T) {} func form<T>() -> Action<T> { return discard<T>; }",
        "callable Action<T> = func(pos value: T); func form<T>() -> Action<T> { return lambda(pos value: T) {}; }",
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            {body}
            async func main() {{ let selected = form<Owner>(); selected(Owner {{}}); }}
        "#
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
        );
    }
}

#[test]
fn generic_callable_values_preserve_transferred_owners() {
    for body in [
        "callable Action<T> = func(pos value: T) -> T; func transfer<T>(pos value: T) -> T { return value; } func form<T>() -> Action<T> { return transfer<T>; }",
        "callable Action<T> = func(pos value: T) -> T; func form<T>() -> Action<T> { return lambda(pos value: T) -> T { return value; }; }",
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            {body}
            async func main() {{ let selected = form<Owner>(); let retained = selected(Owner {{}}); }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}

#[test]
fn generic_cleanup_resolves_selected_associated_types() {
    for (item, value, valid) in [("i32", "1", true), ("Owner", "Owner {}", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            trait Source {{
                type Item;
                func item() -> Item;
                func discard() {{ let retained = self.item(); }}
            }}
            struct Producer {{ marker: i32; }}
            impl ProducerSource = Producer(Source) {{
                type Item = {item};
                func item() -> {item} {{ return {value}; }}
            }}
            using ProducerSource;
            async func main() {{ let producer = Producer {{ marker = 0 }}; producer.discard(); }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}
