use super::support::selection::selection_rejections;
use crate::test_support::{
    compilation, only_call_selection, source_callable_body_key, source_function_body_key,
    source_trait_callable_fulfillment_body_key,
};
use bray_bound_tree::SemanticSelection;
use bray_diagnostics::{DiagnosticKind, DiagnosticSelectionRejectionReason};
use bray_symbols::{NamedTypeSymbolId, TypeData};

#[test]
fn generic_calls_infer_arguments_from_trait_fulfillment_receivers() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Reader\n",
        "{\n",
        "    mut func read(pos destination: &mut [i32]) -> i32;\n",
        "}\n",
        "struct BufferedReader<Source>\n",
        "{\n",
        "    mut source: Source;\n",
        "}\n",
        "func read_buffered<Source>(\n",
        "    pos reader: &mut BufferedReader<Source>,\n",
        "    pos destination: &mut [i32],\n",
        ") -> i32\n",
        "    with(Source: Reader)\n",
        "{\n",
        "    return 0;\n",
        "}\n",
        "impl BufferedReaderSourceReader = BufferedReader<Source>(Reader)\n",
        "    with(Source: Reader)\n",
        "{\n",
        "    mut func read(pos destination: &mut [i32]) -> i32\n",
        "    {\n",
        "        return read_buffered(&mut self, destination);\n",
        "    }\n",
        "}\n",
    ));

    let key = source_trait_callable_fulfillment_body_key(&compilation, "read");

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("fulfillment selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );
}

#[test]
fn active_generic_constraints_enable_constrained_implementation_candidates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Compares<T> {}\n",
        "trait ItemKey<Key> {}\n",
        "struct Item<Key, Value> {}\n",
        "impl ItemKeyImplementation = Item<Key, Value>(ItemKey<Key>)\n",
        "    with(Key: Compares<Key>)\n",
        "{}\n",
        "func search<Stored, Key>()\n",
        "    with(Stored: ItemKey<Key>)\n",
        "{}\n",
        "func use_search<Key, Value>()\n",
        "    with(Key: Compares<Key>)\n",
        "{\n",
        "    search<Item<Key, Value>, Key>();\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "use_search");

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("constrained generic call must publish: {error:?}"));

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
        .unwrap_or_else(|| panic!("constrained generic call must be selected"));

    assert_eq!(selected.implementation_witnesses().len(), 1);
}

#[test]
fn missing_generic_constraint_evidence_remains_anchored_to_the_call() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Compares<T> {}\n",
        "trait ItemKey<Key> {}\n",
        "struct Item<Key, Value> {}\n",
        "impl ItemKeyImplementation = Item<Key, Value>(ItemKey<Key>)\n",
        "    with(Key: Compares<Key>)\n",
        "{}\n",
        "func search<Stored, Key>()\n",
        "    with(Stored: ItemKey<Key>)\n",
        "{}\n",
        "func use_search<Key, Value>()\n",
        "{\n",
        "    search<Item<Key, Value>, Key>();\n",
        "}\n",
    ));

    let selections = compilation
        .semantic_selections(source_function_body_key(&compilation, "use_search"))
        .unwrap_or_else(|error| panic!("invalid constrained call must recover: {error:?}"));

    let diagnostic = selections
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingNoApplicableCandidate)
        .next()
        .unwrap_or_else(|| panic!("missing evidence must reject the call"));

    assert!(diagnostic.primary_span().is_some(), "{diagnostic:#?}");
}

#[test]
fn explicit_generic_calls_publish_specialized_types_and_selections() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let result = convert<Item, Item>(0);\n",
        "}\n",
        "func convert<T, U>(pos unused: i32) -> T\n",
        "{\n",
        "}\n",
        "struct Item\n",
        "{\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("generic expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("generic semantic selections must publish: {error:?}"),
    };

    assert!(
        types.diagnostics().is_empty(),
        "generic expression typing must be diagnostic-free: {:?}",
        types.diagnostics()
    );

    assert!(!types.value().is_recovered());
    assert!(selections.diagnostics().is_empty());

    let selection = only_call_selection(selections.value());

    assert!(matches!(selection.selection(), SemanticSelection::Call(_)));

    let Some(result) = types.value().expression(selection.expression()) else {
        panic!("generic call must have a final type");
    };

    let values = match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic values must be available: {error:?}"),
    };

    let result = values.type_data(result.ty());

    assert!(matches!(
        result.as_ref(),
        TypeData::Named {
            definition: NamedTypeSymbolId::Struct(_),
            ..
        }
    ));
}

#[test]
fn generic_instance_methods_combine_receiver_and_method_arguments() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Wrapper<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "impl Wrapper<T>\n",
        "{\n",
        "    func choose<U>(pos value: U) -> U\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "}\n",
        "func main(pos wrapper: Wrapper<i32>) -> bool\n",
        "{\n",
        "    let explicit: bool = wrapper.choose<bool>(true);\n",
        "    let inferred: i32 = wrapper.choose(7);\n",
        "\n",
        "    return explicit;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("generic method selections must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "generic method selection must be diagnostic-free: {:?}",
        selections.diagnostics()
    );

    assert_eq!(
        selections
            .value()
            .entries()
            .iter()
            .filter(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
            .count(),
        2
    );
}

#[test]
fn tuple_projections_preserve_receiver_capability_for_method_calls() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Wrapper<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "impl Wrapper<T>\n",
        "{\n",
        "    func choose<U>(pos value: U) -> U\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "}\n",
        "func main(pos values: (Wrapper<i32>, bool)) -> i32\n",
        "{\n",
        "    return values.0.choose(7);\n",
        "}\n",
    ));

    let selections = compilation
        .semantic_selections(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("tuple-projected method selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "tuple-projected method selection must be diagnostic-free: {:?}",
        selections.diagnostics()
    );

    assert!(
        selections
            .value()
            .entries()
            .iter()
            .any(|entry| { matches!(entry.selection(), SemanticSelection::Call(_)) })
    );
}

#[test]
fn generic_instance_method_diagnostics_count_only_method_arguments() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Wrapper<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "impl Wrapper<T>\n",
        "{\n",
        "    func choose<U>(pos value: U) -> U\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "}\n",
        "func main(pos wrapper: Wrapper<i32>)\n",
        "{\n",
        "    let value = wrapper.choose<bool, i32>(true);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("generic method diagnostics must publish: {error:?}"));

    let diagnostic = selections
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
        .next()
        .unwrap_or_else(|| panic!("generic argument count diagnostic must exist"));

    let rejections = selection_rejections(diagnostic);

    assert!(rejections.rejections().iter().any(|rejection| {
        matches!(
            rejection.reason(),
            DiagnosticSelectionRejectionReason::GenericArgumentCount {
                provided: 2,
                maximum: 1,
            }
        )
    }));
}
