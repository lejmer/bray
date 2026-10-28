use crate::test_support::{compilation, source_function_body_key};

#[test]
fn borrowed_trait_view_forms_and_dispatches() {
    let compilation = compilation(
        r#"
        module app;

        using sink_value;

        trait Value
        {
            func read() -> i32;
        }

        struct Sink
        {
            marker: i32;
        }

        impl sink_value = Sink(Value)
        {
            func read() -> i32
            {
                return self.marker;
            }
        }

        func main() -> i32
        {
            let sink = Sink
            {
                marker = 1,
            };

            let selected: &view Value = &sink;

            return selected.read();
        }
        "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("borrowed trait-view formation must publish its checked result");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    assert!(checked.value().is_some());
}

#[test]
fn borrowed_trait_view_preserves_mutability_and_reborrows() {
    let compilation = compilation(
        r#"
        module app;

        using cell_counter;

        trait Counter
        {
            func read() -> i32
                executes(pure, total);

            mut func increment();
        }

        struct Cell
        {
            mut value: i32;
        }

        impl cell_counter = Cell(Counter)
        {
            func read() -> i32
                executes(pure, total)
            {
                return self.value;
            }

            mut func increment()
            {
                self.value += 1;
            }
        }

        func read(pos counter: &view Counter) -> i32
        {
            return counter.read();
        }

        func exercise() -> i32
        {
            let mut cell = Cell
            {
                value = 1,
            };

            let counter: &mut view Counter = &mut cell;

            counter.increment();

            let observed: &view Counter = &counter;

            return read(observed);
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "exercise"))
        .expect("borrowed trait-view body must lower");

    assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());
}

#[test]
fn borrowed_trait_view_forms_at_argument_and_return_boundaries() {
    let compilation = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read() -> i32
                executes(pure, total);
        }

        struct Cell
        {
            value: i32;
        }

        impl cell_value = Cell(Value)
        {
            func read() -> i32
                executes(pure, total)
            {
                return self.value;
            }
        }

        func read(pos value: &view Value) -> i32
        {
            return value.read();
        }

        func erase<T>(pos value: &T) -> &view Value
            with(T: Value)
        {
            return &value;
        }

        func exercise() -> i32
        {
            let cell = Cell
            {
                value = 20,
            };

            let selected = erase<Cell>(&cell);

            return read(&cell) + selected.read();
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["erase", "exercise"] {
        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, name))
            .expect("borrowed trait-view body must lower");

        assert!(
            lowered.value().is_some(),
            "{name}: {:?}",
            lowered.diagnostics()
        );
    }
}

#[test]
fn borrowed_trait_view_rejects_missing_and_mismatched_implementations() {
    for (implemented, application, valid) in [
        (true, "Value<i32>", true),
        (true, "Value<bool>", false),
        (false, "Value<i32>", false),
    ] {
        let implementation = if implemented {
            r#"
            impl cell_value = Cell(Value<i32>)
            {
                func read() -> i32
                {
                    return self.value;
                }
            }
            "#
        } else {
            ""
        };

        let participation = if implemented { "using cell_value;" } else { "" };

        let source = format!(
            r#"
            module app;

            {participation}

            trait Value<T>
            {{
                func read() -> T;
            }}

            struct Cell
            {{
                value: i32;
            }}

            {implementation}

            func exercise()
            {{
                let cell = Cell
                {{
                    value = 1,
                }};

                let selected: &view {application} = &cell;
            }}
            "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingNoApplicableCandidate,
            );
        }
    }
}

