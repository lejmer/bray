use super::support::representation::assert_expression_representation;
use crate::test_support::{
    compilation, compilation_with_sources_and_worker_budget, only_call_selection,
    source_callable_body_key, source_function_body_key,
};
use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SemanticSelection};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticType};
use bray_symbols::ConstantValueKind;

#[test]
fn panic_arguments_require_one_message_before_lowering() {
    for (call, count) in [("panic()", 0), ("panic(\"one\", \"two\")", 2)] {
        for modifier in ["", "async "] {
            for statement in [
                format!("{call};"),
                format!("for item in 0..2 {{ {call}; }}"),
            ] {
                let source = format!("module app; {modifier}func main() {{ {statement} }}");
                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::BindingPanicArgumentCountMismatch,
                );

                let diagnostic = diagnostics
                    .iter()
                    .find(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::BindingPanicArgumentCountMismatch
                    })
                    .expect("panic argument count retains its specific cause");

                assert!(
                    diagnostic
                        .args()
                        .contains(&DiagnosticArg::actual_count(count))
                );

                let lowered = compilation
                    .lowered_unit(source_callable_body_key(&compilation))
                    .expect("invalid panic arguments preserve binding diagnostics");

                assert!(lowered.value().is_none(), "{source}");
            }
        }
    }
}

#[test]
fn panic_arguments_reject_names_before_the_mapping_is_erased() {
    for name in ["message", "other"] {
        let source = format!("module app; func main() {{ panic({name} = \"message\"); }}");
        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::BindingNamedPanicArgument,
        );

        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingNamedPanicArgument)
            .expect("panic argument name retains its specific cause");

        assert!(
            diagnostic
                .args()
                .contains(&DiagnosticArg::referenced_name(name))
        );

        let span = diagnostic
            .primary_span()
            .expect("argument name retains its source");

        assert_eq!(
            compilation
                .source(span.source_id())
                .expect("source exists")
                .text_slice(span.range()),
            Some(name)
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .expect("named panic arguments preserve binding diagnostics");

        assert!(lowered.value().is_none());
    }
}

#[test]
fn panic_arguments_accept_one_string_with_an_optional_trailing_comma() {
    for call in [
        "panic(\"message\")",
        "panic(\"message\",)",
        "panic(make_message(text = \"message\"))",
    ] {
        for modifier in ["", "async "] {
            let source = format!(
                "module app; func make_message(text: string) -> string {{ return text; }} \
                 {modifier}func main() {{ for item in 0..2 {{ {call}; }} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "main"))
                .expect("valid panic message lowers");

            let mir = lowered
                .value()
                .as_ref()
                .and_then(bray_lowering::LoweredUnit::mir)
                .expect("valid panic retains executable MIR");

            assert!(mir.is_valid(), "{source}: {mir:?}");
        }
    }
}

#[test]
fn panic_and_assertion_arguments_preserve_required_message_and_condition_types() {
    for call in ["panic(42)", "panic(true)", "assert(42)", "assert(true, 42)"] {
        let source = format!("module app; func main() {{ {call}; }}");
        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingIncompatibleExpressionType,
        );
    }
}

#[test]
fn assertion_arguments_keep_their_condition_and_optional_message_grammar() {
    for call in [
        "assert()",
        "assert(true, \"one\", \"two\")",
        "assert(condition = true)",
    ] {
        let source = format!("module app; func main() {{ {call}; }}");
        let compilation = compilation(&source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::SyntaxExpectedToken,
        );
    }
}

#[test]
fn incompatible_pointer_handle_reports_a_call_error_before_lowering() {
    let compilation = compilation_with_sources_and_worker_budget(
        &[
            r#"trusted module app;

using core.memory;
using internal app.platform;

trusted func main()
{
    let mut transferred: u64 = 99;
    let progress: DataPointer<u64> = core.memory.address_of_mut<u64>(&mut transferred);
    let handle: DataPointer<u8> = core.memory.null<u8>();

    let read: u32 = trusted internal app.platform.handle_read(
        handle,
        destination = handle,
        length = 0,
        transferred = progress,
    );
}
"#,
            r#"trusted internal module app.platform;

trusted internal func handle_read(
    pos handle: u64,
    standard_stream: u32 = 0,
    destination: DataPointer<u8>,
    length: u64,
    transferred: DataPointer<u64>,
) -> u32
{
    return 0;
}
"#,
        ],
        crate::WorkerBudget::serial(),
    );

    let key = source_function_body_key(&compilation, "main");

    let selections = compilation
        .semantic_selections(key.clone())
        .expect("fixture callable must have semantic selections");

    let diagnostic = selections
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
        .next()
        .expect("invalid handle must retain its type diagnostic");

    bray_testing::assert_goal_state_diagnostic(diagnostic);

    assert!(
        diagnostic
            .args()
            .contains(&DiagnosticArg::expected_type(DiagnosticType::U64))
    );

    let span = diagnostic
        .primary_span()
        .expect("type error must retain its argument span");

    let source = compilation
        .source(span.source_id())
        .expect("fixture `source` call must succeed");

    assert_eq!(
        source.text_slice(span.range()).map(str::trim),
        Some("handle")
    );

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    assert!(lowered.value().is_none());
    assert!(lowered.diagnostics().has_errors());
}

