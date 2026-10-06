use crate::test_support::{compilation, source_function_body_key};
use bray_bound_tree::{BoundCallableTarget, SemanticSelection};
use bray_diagnostics::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn trait_default_bodies_dispatch_calls_through_the_implementing_witness() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Counter\n",
        "{\n",
        "    func count() -> i32;\n",
        "    func doubled() -> i32\n",
        "    {\n",
        "        return self.count() + self.count();\n",
        "    }\n",
        "}\n",
    ));

    let key = crate::test_support::source_trait_callable_member_body_key(&compilation, "doubled");

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("trait default selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    let calls = selections
        .value()
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Call(call) if call.resolution().trait_dispatch().is_some()
            )
        })
        .count();

    assert_eq!(calls, 2);
}

#[test]
fn omitted_trait_members_select_the_trait_owned_default_body() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Counter\n",
        "{\n",
        "    func count() -> i32;\n",
        "    func doubled() -> i32\n",
        "    {\n",
        "        return self.count() + self.count();\n",
        "    }\n",
        "}\n",
        "struct Value\n",
        "{\n",
        "    count: i32;\n",
        "}\n",
        "impl ValueCounter = Value(Counter)\n",
        "{\n",
        "    func count() -> i32\n",
        "    {\n",
        "        return self.count;\n",
        "    }\n",
        "}\n",
        "using ValueCounter;\n",
        "func read(pos value: &Value) -> i32\n",
        "{\n",
        "    return value.doubled();\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "read");

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("trait default call selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    let selected = selections
        .value()
        .entries()
        .iter()
        .find_map(|entry| match entry.selection() {
            SemanticSelection::Call(call) => Some(call.resolution()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("trait default call must be selected"));

    assert!(matches!(
        selected.target(),
        BoundCallableTarget::Declaration(instance)
            if matches!(
                instance.definition().symbol(),
                bray_symbols::AnySymbolId::TraitCallableMember(_)
            )
    ));

    assert_eq!(selected.implementation_witnesses().len(), 1);

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("trait default call must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
fn selected_trait_defaults_normalize_associated_types_from_the_witness() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Sequence\n",
        "{\n",
        "    type Element;\n",
        "    func compare(pos value: &Element) -> bool;\n",
        "    func contains(pos value: &Element) -> bool\n",
        "    {\n",
        "        return self.compare(value);\n",
        "    }\n",
        "}\n",
        "struct Numbers {}\n",
        "impl NumbersSequence = Numbers(Sequence)\n",
        "{\n",
        "    type Element = i32;\n",
        "    func compare(pos value: &i32) -> bool\n",
        "    {\n",
        "        return value == 1;\n",
        "    }\n",
        "}\n",
        "using NumbersSequence;\n",
        "func read(pos numbers: &Numbers, pos value: &i32) -> bool\n",
        "{\n",
        "    return numbers.contains(value);\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "read");

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("trait default selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "associated types in the selected default signature must resolve: {:?}",
        selections.diagnostics()
    );
}

#[test]
fn participating_trait_methods_bind_direct_generic_arguments() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Mapper\n",
        "{\n",
        "    func default_map<T>(pos value: T) -> T\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "    func explicit_map<T>(pos value: T) -> T;\n",
        "}\n",
        "struct Value {}\n",
        "impl ValueMapper = Value(Mapper)\n",
        "{\n",
        "    func explicit_map<T>(pos value: T) -> T\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "}\n",
        "using ValueMapper;\n",
        "func read(pos value: &Value) -> i32\n",
        "{\n",
        "    return value.default_map<i32>(1) + value.explicit_map<i32>(2);\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "read");

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("generic trait calls must select: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("generic trait calls must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
fn participating_trait_methods_report_each_ambiguous_implementation() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait First\n",
        "{\n",
        "    func inspect() -> i32\n",
        "    {\n",
        "        return 1;\n",
        "    }\n",
        "}\n",
        "trait Second\n",
        "{\n",
        "    func inspect() -> i32\n",
        "    {\n",
        "        return 2;\n",
        "    }\n",
        "}\n",
        "struct Value {}\n",
        "impl ValueFirst = Value(First) {}\n",
        "impl ValueSecond = Value(Second) {}\n",
        "func read(pos value: &Value) -> i32\n",
        "{\n",
        "    return value.inspect();\n",
        "}\n",
    ));

    let selections = compilation
        .semantic_selections(source_function_body_key(&compilation, "read"))
        .unwrap_or_else(|error| panic!("ambiguous trait method must recover: {error:?}"));

    let diagnostics = selections.diagnostics();
    let mut ambiguous = diagnostics.by_kind(DiagnosticKind::CheckingAmbiguousCandidate);

    let Some(diagnostic) = ambiguous.next() else {
        panic!("one ambiguous trait method diagnostic expected: {diagnostics:?}");
    };

    assert!(ambiguous.next().is_none(), "{diagnostics:?}");

    let Some(DiagnosticArgValue::SelectionCandidates(candidates)) = diagnostic
        .args()
        .iter()
        .find(|argument| argument.name() == DiagnosticArgName::SelectionCandidates)
        .map(DiagnosticArg::value)
    else {
        panic!("ambiguous trait method must retain its implementations: {diagnostic:?}");
    };

    assert_eq!(candidates.omitted_count(), 0);
    assert_eq!(candidates.candidates().len(), 2);
    assert_eq!(diagnostic.related_locations().len(), 2);

    assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingAmbiguousCandidate);
}
