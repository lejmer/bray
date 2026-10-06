use super::support::representation::assert_expression_representation;
use crate::test_support::{
    compilation, only_call_selection, source_callable_body_key, source_function_body_key,
};
use bray_bound_tree::{SelectedArgument, SemanticSelection};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind,
    DiagnosticRelatedLocationKind, DiagnosticSelectionCandidateIdentity,
    DiagnosticSelectionCandidateSignature, DiagnosticType,
};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn overload_narrowing_provides_context_before_literal_defaults() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = choose(true, 1);\n",
        "}\n",
        "func choose_wide(pos key: bool, pos value: i64) -> i64\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "func choose_default(pos key: i32, pos value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "overload choose = {choose_wide, choose_default}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("semantic selections must publish: {error:?}"),
    };

    assert!(
        types.diagnostics().is_empty(),
        "overload typing must be diagnostic-free: {:?}",
        types.diagnostics()
    );

    assert!(
        selections.diagnostics().is_empty(),
        "overload selection must be diagnostic-free: {:?}",
        selections.diagnostics()
    );

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("overload call must publish a callable selection");
    };

    let [_, SelectedArgument::Explicit { expression, .. }] = call.arguments() else {
        panic!("overload call must retain both explicit arguments");
    };

    assert_expression_representation(
        &compilation,
        types.value(),
        *expression,
        RepresentationRole::ScalarI64,
    );
}

#[test]
fn type_owned_overloads_resolve_named_constructors_and_methods() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Buffer\n",
        "{\n",
        "    value: i32;\n",
        "    internal construct empty() -> Self\n",
        "    {\n",
        "        return {value = 0};\n",
        "    }\n",
        "    internal construct with_value(pos value: i32) -> Self\n",
        "    {\n",
        "        return {value = value};\n",
        "    }\n",
        "    overload new = {empty, with_value}\n",
        "    internal func read_bool(pos marker: bool) -> i32\n",
        "    {\n",
        "        return self.value;\n",
        "    }\n",
        "    internal func read_i32(pos marker: i32) -> i32\n",
        "    {\n",
        "        return self.value + marker;\n",
        "    }\n",
        "    overload read = {read_bool, read_i32}\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let shared: Buffer = Buffer.new();\n",
        "    let _: i32 = shared.read(true);\n",
        "    let buffer: Buffer = Buffer.new(42);\n",
        "    let _: i32 = buffer.read(1);\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "main");

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("type-owned overload selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "type-owned overloads must be diagnostic-free: {:?}",
        selections.diagnostics()
    );
}

#[test]
fn equally_applicable_source_overloads_report_every_candidate() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = choose(1);\n",
        "}\n",
        "func first(pos value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "func second(pos value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "overload choose = {first, second}\n",
    ));

    let selections = compilation
        .semantic_selections(source_function_body_key(&compilation, "main"))
        .unwrap_or_else(|error| panic!("ambiguous selection must recover: {error:?}"));

    let diagnostics = selections.diagnostics();
    let mut ambiguous = diagnostics.by_kind(DiagnosticKind::CheckingAmbiguousCandidate);

    let Some(diagnostic) = ambiguous.next() else {
        panic!("one ambiguous selection expected: {diagnostics:?}");
    };

    assert!(ambiguous.next().is_none(), "{diagnostics:?}");

    let Some(DiagnosticArgValue::SelectionCandidates(candidates)) = diagnostic
        .args()
        .iter()
        .find(|argument| argument.name() == DiagnosticArgName::SelectionCandidates)
        .map(DiagnosticArg::value)
    else {
        panic!("ambiguous selection must retain its candidates: {diagnostic:?}");
    };

    assert_eq!(candidates.omitted_count(), 0);
    assert_eq!(candidates.candidates().len(), 2);

    let names = candidates
        .candidates()
        .iter()
        .map(|candidate| match candidate.identity() {
            DiagnosticSelectionCandidateIdentity::NamedDeclaration { name, .. } => name.as_str(),
            other => panic!("source function candidate must retain its name: {other:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(names, ["first", "second"]);

    for candidate in candidates.candidates() {
        assert_eq!(
            candidate.signature(),
            &DiagnosticSelectionCandidateSignature::Callable {
                parameter_types: Box::new([DiagnosticType::I32]),
                result_type: DiagnosticType::I32,
            }
        );
    }

    assert_eq!(diagnostic.related_locations().len(), 2);

    assert!(
        diagnostic.related_locations().iter().all(|location| {
            location.kind() == DiagnosticRelatedLocationKind::SelectionCandidate
        })
    );

    assert!(
        diagnostic
            .related_locations()
            .windows(2)
            .all(|pair| pair[0].span() < pair[1].span())
    );

    assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingAmbiguousCandidate);
}

#[test]
fn named_arguments_share_selection_mapping_with_type_inference() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = select(second = 1, first = true);\n",
        "}\n",
        "func select(pos first: bool, pos second: i64) -> i64\n",
        "{\n",
        "    return second;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("named-argument expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("named-argument selection must publish: {error:?}"),
    };

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("named call must publish a callable selection");
    };

    let [
        SelectedArgument::Explicit {
            expression: second,
            ordinal: 1,
            ..
        },
        SelectedArgument::Explicit { ordinal: 0, .. },
    ] = call.arguments()
    else {
        panic!("named call must preserve source order and declaration ordinals");
    };

    assert_expression_representation(
        &compilation,
        types.value(),
        *second,
        RepresentationRole::ScalarI64,
    );
}