#[test]
fn yielded_diverging_calls_preserve_never_at_a_value_join() {
    for expression in [
        "match choice { case true { yield 1; } case false { yield trusted core.target.abort(); } }",
        "if choice { yield 1; } else { yield trusted core.target.abort(); }",
        "loop { if choice { break 1; } break trusted core.target.abort(); }",
    ] {
        let source = format!(
            r#"trusted module app;

using core.target;
trusted func choose(pos choice: bool) -> usize {{ return {expression}; }}"#
        );

        let compilation = compilation(&source);
        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .expect("diverging call must retain its selected signature");

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("diverging join must lower");

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some()
        );
    }
}

#[test]
fn literal_values_are_adapted_once_to_final_types_and_selected_target() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let fixed: i8 = 42;\n",
        "    let target_sized: usize = 42;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    let values = match compilation.literal_values(key.clone()) {
        Ok(values) => values,
        Err(error) => panic!("literal values must publish: {error:?}"),
    };

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(true)
    );

    assert_eq!(values.value().entries().len(), 2);

    assert_eq!(
        values.value().target_integer_width_bits(),
        compilation.selected_target().target().integer_width_bits()
    );

    let semantic_values = match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic values must publish: {error:?}"),
    };

    for entry in values.value().entries() {
        let value = semantic_values.constant_value_data(entry.value());

        assert!(matches!(value.kind(), ConstantValueKind::Integer(_)));
    }

    assert!(values.diagnostics().is_empty());
}

#[test]
fn unrepresentable_literals_publish_recovery_values_and_diagnostics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let value: i8 = 128;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let values = match compilation.literal_values(key) {
        Ok(values) => values,
        Err(error) => panic!("invalid literal values must recover: {error:?}"),
    };

    assert_eq!(
        values
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingConstantLiteralNotRepresentable)
            .count(),
        1
    );

    let [entry] = values.value().entries() else {
        panic!("source must publish one literal value");
    };

    let semantic_values = match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic values must publish: {error:?}"),
    };

    let value = semantic_values.constant_value_data(entry.value());

    assert!(matches!(value.kind(), ConstantValueKind::Error));
}

#[test]
fn expression_typing_and_call_selection_converge_into_cached_results() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = identity(1);\n",
        "}\n",
        "func identity(pos value: i64) -> i64\n",
        "{\n",
        "    return value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key.clone()) {
        Ok(selections) => selections,
        Err(error) => panic!("semantic selections must publish: {error:?}"),
    };

    assert!(
        types.diagnostics().is_empty(),
        "expression typing must be diagnostic-free: {:?}",
        types.diagnostics()
    );

    assert!(
        selections.diagnostics().is_empty(),
        "semantic selection must be diagnostic-free: {:?}",
        selections.diagnostics()
    );

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("direct call must publish a callable selection");
    };

    let [
        SelectedArgument::Explicit {
            expression: argument,
            ..
        },
    ] = call.arguments()
    else {
        panic!("direct call must retain one explicit argument");
    };

    let Some(call_type) = types.value().expression(selection.expression()) else {
        panic!("selected call must have a final type");
    };

    let Some(argument_type) = types.value().expression(*argument) else {
        panic!("selected argument must have a final type");
    };

    assert!(!call_type.is_recovered());
    assert_eq!(argument_type, call_type);

    assert_expression_representation(
        &compilation,
        types.value(),
        selection.expression(),
        RepresentationRole::ScalarI64,
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(true)
    );

    let repeated_types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("repeated expression types must publish: {error:?}"),
    };

    let repeated_selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("repeated semantic selections must publish: {error:?}"),
    };

    let literals = compilation
        .literal_values(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("literal values must publish: {error:?}"));

    assert_eq!(types.snapshot_address(), repeated_types.snapshot_address());

    assert_eq!(
        selections.snapshot_address(),
        repeated_selections.snapshot_address()
    );

    assert_eq!(types.snapshot_address(), selections.snapshot_address());
    assert_eq!(types.snapshot_address(), literals.snapshot_address());
}