#[test]
fn borrowed_trait_view_rejects_members_outside_its_surface() {
    for call in [
        "",
        "selected.create()",
        "selected.generic<i32>()",
        "selected.same()",
        "selected.take()",
        "selected.item()",
        "selected.compare(&cell)",
        "selected.mutate()",
        "selected.value",
        "selected.constrained()",
        "let upgraded: &mut view Surface = &mut selected;\n                upgraded.mutate()",
    ] {
        let statement = if call.is_empty() {
            String::new()
        } else {
            format!("{call};")
        };

        let source = format!(
            r#"
            module app;

            using cell_other;
            using cell_surface;

            trait Other
            {{
            }}

            trait Surface
            {{
                func constrained() -> i32
                    with(Self: Other)
                {{
                    return 1;
                }}

                type Item;

                func item() -> Item;

                static func create() -> i32;

                func generic<T>() -> i32;

                func same() -> Self;

                func compare(pos other: &Self) -> bool;

                consume func take();

                mut func mutate();
            }}

            struct Cell
            {{
                mut value: i32;
            }}

            impl cell_surface = Cell(Surface)
            {{
                type Item = i32;

                func item() -> i32
                {{
                    return 1;
                }}

                static func create() -> i32
                {{
                    return 1;
                }}

                func generic<T>() -> i32
                {{
                    return 1;
                }}

                func same() -> Self
                {{
                    return Cell
                    {{
                        value = 1,
                    }};
                }}

                func compare(pos other: &Self) -> bool
                {{
                    return true;
                }}

                consume func take()
                {{
                }}

                mut func mutate()
                {{
                    self.value += 1;
                }}
            }}

            impl cell_other = Cell(Other)
            {{
            }}

            func exercise()
            {{
                let cell = Cell
                {{
                    value = 1,
                }};

                let concrete = cell.constrained();
                let selected: &view Surface = &cell;

                {statement}
            }}
            "#
        );

        let compilation = compilation(&source);

        if call.is_empty() {
            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );
        } else {
            assert!(compilation.check_diagnostics().has_errors(), "{call}");

            assert!(
                compilation
                    .check_diagnostics()
                    .iter()
                    .all(|diagnostic| diagnostic.kind().code().raw() >= 7000),
                "{call}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}

#[test]
fn borrowed_trait_view_checks_cleanup_before_erasing_unused_members() {
    let compilation = compilation(
        r#"
        module app;

        using sink_dispose;

        async func pause()
        {
        }

        struct Owner
        {
            async finalize()
            {
                await pause();
            }
        }

        trait Dispose<T>
        {
            func dispose(pos value: T);
        }

        struct Sink<T>
        {
            marker: i32;
        }

        impl sink_dispose = Sink<T>(Dispose<T>)
        {
            func dispose(pos value: T)
            {
            }
        }

        async func main()
        {
            let sink: Sink<Owner> =
            {
                marker = 0,
            };

            let selected: &view Dispose<Owner> = &sink;
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
    );
}

#[test]
fn borrowed_trait_view_preserves_stored_borrows() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read() -> i32
                executes(pure, total);
        }

        struct Cell
        {
            mut value: i32;
        }

        impl cell_value = Cell(Value)
        {
            func read() -> i32
                executes(pure, total)
            {
                return self.value;
            }
        }

        struct Stored
        {
            selected: &view Value;
        }

        func forward(pos source: &Cell) -> Stored
        {
            return
            {
                selected = &source,
            };
        }

        func exercise() -> i32
        {
            let cell = Cell
            {
                value = 2,
            };

            let stored = forward(&cell);

            return stored.selected.read();
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "exercise"))
        .expect("borrowed trait-view body must lower");

    assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());
}

#[test]
fn borrowed_trait_view_rejects_local_storage_escape() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read() -> i32
                executes(pure, total);
        }

        struct Cell
        {
            mut value: i32;
        }

        impl cell_value = Cell(Value)
        {
            func read() -> i32
                executes(pure, total)
            {
                return self.value;
            }
        }

        func bad() -> &view Value
        {
            let cell = Cell
            {
                value = 1,
            };

            return &cell;
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn borrowed_trait_view_rejects_mutation_while_shared() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read() -> i32
                executes(pure, total);
        }

        struct Cell
        {
            mut value: i32;
        }

        impl cell_value = Cell(Value)
        {
            func read() -> i32
                executes(pure, total)
            {
                return self.value;
            }
        }

        func bad() -> i32
        {
            let mut cell = Cell
            {
                value = 1,
            };

            let selected: &view Value = &cell;

            cell.value = 2;

            return selected.read();
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
fn borrowed_trait_view_lowers_async_methods() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            async func read() -> i32;
        }

        struct Cell
        {
            value: i32;
        }

        impl cell_value = Cell(Value)
        {
            async func read() -> i32
            {
                return self.value;
            }
        }

        async func read(pos selected: &view Value) -> i32
        {
            return await selected.read();
        }

        async func exercise()
        {
            let cell = Cell
            {
                value = 2,
            };

            let selected: &view Value = &cell;
            let result = await selected.read();
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "read"))
        .expect("borrowed trait-view body must lower");

    assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "exercise"))
        .expect("borrowed trait-view body must lower");

    assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());
}

