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

#[test]
fn recursive_generic_destructor_cleanup_reaches_a_finite_summary() {
    for destructor in ["", "let local = self.value;"] {
        for (ty, value, valid) in [("i32", "1", true), ("Owner", "Owner {}", false)] {
            let application = compilation(&format!(
                r#"
                module app;
                async func pause() {{}}
                struct Owner {{ async finalize() {{ await pause(); }} }}
                struct Container<T> {{ value: T; destruct() {{ {destructor} }} }}
                func sink<T>(pos value: T, pos stop: bool) {{
                    if stop {{ return; }}
                    let wrapped: Container<T> = {{ value = value }};
                    let outer: Container<Container<T>> = {{ value = wrapped }};
                    forward<Container<Container<T>>>(outer, stop);
                }}
                func forward<T>(pos value: T, pos stop: bool) {{ sink<T>(value, stop); }}
                async func main() {{ sink<{ty}>({value}, true); }}
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
}

#[test]
fn recursive_generic_cleanup_preserves_certified_finalizers() {
    for (finalizer, valid) in [
        ("async finalize() executes(pure, total) {}", true),
        ("async finalize() { await pause(); }", false),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Container<T> {{ value: T; {finalizer} }}
            func sink<T>(pos value: T, pos stop: bool) {{
                if stop {{ return; }}
                let wrapped: Container<T> = {{ value = value }};
                sink<Container<T>>(wrapped, stop);
            }}
            func main() {{ sink<i32>(1, true); }}
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
fn recursive_cleanup_normalizes_closed_constant_expressions() {
    for (ty, expression, argument) in [
        ("i32", "N + 0", "0"),
        ("usize", "N + 0", "0"),
        ("bool", "N && true", "true"),
    ] {
        let application = compilation(&format!(
            r#"
            module app;
            func recur<T, const N: {ty}>(pos value: T, pos stop: bool) {{
                if stop {{ return; }}
                recur<T, {expression}>(value, stop);
            }}
            func main() {{ recur<i32, {argument}>(1, true); }}
            "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(diagnostics.is_empty(), "{ty}: {diagnostics:?}");
    }
}

#[test]
fn recursive_generic_cleanup_retains_constant_completion_arguments() {
    for (ready, valid) in [("true", true), ("false", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            struct Container<T, const Ready: bool> {{
                value: T;
                async finalize() when(Ready) {{ executes(pure, total) }} {{}}
            }}
            func sink<T, const Ready: bool>(pos value: T, pos stop: bool) {{
                if stop {{ return; }}
                let wrapped: Container<T, Ready && true> = {{ value = value }};
                sink<Container<T, Ready && true>, Ready && true>(wrapped, stop);
            }}
            func main() {{ sink<i32, {ready}>(1, true); }}
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
fn generic_inactive_future_cleanup_checks_remaining_captures() {
    for (ty, value, valid) in [("i32", "1", true), ("Owner", "Owner {}", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            async func hold<T>(pos value: T) {{}}
            func discard<T>(pos value: T) {{ let pending = hold<T>(value); }}
            async func main() {{ discard<{ty}>({value}); }}
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
fn recursive_cleanup_distinguishes_preserved_constant_contexts() {
    let application = compilation(
        r#"
        module app;
        struct Container<T, const Mode: usize> {
            value: T;
            async finalize() when(Mode > 0) { executes(pure, total) } {}
        }
        func sink<T, const Mode: usize>(pos value: T, pos stop: bool) {
            if stop { return; }
            let wrapped: Container<T, Mode> = { value = value };
            sink<Container<T, Mode>, Mode>(wrapped, stop);
        }
        func main() { sink<i32, 1>(1, true); sink<i32, 2>(2, true); }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn certified_completion_omits_finalizer_body_cleanup_obligations() {
    let application = compilation(
        r#"
        module app;
        async func pause() {}
        struct Owner { async finalize() { await pause(); } }
        trait Source<T> { func make() -> T; }
        struct Producer {}
        impl ProducerSource = Producer(Source<Owner>) {
            func make() -> Owner { return Owner {}; }
        }
        using ProducerSource;
        func sink<T>(pos value: T) {}
        struct Container<S, T, const Ready: bool> with(S: Source<T>) {
            source: S;
            async finalize() when(Ready) { executes(pure, total) } {
                if !Ready { sink<T>(self.source.make()); }
            }
        }
        async func main() {
            let value: Container<Producer, Owner, true> = { source = Producer {} };
        }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn recursive_cleanup_reports_the_configured_analysis_limit() {
    for maximum in [
        32,
        crate::SemanticAnalysisLimits::default().recursion_depth(),
    ] {
        let application = crate::test_support::compilation_with_options(
            r#"
        module app;
        struct Container<T, const N: usize> { value: T; destruct() {} }
        func sink<T, const N: usize>(pos value: T, pos stop: bool) {
            if stop { return; }
            let wrapped: Container<T, N> = { value = value };
            sink<Container<T, N>, N + 1>(wrapped, stop);
        }
        func main() { sink<i32, 0>(1, true); }
    "#,
            crate::CompilationOptions::default().with_semantic_analysis_limits(
                crate::SemanticAnalysisLimits::new(maximum, 100_000),
            ),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingCleanupRecursionLimitExceeded,
        );
    }
}

#[test]
fn recursive_runtime_defaults_keep_cleanup_summaries_finite() {
    let application = compilation(
        r#"
        module app;
        struct Marker<T> { marker: i32; destruct() {} }
        func make<T>() -> Marker<T> { return { marker = 0 }; }
        func check<T>(pos value: T) -> bool { return false; }
        func sink<T>(pos stop: bool,
            flag: bool = stop || check<Marker<T>>(make<T>()) || sink<Marker<T>>(stop)
        ) -> bool { return true; }
        func main() { let result = sink<i32>(true); }
        "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn generic_cleanup_checks_each_runtime_default_provider() {
    for (body, uses_default) in [
        ("select<T>();", true),
        ("let constructed = Constructed<T>();", true),
        ("let holder: Holder<T> = { marker = 0 };", true),
        ("let choice: Choice<T> = Value();", true),
        ("select<T>(value = 0);", false),
        ("let constructed = Constructed<T>(value = 0);", false),
        ("let holder: Holder<T> = { marker = 0, value = 0 };", false),
        ("let choice: Choice<T> = Value(value = 0);", false),
    ] {
        for ty in ["i32", "Owner"] {
            let valid = ty == "i32" || !uses_default;

            let application = compilation(&format!(
                r#"
                module app;
                async func pause() {{}}
                struct Owner {{ async finalize() {{ await pause(); }} }}
                func discard<T>(pos value: T) {{}}
                func demand<T>() -> i32 {{ let handler: func(pos value: T) = discard<T>; return 0; }}
                func select<T>(value: i32 = demand<T>()) {{}}
                struct Holder<T> {{ marker: i32; value: i32 = demand<T>(); }}
                union Choice<T> {{ Value(value: i32 = demand<T>()); }}
                struct Constructed<T> {{ marker: i32; construct(value: i32 = demand<T>()) -> Self {{ return {{ marker = value }}; }} }}
                func invoke<T>() {{ {body} }}
                async func main() {{ invoke<{ty}>(); }}
                "#
            ));

            let diagnostics = application.check_diagnostics();

            if valid {
                assert!(diagnostics.is_empty(), "{body}: {diagnostics:?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
                );
            }
        }
    }
}

#[test]
fn runtime_defaults_resolve_selected_associated_cleanup_types() {
    for (item, valid) in [("i32", true), ("Owner", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            async func pause() {{}}
            struct Owner {{ async finalize() {{ await pause(); }} }}
            func discard<T>(pos value: T) {{}}
            func demand<T>() -> i32 {{ let handler: func(pos value: T) = discard<T>; return 0; }}
            trait Source {{
                type Item;
                func select(value: i32 = demand<Item>()) {{}} func outer() {{ self.select(); }}
            }}
            struct Producer {{ marker: i32; }}
            impl ProducerSource = Producer(Source) {{ type Item = {item}; }}
            using ProducerSource;
            async func main() {{ let producer = Producer {{ marker = 0 }}; producer.outer(); }}
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
