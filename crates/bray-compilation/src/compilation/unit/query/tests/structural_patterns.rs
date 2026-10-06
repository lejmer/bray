use super::support::fixtures::pattern_compilation;
use super::support::representation::assert_type_representation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{PatternOperation, PatternProjection, SelectedOperation, SemanticSelection};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticKind;
use bray_symbols::SymbolOrdinal;

#[test]
fn patterns_report_arms_after_a_catch_all_as_unreachable() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case _\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case true\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive());
    assert_eq!(coverage.unreachable_arms(), &[1]);
}

#[test]
fn patterns_check_fixed_array_shape_before_proving_irrefutability() {
    let valid = pattern_compilation("    let [first, .., last]: [i32; 3] = [1, 2, 3];\n");
    let invalid = pattern_compilation("    let [first]: [i32; 2] = [1, 2];\n");

    let valid_key = source_callable_body_key(&valid);
    let invalid_key = source_callable_body_key(&invalid);

    let valid_patterns = match valid.patterns(valid_key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("valid array-pattern analysis must be available: {error:?}"),
    };

    let invalid_patterns = match invalid.patterns(invalid_key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("invalid array-pattern analysis must be available: {error:?}"),
    };

    assert!(
        valid_patterns.diagnostics().is_empty(),
        "{:?}",
        valid_patterns.diagnostics()
    );

    let [first, last] = valid_patterns.value().binding_types() else {
        panic!("array pattern must publish its two binding types");
    };

    assert_eq!(
        first.projection(),
        Some(PatternProjection::ElementFromStart(SymbolOrdinal::new(0)))
    );

    assert_eq!(
        last.projection(),
        Some(PatternProjection::ElementFromEnd(SymbolOrdinal::new(0)))
    );

    assert!(!valid_patterns.value().is_recovered());

    assert_eq!(
        crate::test_support::diagnostic_kinds(invalid_patterns.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );
}

#[test]
fn patterns_check_product_field_coverage() {
    let valid = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "    y: i32;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let { x, y }: Point = Point { x = 1, y = 2 };\n",
        "}\n",
    ));

    let invalid = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "    y: i32;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let { x }: Point = Point { x = 1, y = 2 };\n",
        "}\n",
    ));

    let valid_key = source_callable_body_key(&valid);
    let invalid_key = source_callable_body_key(&invalid);

    let valid_patterns = match valid.patterns(valid_key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("valid product-pattern analysis must be available: {error:?}"),
    };

    let invalid_patterns = match invalid.patterns(invalid_key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("invalid product-pattern analysis must be available: {error:?}"),
    };

    assert!(
        valid_patterns.diagnostics().is_empty(),
        "{:?}",
        valid_patterns.diagnostics()
    );

    assert_eq!(
        crate::test_support::diagnostic_kinds(invalid_patterns.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );
}

