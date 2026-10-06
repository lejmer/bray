use super::support::fixtures::pattern_compilation;
use super::support::representation::assert_type_representation;
use super::support::traversal::{first_expression, first_pattern_reference};
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundReferenceTarget, BoundWalkControl, BoundWalkEvent,
    SemanticSelection, walk_bound_unit_view,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticKind;

#[test]
fn late_typed_match_subjects_resolve_contextual_variants() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    match make_choice()\n",
        "    {\n",
        "        case First\n",
        "        {\n",
        "            First;\n",
        "        }\n",
        "    }\n",
        "}\n",
        "func make_choice() -> Choice\n",
        "{\n",
        "    return First;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("late subject pattern must bind: {error:?}"),
    };

    let Some(reference) = first_pattern_reference(bound.value()) else {
        panic!("variant arm body must retain a deferred pattern reference");
    };

    let selections = match compilation.semantic_selections(key.clone()) {
        Ok(selections) => selections,
        Err(error) => panic!("late subject selections must be available: {error:?}"),
    };

    assert_eq!(selections.value().expression(reference), None);

    assert!(
        crate::test_support::diagnostic_kinds(selections.diagnostics())
            .contains(&DiagnosticKind::BindingUnresolvedName)
    );

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("late subject pattern analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");
    assert!(analysis.value().binding_types().is_empty());
    assert!(!analysis.value().is_recovered());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn nested_variant_patterns_resolve_against_the_nested_subject() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Inner\n",
        "{\n",
        "    First;\n",
        "}\n",
        "union Outer\n",
        "{\n",
        "    Wrap(value: Inner);\n",
        "}\n",
        "func main(value: Outer)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case Wrap(value = First)\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("nested pattern analysis must be available: {error:?}"),
    };

    assert!(analysis.value().binding_types().is_empty());
    assert!(!analysis.value().is_recovered());

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn late_typed_bare_bindings_become_visible_after_pattern_resolution() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    match make_value()\n",
        "    {\n",
        "        case captured\n",
        "        {\n",
        "            captured;\n",
        "        }\n",
        "    }\n",
        "}\n",
        "func make_value() -> bool\n",
        "{\n",
        "    return true;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("late binding pattern must bind: {error:?}"),
    };

    let Some(binding) = bound
        .value()
        .local_symbols()
        .bindings()
        .iter()
        .find(|binding| binding.name().as_str() == "captured")
        .map(bray_symbols::LocalBindingSymbol::id)
    else {
        panic!("late binding pattern must retain its candidate identity");
    };

    let Some(reference) = first_pattern_reference(bound.value()) else {
        panic!("arm body must retain a deferred pattern reference");
    };

    let selections = match compilation.semantic_selections(key.clone()) {
        Ok(selections) => selections,
        Err(error) => panic!("late binding selections must be available: {error:?}"),
    };

    assert_eq!(
        selections.value().expression(reference),
        Some(&SemanticSelection::Reference(BoundReferenceTarget::Local(
            binding.into()
        )))
    );

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("late binding pattern analysis must be available: {error:?}"),
    };

    let Some(binding_type) = analysis.value().binding_type(binding) else {
        panic!("resolved binding must publish its checked type");
    };

    assert_type_representation(
        &compilation,
        binding_type.ty(),
        RepresentationRole::ScalarBool,
    );

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn nearer_locals_shadow_contextual_pattern_candidates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    selected;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    match make_choice()\n",
        "    {\n",
        "        case selected\n",
        "        {\n",
        "            let selected: bool = true;\n",
        "            selected;\n",
        "        }\n",
        "    }\n",
        "}\n",
        "func make_choice() -> Choice\n",
        "{\n",
        "    return .selected;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("shadowed contextual pattern must bind: {error:?}"),
    };

    assert_eq!(first_pattern_reference(bound.value()), None);
    assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

    let types = match compilation.expression_types(key) {
        Ok(types) => types,
        Err(error) => panic!("shadowed contextual pattern types must be available: {error:?}"),
    };

    let Some(reference) = first_expression(bound.value(), |expression| {
        matches!(
            expression,
            BoundExpression::Name(name)
                if matches!(name.target(), BoundReferenceTarget::Local(_))
        )
    }) else {
        panic!("shadowing local reference must remain bound");
    };

    let Some(result) = types.value().expression(reference) else {
        panic!("shadowing local reference must have a final type");
    };

    assert_type_representation(&compilation, result.ty(), RepresentationRole::ScalarBool);

    assert!(
        !crate::test_support::diagnostic_kinds(types.diagnostics())
            .contains(&DiagnosticKind::BindingUnresolvedName),
        "{:?}",
        types.diagnostics()
    );
}

#[test]
fn patterns_introduce_bare_bindings_after_pattern_name_resolution_fails() {
    let compilation = pattern_compilation(concat!(
        "    let value: bool = true;\n",
        "    match value\n",
        "    {\n",
        "        case captured\n",
        "        {\n",
        "            captured;\n",
        "        }\n",
        "    }\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("bare binding pattern must bind: {error:?}"),
    };

    let Some(captured) = bound
        .value()
        .local_symbols()
        .bindings()
        .iter()
        .find(|binding| binding.name().as_str() == "captured")
        .map(bray_symbols::LocalBindingSymbol::id)
    else {
        panic!("bare pattern must introduce the captured local");
    };

    assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("bare binding pattern analysis must be available: {error:?}"),
    };

    let Some(binding) = analysis
        .value()
        .binding_types()
        .iter()
        .copied()
        .find(|binding| binding.binding() == captured)
    else {
        panic!("bare binding pattern must publish the captured binding type");
    };

    assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn bare_variants_never_activate_provisional_bindings_for_guards_or_bodies() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case First when First\n",
        "        {\n",
        "            First;\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key) {
        Ok(bound) => bound,
        Err(error) => panic!("bare variant references must bind: {error:?}"),
    };

    assert!(bound.value().local_symbols().bindings().is_empty());

    let mut contextual_variants = 0;

    walk_bound_unit_view(bound.value().view(), bound.value().root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        if matches!(
            bound.value().view().expression(expression),
            Some(BoundExpression::UnqualifiedVariant(_))
        ) {
            contextual_variants += 1;
        }

        BoundWalkControl::Continue
    });

    assert_eq!(contextual_variants, 2);
}

#[test]
fn patterns_do_not_treat_bare_variant_alternatives_as_bindings() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "    Second;\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case First | Second\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("bare variant alternatives must bind: {error:?}"),
    };

    assert!(bound.value().local_symbols().bindings().is_empty());
    assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

    let analysis = match compilation.patterns(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("bare variant alternative analysis must be available: {error:?}"),
    };

    let [coverage] = analysis.value().matches() else {
        panic!("test source must contain one match expression");
    };

    assert!(coverage.is_exhaustive(), "{analysis:?}");

    assert!(
        analysis.diagnostics().is_empty(),
        "{:?}",
        analysis.diagnostics()
    );
}

#[test]
fn pattern_binding_reports_ambiguous_lexical_and_subject_candidates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const First: bool = true;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case First\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key) {
        Ok(bound) => bound,
        Err(error) => panic!("ambiguous pattern must recover: {error:?}"),
    };

    assert_eq!(
        crate::test_support::diagnostic_kinds(bound.diagnostics()),
        [DiagnosticKind::BindingAmbiguousName]
    );

    assert!(bound.value().local_symbols().bindings().is_empty());
}
