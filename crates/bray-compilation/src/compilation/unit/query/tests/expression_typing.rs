use super::support::representation::assert_expression_representation;
use crate::test_support::{compilation, only_call_selection, source_callable_body_key};
use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SemanticSelection};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticKind;
use bray_symbols::ConstantValueKind;

#[test]
fn yielded_diverging_calls_preserve_never_at_a_value_join() {
    for expression in [
        "match choice { case true { yield 1; } case false { yield trusted core.target.abort(); } }",
        "if choice { yield 1; } else { yield trusted core.target.abort(); }",
        "loop { if choice { break 1; } break trusted core.target.abort(); }",
    ] {
        let source = format!("trusted module app;\ntrusted func choose(pos choice: bool) -> usize {{ return {expression}; }}");
        let compilation = compilation(&source);
        let key = source_callable_body_key(&compilation);
        let selections = compilation.semantic_selections(key.clone()).expect("diverging call must retain its selected signature");
        assert!(selections.diagnostics().is_empty(), "{:?}", selections.diagnostics());
        let lowered = compilation.lowered_unit(key).expect("diverging join must lower");
        assert!(lowered.diagnostics().is_empty(), "{:?}", lowered.diagnostics());
        assert!(lowered.value().as_ref().and_then(|unit|unit.mir()).is_some());
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
