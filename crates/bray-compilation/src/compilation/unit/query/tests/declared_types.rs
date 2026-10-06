use crate::Compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKind, DeclaredValueTypeConstraintKind,
    DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_symbols::{SymbolKind, TypeExpressionTemplate};
use std::sync::Arc;

#[test]
fn declared_value_type_templates_publish_lazy_source_evidence_and_constraints() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func identity<const count: usize>(value: [i32; count]) -> [i32; count]\n",
        "{\n",
        "    let local: [i32; count] = value;\n",
        "    const copy: [i32; count] = local;\n",
        "    let callable = lambda(item: [i32; count]) -> [i32; count]\n",
        "    {\n",
        "        item\n",
        "    };\n",
        "    copy\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation
            .state
            .declared_value_type_templates
            .is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.checked_control_flow.is_published(&key),
        Ok(false)
    );

    let analysis = match compilation.declared_value_type_templates(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("declared value types must publish: {error:?}"),
    };

    assert!(has_value_kind(
        analysis.value(),
        SymbolKind::GenericConstParameter
    ));

    assert!(has_value_kind(
        analysis.value(),
        SymbolKind::CallableParameter
    ));

    assert!(has_value_kind(analysis.value(), SymbolKind::LocalConstant));

    assert!(
        analysis
            .value()
            .evidence()
            .iter()
            .any(|entry| matches!(entry.term(), DeclaredValueTypeTerm::Pattern(_)))
    );

    assert!(matches!(
        analysis.value().callable_result(),
        Some(TypeExpressionTemplate::Array { .. })
    ));

    assert!(has_constraint_kind(
        analysis.value(),
        DeclaredValueTypeConstraintKind::Initializer
    ));

    assert!(has_constraint_kind(
        analysis.value(),
        DeclaredValueTypeConstraintKind::PatternBinding
    ));

    assert!(has_constraint_kind(
        analysis.value(),
        DeclaredValueTypeConstraintKind::DefinitionUse
    ));

    assert_eq!(
        compilation.state.checked_control_flow.is_published(&key),
        Ok(false)
    );

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::DeclaredValueTypeTemplates(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published declared value types must retain dependencies"),
        Err(error) => panic!("declared value type dependencies must be readable: {error:?}"),
    };

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));

    let bound = match compilation.bound_unit(key) {
        Ok(bound) => bound,
        Err(error) => panic!("bound callable must remain available: {error:?}"),
    };

    let [nested] = bound.value().nested_units() else {
        panic!("test callable must retain one anonymous callable");
    };

    let nested = nested.clone();

    let nested_templates = match compilation.declared_value_type_templates(nested.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("anonymous callable value types must publish: {error:?}"),
    };

    assert!(has_value_kind(
        nested_templates.value(),
        SymbolKind::AnonymousCallableParameter
    ));

    assert!(has_value_kind(
        nested_templates.value(),
        SymbolKind::GenericConstParameter
    ));

    assert!(matches!(
        nested_templates.value().callable_result(),
        Some(TypeExpressionTemplate::Array { .. })
    ));

    assert_eq!(
        compilation.state.checked_control_flow.is_published(&nested),
        Ok(false)
    );
}

#[test]
fn declared_value_type_templates_cover_declaration_surface_categories() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const size: i32 = 1;\n",
        "predicate valid(value: i32) = true;\n",
        "struct Holder\n",
        "{\n",
        "    func value() -> i32\n",
        "    {\n",
        "        return 1;\n",
        "    }\n",
        "}\n",
        "func check(value: i32) -> i32 ensures(result == value)\n",
        "{\n",
        "    return value;\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("declared unit keys must be available: {error:?}"),
    };

    let constant = types_for_kind(&compilation, &keys, BoundUnitKind::ConstantTemplate);

    assert!(has_value_kind(constant.value(), SymbolKind::Constant));

    assert!(has_constraint_kind(
        constant.value(),
        DeclaredValueTypeConstraintKind::Initializer
    ));

    let predicate = types_for_kind(&compilation, &keys, BoundUnitKind::PredicateDefinition);

    assert!(has_value_kind(
        predicate.value(),
        SymbolKind::PredicateParameter
    ));

    let receiver = keys
        .iter()
        .filter(|key| key.kind() == BoundUnitKind::CallableBody)
        .filter_map(|key| compilation.declared_value_type_templates(key.clone()).ok())
        .find(|analysis| has_value_kind(analysis.value(), SymbolKind::ReceiverParameter))
        .unwrap_or_else(|| panic!("type callable body must publish receiver evidence"));

    assert!(receiver.value().callable_result().is_some());

    let contract = types_for_kind(&compilation, &keys, BoundUnitKind::ContractClause);

    assert!(has_value_kind(
        contract.value(),
        SymbolKind::PostconditionResult
    ));

    assert!(has_value_kind(
        contract.value(),
        SymbolKind::CallableParameter
    ));
}