#[test]
fn contextual_numeric_operations_do_not_retain_provisional_diagnostics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func padding(pos width: usize, pos length: usize) -> usize\n",
        "{\n",
        "    let remaining: usize = width - length;\n",
        "\n",
        "    return remaining;\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn callable_values_are_classified_from_converged_types() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let operation: func(pos value: i32) -> i32 = identity;\n",
        "    let result = operation(1);\n",
        "}\n",
        "func identity(pos value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("callable-value expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("callable-value selection must publish: {error:?}"),
    };

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("callable value must publish a call selection");
    };

    assert!(matches!(call.target(), BoundCallableTarget::Indirect(_)));

    assert!(!types.value().is_recovered());
}

#[test]
fn direct_lambda_calls_use_nested_callable_types_and_identities() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = lambda(pos value: i32) -> i32\n",
        "    {\n",
        "        return value;\n",
        "    }(1);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("lambda expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("lambda call selection must publish: {error:?}"),
    };

    assert!(!types.value().is_recovered());

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("direct lambda invocation must publish a call selection");
    };

    assert!(matches!(call.target(), BoundCallableTarget::Anonymous(_)));
}

#[test]
fn nullable_destinations_do_not_become_async_initializer_evidence() {
    let compilation = compilation(
        r#"
        module app;

        struct Capture
        {
            value: i32;
        }

        async func body(pos capture: Capture)
        {
        }

        func optional() -> Future<unit>?
        {
            return none;
        }

        func main()
        {
            let pending: Future<unit>? = body(Capture { value = 1 });
            let inferred = body(Capture { value = 2 });
            let lifted: Future<unit>? = inferred;
            let already_nullable: Future<unit>? = optional();
            let aggregate: (i32, i32)? = (3, 4);
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn nullable_initializers_do_not_satisfy_present_destinations() {
    let compilation = compilation(
        r#"
        module app;

        func optional() -> Future<unit>?
        {
            return none;
        }

        func main()
        {
            let required: Future<unit> = optional();
        }
    "#,
    );

    assert!(
        compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
            .next()
            .is_some()
    );
}

#[test]
fn directional_initializers_preserve_contextual_variant_types() {
    let compilation = compilation(
        r#"
        module app;

        union Choice
        {
            Empty;
            Value(value: i32);
        }

        func receive(pos choice: Choice = Empty)
        {
        }

        func main()
        {
            let empty: Choice = Empty;
            let value: Choice = Value(value = 1);
            let dotted: Choice = .Value(value = 2);
            let optional: Choice? = Value(value = 3);

            receive(Value(value = 4));
            receive();
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn byte_extents_report_incorrect_argument_counts() {
    let compilation = compilation(
        r#"
            module app;

            func sample()
            {
                let buffer: bytes<1, 2> = [0; 1];
            }
        "#,
    );

    let key = source_callable_body_key(&compilation);

    let declared = compilation
        .declared_value_type_templates(key.clone())
        .expect("fixture `declared_value_type_templates` call must succeed");

    let diagnostic = declared
        .diagnostics()
        .by_kind(DiagnosticKind::BindingGenericArgumentCountMismatch)
        .next()
        .expect("invalid byte extent arity retains its source diagnostic");

    bray_testing::assert_goal_state_diagnostic(diagnostic);

    assert!(
        diagnostic
            .args()
            .contains(&DiagnosticArg::expected_count(1))
    );

    assert!(diagnostic.args().contains(&DiagnosticArg::actual_count(2)));

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    assert!(lowered.value().is_none());

    assert!(
        lowered
            .diagnostics()
            .by_kind(DiagnosticKind::BindingGenericArgumentCountMismatch)
            .next()
            .is_some()
    );
}

#[test]
fn trait_default_bodies_apply_associated_type_equalities_in_either_query_order() {
    for equality in [
        "Self(Cursor).Element == Element",
        "Self(Operations).Element == Self(Cursor).Element",
        "",
    ] {
        for header_first in [false, true] {
            let constraints = if equality.is_empty() {
                "Self: Cursor".to_owned()
            } else {
                format!("Self: Cursor, {equality}")
            };

            let source = r#"
                module app;

                trait Cursor
                {
                    type Element;
                    mut func next() -> Element?;
                }

                trait Operations
                {
                    type Element;

                    consume mut func first() -> Element?
                        with ($CONSTRAINTS)
                    {
                        let item: Element? = self(Cursor).next();
                        return item;
                    }
                }
            "#
            .replace("$CONSTRAINTS", &constraints);

            let compilation = compilation(&source);

            assert!(
                compilation.syntax_tree_result().diagnostics().is_empty(),
                "{source}\n{:#?}",
                compilation.syntax_tree_result().diagnostics()
            );

            let key =
                crate::test_support::source_trait_callable_member_body_key(&compilation, "first");

            if header_first {
                let symbols = compilation
                    .symbol_graph()
                    .expect("fixture compilation must build its symbol graph");

                let owner = symbols
                    .symbol_for_key(key.declared_owner())
                    .expect("fixture `symbol_for_key` call must succeed");

                let cancellation = crate::fact::CancellationToken::new();

                let binding = compilation
                    .binding_context(&cancellation)
                    .expect("fixture compilation must provide a binding context");

                let constraints = bray_binder::SymbolQueryProvider::resolve_symbol_query(
                    &binding,
                    bray_symbols::SymbolQueryRequest::<bray_symbols::GenericTypeConstraintsQuery>::new(
                        bray_symbols::GenericOwnerId::try_new(owner).expect("fixture generic owner id must be valid"),
                    ),
                ).expect("fixture symbol query must resolve");

                assert!(constraints.diagnostics().is_empty());
            }

            let types = compilation
                .expression_types(key.clone())
                .expect("fixture callable must complete expression typing");

            if equality.is_empty() {
                bray_testing::assert_goal_state_diagnostic_kind(
                    types.diagnostics(),
                    DiagnosticKind::CheckingIncompatibleExpressionType,
                );
            } else {
                assert!(
                    types.diagnostics().is_empty(),
                    "{source}\nheader first: {header_first}\n{:#?}",
                    types.diagnostics()
                );

                assert!(
                    compilation.check_diagnostics().is_empty(),
                    "{:#?}",
                    compilation.check_diagnostics()
                );

                assert!(
                    compilation
                        .lowered_unit(key)
                        .expect("fixture callable must lower successfully")
                        .value()
                        .is_some()
                );
            }
        }
    }
}

#[test]
fn embedded_type_equality_extents_do_not_assume_their_unchecked_header() {
    let compilation = compilation(
        r#"
        module app;

        const func extent() -> usize
        {
            return 1;
        }

        func accept<T>(pos value: T) -> [bool; extent()]
            with ([bool; extent()] == T)
        {
            return value;
        }

        func caller() -> [bool; 1]
        {
            return accept<[bool; 1]>([true]);
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    assert!(
        compilation
            .lowered_unit(source_function_body_key(&compilation, "caller"))
            .expect("fixture callable must lower successfully")
            .value()
            .is_some()
    );
}

#[test]
fn source_predicate_extents_reuse_checked_type_and_trait_headers() {
    for premise in ["T == bool", "T: Copyable"] {
        let source = r#"
            module app;

            const func extent<T>() -> usize
                with ($PREMISE)
            {
                return 1;
            }

            const func allowed<T>() -> bool
            {
                return true;
            }

            func accept<T>()
                with ($PREMISE, allowed<[bool; extent<T>()]>())
            {
            }

            func caller()
            {
                accept<bool>();
            }
        "#
        .replace("$PREMISE", premise);

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}\n{:#?}",
            compilation.check_diagnostics()
        );

        assert!(
            compilation
                .lowered_unit(source_function_body_key(&compilation, "caller"))
                .expect("fixture callable must lower successfully")
                .value()
                .is_some()
        );
    }
}

#[test]
fn inherited_trait_premises_follow_scoped_type_equalities() {
    for (left, valid) in [("bool", true), ("u8", false)] {
        let source = r#"
            module app;

            func constrained<T>(pos value: T)
                with (T: Copyable)
            {
            }

            struct Holder<T, U>
            {
                value: U;
            }

            impl Holder<T, U>
                with (T == T, U: Copyable)
            {
                consume func convert()
                    with (T == U)
                {
                    constrained(self.value);
                }
            }

            func caller()
            {
                let value: Holder<$LEFT, bool> = { value = true };
                value.convert();
            }
        "#
        .replace("$LEFT", left);

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:#?}");

            assert!(
                compilation
                    .lowered_unit(source_function_body_key(&compilation, "caller"))
                    .expect("fixture callable must lower successfully")
                    .value()
                    .is_some()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingNoApplicableCandidate,
            );
        }
    }
}