#[test]
fn patterns_check_and_project_generic_product_fields() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Wrapper<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "func main(input: Wrapper<bool>)\n",
        "{\n",
        "    let { value }: Wrapper<bool> = input;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("generic product-pattern analysis must be available: {error:?}"),
    };

    let [binding] = analysis.value().binding_types() else {
        panic!("field shorthand must publish one binding type");
    };

    assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

    assert_eq!(binding.operation(), PatternOperation::Consume);

    assert!(matches!(
        binding.projection(),
        Some(PatternProjection::ProductField(_))
    ));

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn patterns_check_and_project_generic_union_payload_fields() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Maybe<T>\n",
        "{\n",
        "    Some(value: T);\n",
        "    None;\n",
        "}\n",
        "func main(input: Maybe<bool>)\n",
        "{\n",
        "    match input\n",
        "    {\n",
        "        case Some(value = value)\n",
        "        {\n",
        "        }\n",
        "\n",
        "        case .None\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("generic payload-pattern analysis must be available: {error:?}"),
    };

    let [binding] = analysis.value().binding_types() else {
        panic!("payload pattern must publish one binding type");
    };

    assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

    assert!(matches!(
        binding.projection(),
        Some(PatternProjection::ActiveUnionPayloadField { .. })
    ));

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn patterns_bind_positional_generic_union_payload_fields() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Maybe<T>\n",
        "{\n",
        "    Some(pos value: T);\n",
        "    None;\n",
        "}\n",
        "func main(input: Maybe<bool>)\n",
        "{\n",
        "    match input\n",
        "    {\n",
        "        case Some(value)\n",
        "        {\n",
        "            assert(value);\n",
        "        }\n",
        "\n",
        "        case None {}\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => {
            panic!("positional payload-pattern analysis must be available: {error:?}")
        }
    };

    let [binding] = analysis.value().binding_types() else {
        panic!("positional payload pattern must publish one binding type");
    };

    assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

    assert_ne!(binding.operation(), PatternOperation::Recovered);

    assert!(matches!(
        binding.projection(),
        Some(PatternProjection::ActiveUnionPayloadField { .. })
    ));

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn positional_payload_bindings_support_member_calls() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Item\n",
        "{\n",
        "    func value() -> i32\n",
        "    {\n",
        "        return 1;\n",
        "    }\n",
        "}\n",
        "union Choice\n",
        "{\n",
        "    Pair(pos first: Item, pos second: Item);\n",
        "    Empty;\n",
        "}\n",
        "func main(input: Choice)\n",
        "{\n",
        "    match input\n",
        "    {\n",
        "        case Pair(first, second)\n",
        "        {\n",
        "            let value: i32 = first.value();\n",
        "        }\n",
        "\n",
        "        case Empty {}\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("member-call selections must be available: {error:?}"),
    };

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    assert!(selections.value().entries().iter().any(|entry| matches!(
        entry.selection(),
        SemanticSelection::Operation(SelectedOperation::Member(_))
    )));

    assert!(
        selections
            .value()
            .entries()
            .iter()
            .any(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
    );
}

#[test]
fn operation_resolved_match_subjects_support_payload_member_calls() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Item\n",
        "{\n",
        "    func value() -> i32\n",
        "    {\n",
        "        return 1;\n",
        "    }\n",
        "}\n",
        "union Choice\n",
        "{\n",
        "    Present(pos value: Item);\n",
        "    Empty;\n",
        "}\n",
        "func main(input: Result<Choice, bool>) -> Result<unit, bool>\n",
        "{\n",
        "    match try input\n",
        "    {\n",
        "        case Present(value)\n",
        "        {\n",
        "            let number: i32 = value.value();\n",
        "        }\n",
        "\n",
        "        case Empty {}\n",
        "    }\n",
        "\n",
        "    return Ok(unit);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("operation-resolved pattern selections must exist: {error:?}"),
    };

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    assert!(selections.value().entries().iter().any(|entry| matches!(
        entry.selection(),
        SemanticSelection::Operation(SelectedOperation::Member(_))
    )));

    assert!(
        selections
            .value()
            .entries()
            .iter()
            .any(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
    );
}

#[test]
fn patterns_do_not_treat_unknown_named_payload_fields_as_positional() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Maybe<T>\n",
        "{\n",
        "    Some(value: T);\n",
        "}\n",
        "func main(input: Maybe<bool>)\n",
        "{\n",
        "    match input\n",
        "    {\n",
        "        case .Some(other = value)\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("invalid payload-pattern analysis must recover: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );
}

#[test]
fn patterns_check_nested_product_patterns_against_field_types() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "    y: i32;\n",
        "}\n",
        "func main(input: Point)\n",
        "{\n",
        "    let { x = none, y }: Point = input;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("nested product-pattern analysis must be available: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(analysis.diagnostics()),
        [DiagnosticKind::CheckingIncompatiblePattern]
    );
}

#[test]
fn patterns_retain_consuming_match_operations() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match consume value\n",
        "    {\n",
        "        case _\n",
        "        {\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("consuming pattern analysis must be available: {error:?}"),
    };

    assert!(
        analysis
            .value()
            .patterns()
            .iter()
            .all(|pattern| pattern.operation() == PatternOperation::Consume)
    );
}