#[test]
fn runtime_defaults_publish_their_exact_parameter_type_template() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func defaults(first: i32 = 1, second: [i32; 1] = first)\n",
        "{\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("declared unit keys must be available: {error:?}"),
    };

    let templates = keys
        .iter()
        .filter(|key| key.kind() == BoundUnitKind::RuntimeDefault)
        .map(|key| {
            let analysis = match compilation.declared_value_type_templates(key.clone()) {
                Ok(analysis) => analysis,
                Err(error) => panic!("runtime default types must publish: {error:?}"),
            };

            let initializer = analysis
                .value()
                .constraints()
                .iter()
                .find(|constraint| {
                    constraint.kind()
                        == bray_bound_tree::DeclaredValueTypeConstraintKind::Initializer
                })
                .expect("runtime default must identify its initialized parameter");

            let evidence = analysis
                .value()
                .evidence()
                .iter()
                .find(|evidence| evidence.term() == initializer.right())
                .expect("runtime default must publish the initialized parameter's declared type");

            evidence.template().clone()
        })
        .collect::<Vec<_>>();

    assert_eq!(templates.len(), 2);

    assert!(
        templates
            .iter()
            .any(|template| matches!(template, TypeExpressionTemplate::Resolved(_)))
    );

    assert!(
        templates
            .iter()
            .any(|template| matches!(template, TypeExpressionTemplate::Array { .. }))
    );
}

#[test]
fn recovered_declared_value_type_syntax_is_deterministic_and_panic_free() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func broken(value: i32) -> i32\n",
        "{\n",
        "    let local: = value;\n",
        "    local\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let first = match compilation.declared_value_type_templates(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("recovered declared value types must publish: {error:?}"),
    };

    let second = match compilation.declared_value_type_templates(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated recovered request must publish: {error:?}"),
    };

    assert!(Arc::ptr_eq(&first, &second));

    assert!(
        first
            .value()
            .evidence()
            .iter()
            .any(|entry| matches!(entry.term(), DeclaredValueTypeTerm::Pattern(_)))
    );
}

#[test]
fn declared_array_types_resolve_their_embedded_constant_lengths() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos source: [i32; 4])\n",
        "{\n",
        "    let copy: [i32; 4] = source;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key) {
        Ok(types) => types,
        Err(error) => panic!("deferred expression types must publish: {error:?}"),
    };

    assert!(
        types.diagnostics().is_empty(),
        "checked declared types must not produce derived diagnostics: {:?}",
        types.diagnostics()
    );

    assert!(!types.value().is_recovered());
}

fn types_for_kind(
    compilation: &Compilation,
    keys: &[bray_bound_tree::BoundUnitKey],
    kind: BoundUnitKind,
) -> Arc<bray_diagnostics::DiagnosticResult<DeclaredValueTypeTemplates>> {
    let key = keys
        .iter()
        .find(|key| key.kind() == kind)
        .unwrap_or_else(|| panic!("test source must publish a {kind:?} unit"));

    match compilation.declared_value_type_templates(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("{kind:?} declared value types must publish: {error:?}"),
    }
}

fn has_constraint_kind(
    analysis: &DeclaredValueTypeTemplates,
    kind: DeclaredValueTypeConstraintKind,
) -> bool {
    analysis
        .constraints()
        .iter()
        .any(|entry| entry.kind() == kind)
}

fn has_value_kind(analysis: &DeclaredValueTypeTemplates, kind: SymbolKind) -> bool {
    analysis.evidence().iter().any(|entry| match entry.term() {
        DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(symbol)) => symbol.kind() == kind,
        DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(symbol)) => {
            symbol.kind() == kind
        }
        DeclaredValueTypeTerm::Expression(_)
        | DeclaredValueTypeTerm::Pattern(_)
        | DeclaredValueTypeTerm::BoxStoragePolicy(_) => false,
    })
}