#[test]
fn borrowed_trait_view_preserves_parameter_preconditions() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read(pos value: i32) -> i32
                requires(value > 0)
                executes(pure, total);
        }

        struct Cell
        {
            value: i32;
        }

        impl cell_value = Cell(Value)
        {
            func read(pos value: i32) -> i32
                requires(value > 0)
                executes(pure, total)
            {
                return value;
            }
        }

        func read(pos selected: &view Value) -> i32
        {
            return selected.read(1);
        }

        func exercise() -> i32
        {
            let cell = Cell
            {
                value = 2,
            };

            let selected: &view Value = &cell;

            return read(selected);
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let lowered = application
        .lowered_unit(source_function_body_key(&application, "exercise"))
        .expect("borrowed trait-view body must lower");

    assert!(lowered.value().is_some(), "{:?}", lowered.diagnostics());
}

#[test]
fn borrowed_trait_view_checks_cleanup_of_unused_default_providers() {
    for (argument, valid) in [("i32", true), ("Owner", false)] {
        let application = compilation(&format!(
            r#"
            module app;

            using sink_value;

            async func pause()
            {{
            }}

            struct Owner
            {{
                async finalize()
                {{
                    await pause();
                }}
            }}

            func discard<T>(pos value: T)
            {{
            }}

            func demand<T>() -> i32
            {{
                let handler: func(pos value: T) = discard<T>;

                return 0;
            }}

            trait Value<T>
            {{
                func read(value: i32 = demand<T>()) -> i32;
            }}

            struct Sink<T>
            {{
                marker: i32;
            }}

            impl sink_value = Sink<T>(Value<T>)
            {{
                func read(value: i32 = 0) -> i32
                {{
                    return value;
                }}
            }}

            async func exercise()
            {{
                let sink: Sink<{argument}> =
                {{
                    marker = 1,
                }};

                let selected: &view Value<{argument}> = &sink;
            }}
            "#
        ));

        if valid {
            assert!(
                application.check_diagnostics().is_empty(),
                "{:?}",
                application.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                application.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn borrowed_trait_view_requires_matching_trusted_evidence() {
    for valid in [true, false] {
        let requirement = if valid {
            "requires(trusted live(value))"
        } else {
            ""
        };

        let source = format!(
            r#"
        trusted module app;

        using cell_value;

        trusted predicate live(value: i32);

        trait Value
        {{
            func read(pos value: i32) -> i32
                requires(trusted live(value))
                executes(pure);
        }}

        struct Cell
        {{
            marker: i32;
        }}

        impl cell_value = Cell(Value)
        {{
            func read(pos value: i32) -> i32
                requires(trusted live(value))
                executes(pure)
            {{
                return value;
            }}
        }}

        func forward(pos selected: &view Value, pos value: i32) -> i32
            {requirement}
        {{
            return selected.read(value);
        }}
        "#
        );

        let application = compilation(&source);

        if valid {
            assert!(
                application.check_diagnostics().is_empty(),
                "{:?}",
                application.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                application.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn borrowed_trait_view_forms_inside_contextual_containers() {
    for statement in [
        "let selected: (&view Value)? = &cell;",
        "let selected: [&view Value; 1] = [&cell];",
        "let selected: (&view Value, i32) = (&cell, 1);",
    ] {
        let application = compilation(&format!(
            r#"
            module app;

            using cell_value;

            trait Value
            {{
                func read() -> i32
                    executes(pure, total);
            }}

            struct Cell
            {{
                value: i32;
            }}

            impl cell_value = Cell(Value)
            {{
                func read() -> i32
                    executes(pure, total)
                {{
                    return self.value;
                }}
            }}

            func exercise()
            {{
                let cell = Cell
                {{
                    value = 1,
                }};

                {statement}
            }}
            "#
        ));

        assert!(
            application.check_diagnostics().is_empty(),
            "{statement}: {:?}",
            application.check_diagnostics()
        );

        let lowered = application
            .lowered_unit(source_function_body_key(&application, "exercise"))
            .expect("borrowed trait-view body must lower");

        assert!(
            lowered.value().is_some(),
            "{statement}: {:?}",
            lowered.diagnostics()
        );
    }
}

#[test]
fn borrowed_trait_view_reports_missing_fulfillment() {
    let application = compilation(
        r#"
        module app;

        using cell_value;

        trait Value
        {
            func read() -> i32;
        }

        struct Cell
        {
            value: i32;
        }

        impl cell_value = Cell(Value)
        {
        }

        func exercise()
        {
            let cell = Cell
            {
                value = 1,
            };

            let selected: &view Value = &cell;
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingMissingTraitFulfillment,
    );
}
