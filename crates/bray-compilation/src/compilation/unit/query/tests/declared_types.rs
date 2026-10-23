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

#[test]
fn closed_array_extents_share_literal_named_and_computed_types() {
    let compilation = compilation(
        r#"
        module app;

        const COUNT: usize = 1;
        const NARROW_COUNT: u8 = 1;

        const func count(pos value: usize) -> usize
        {
            return value;
        }

        const VALUES: [u8; NARROW_COUNT as usize] = [7];

        func literal(pos value: [u8; 1]) {}

        func sample()
        {
            let named: [u8; COUNT] = [7];
            let computed: [u8; count(COUNT)] = [7; COUNT];
            let widened: [u8; NARROW_COUNT as usize] = VALUES;

            literal(named);
            literal(computed);
            literal(widened);
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn substituted_array_extents_close_before_callable_and_field_comparison() {
    let compilation = compilation(
        r#"
        module app;

        struct Items<const count: usize>
        {
            values: [u8; count + 1];
        }

        func retain<const count: usize>(pos values: [u8; count + 1]) -> [u8; count + 1]
        {
            return values;
        }

        func sample() -> u8
        {
            let pair: [u8; 2] = retain<1>([2, 3]);
            let items: Items<1> = { values = pair, };

            return items.values[1];
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn invalid_closed_array_extents_preserve_their_source_diagnostics() {
    use bray_diagnostics::DiagnosticKind;

    for (source, expected) in [
        (
            r#"
            module app;
            const COUNT: usize = 2;

            func sample()
            {
                let values: [u8; COUNT] = [0];
            }
        "#,
            DiagnosticKind::CheckingIncompatibleExpressionType,
        ),
        (
            r#"
            module app;

            func sample()
            {
                let values: [u8; -1] = [0];
            }
        "#,
            DiagnosticKind::CheckingConstantLiteralNotRepresentable,
        ),
        (
            r#"
            module app;

            func sample()
            {
                let values: [u8; true] = [0];
            }
        "#,
            DiagnosticKind::CheckingIncompatibleExpressionType,
        ),
        (
            r#"
            module app;
            const COUNT: usize = 1 << 64;

            func sample()
            {
                let values: [u8; COUNT] = [0];
            }
        "#,
            DiagnosticKind::CheckingConstantValueNotRepresentable,
        ),
        (
            r#"
            module app;
            const FIRST: usize = SECOND;
            const SECOND: usize = FIRST;

            func sample()
            {
                let values: [u8; FIRST] = [0];
            }
        "#,
            DiagnosticKind::CheckingCyclicConstantDefinition,
        ),
    ] {
        let compilation = compilation(source);

        bray_testing::assert_goal_state_diagnostic_kind(compilation.check_diagnostics(), expected);
    }
}

#[test]
fn const_array_extents_normalize_signatures_nodes_defaults_and_anonymous_calls() {
    let compilation = compilation(
        r#"
        module app;

        const COUNT: usize = 1;
        const SOURCE: [u8; COUNT] = [7];

        const func make() -> [u8; COUNT]
        {
            return SOURCE;
        }

        const func retain<const count: usize>(pos values: [u8; count + 1]) -> [u8; count + 1]
        {
            return values;
        }

        const func defaulted(pos values: [u8; COUNT] = SOURCE) -> [u8; COUNT]
        {
            return values;
        }

        const func anonymous() -> [u8; COUNT]
        {
            let factory = const lambda () -> [u8; COUNT]
            {
                return [7];
            };

            return factory();
        }

        const VALUES: [u8; 1] = make();
        const PAIR: [u8; 2] = retain<1>([2, 3]);
        const DEFAULTED: [u8; 1] = defaulted();
        const ANONYMOUS: [u8; 1] = anonymous();
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn nested_generic_array_extents_close_computed_helper_arguments() {
    let compilation = compilation(
        r#"
        module app;

        const func extent<const amount: usize>() -> usize
        {
            return amount;
        }

        func retain<const count: usize>(
            pos values: [u8; extent<count + 1>()],
        ) -> [u8; extent<count + 1>()]
        {
            return values;
        }

        func sample() -> u8
        {
            let pair: [u8; 2] = retain<1>([2, 3]);

            return pair[1];
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn closed_generic_array_extents_check_even_unused_helper_arguments() {
    for (count, valid) in [(1, true), (255, false)] {
        let source = format!(
            r#"
            module app;

            const func extent<const amount: u8>() -> usize
            {{
                return 1;
            }}

            func accept<const count: u8>(pos values: [u8; extent<count + 1>()]) {{}}

            func sample()
            {{
                accept<{count}>([0]);
            }}
        "#
        );

        let compilation = compilation(&source);

        if valid {
            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:#?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingConstantValueNotRepresentable,
            );

            let diagnostic = compilation
                .check_diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingConstantValueNotRepresentable)
                .next()
                .expect("invalid u8 argument has a conversion diagnostic");

            assert!(
                diagnostic
                    .args()
                    .contains(&bray_diagnostics::DiagnosticArg::actual_type(
                        bray_diagnostics::DiagnosticType::U8,
                    ))
            );
        }
    }
}
