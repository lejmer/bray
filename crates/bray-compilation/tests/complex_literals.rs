use bray_bound_tree::{
    BoundExpressionId, BoundSourceAnchor, BoundUnitKey, CheckedExpressionTypes, SemanticSelection,
};
use bray_compilation::{Compilation, CompilationRequest};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticKind;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    ConstantValueKind, GenericOwnerId, GenericSubstitutionData, PackageIdentity, SymbolOrigin,
};

fn compilation(source: &str) -> Compilation {
    let source = SourceInput::virtual_text(
        SourceIdentity::new(1),
        "complex-literals",
        SourceVersion::new(1),
        source,
    );

    let request = CompilationRequest::new(
        PackageIdentity::try_new("complex.literal.test").expect("test package identity is valid"),
        vec![source],
    );

    Compilation::load(request).expect("complex literal source loads")
}

fn source_function_body_key(compilation: &Compilation, name: &str) -> BoundUnitKey {
    let graph = compilation.symbol_graph().expect("source symbols publish");

    let function = graph
        .functions()
        .iter()
        .find(|function| {
            function.origin() == SymbolOrigin::Source
                && graph
                    .member_name(function.id().into())
                    .is_some_and(|value| value.as_str() == name)
        })
        .expect("test source has the selected function");

    let anchor = function
        .syntax_anchor()
        .expect("source function has a syntax anchor");

    let source = compilation
        .source(anchor.source_id())
        .expect("function source is loaded");

    BoundUnitKey::callable_body(
        function.key().clone(),
        BoundSourceAnchor::new(anchor, source.version()),
    )
    .expect("source function has a callable body")
}

fn assert_expression_representation(
    compilation: &Compilation,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    expected: RepresentationRole,
) {
    let ty = types
        .expression(expression)
        .expect("checked expression has a final type");

    let values = compilation
        .semantic_value_store()
        .expect("semantic values publish");

    assert_eq!(
        compilation
            .available_compiler_known_symbols()
            .type_representation(values, ty.ty()),
        Some(expected)
    );
}

#[test]
fn complex_literals_preserve_component_types_through_selection_and_lowering() {
    for (annotation, role) in [
        (": c32", RepresentationRole::ScalarC32),
        (": c64", RepresentationRole::ScalarC64),
        (": c128", RepresentationRole::ScalarC128),
        (": c256", RepresentationRole::ScalarC256),
        ("", RepresentationRole::ScalarC128),
    ] {
        let source = format!(
            r#"
            module app;

            func sample()
            {{
                let positive{annotation} = 1.0 + 2.0i;
                let negative{annotation} = 1.0 - 2.0i;
                let signed{annotation} = -1.0 + 2.0i;
                let integer_imaginary{annotation} = 1.0 + 2i;
            }}
        "#
        );

        let compilation = compilation(&source);

        if compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(role)
            .is_none()
        {
            continue;
        }

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "sample");

        let bound = compilation
            .bound_unit(key.clone())
            .expect("complex literal source binds");

        let types = compilation
            .expression_types(key.clone())
            .expect("complex literal types publish");

        let selections = compilation
            .semantic_selections(key.clone())
            .expect("complex literal selections publish");

        assert!(types.diagnostics().is_empty(), "{:?}", types.diagnostics());

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let mut literals = 0;

        for (expression, _) in bound.value().tree().expressions() {
            let Some(components) = bound.value().view().complex_literal_components(expression)
            else {
                continue;
            };

            literals += 1;
            assert_expression_representation(&compilation, types.value(), expression, role);

            for component in components {
                assert_expression_representation(
                    &compilation,
                    types.value(),
                    component,
                    role.complex_component()
                        .expect("complex role has components"),
                );
            }

            let Some(SemanticSelection::Operation(bray_bound_tree::SelectedOperation::Operator {
                result_type,
                ..
            })) = selections.value().expression(expression)
            else {
                panic!("complex literal retains its selected built-in operation");
            };

            assert_eq!(
                *result_type,
                types
                    .value()
                    .expression(expression)
                    .expect("literal result has a type")
                    .ty()
            );
        }

        assert_eq!(literals, 4);

        let lowered = compilation
            .lowered_unit(key)
            .expect("complex literal source lowers");

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );

        assert!(
            lowered
                .value()
                .as_ref()
                .expect("checked source lowers")
                .mir()
                .expect("lowered source has MIR")
                .is_valid()
        );
    }
}

#[test]
fn complex_literal_constants_preserve_addition_and_subtraction_bits() {
    let compilation = compilation(
        r#"
        module app;

        const positive: c64 = 1.0 + 2.0i;
        const negative: c128 = 1.0 - 2.0i;
        const signed_zero: c128 = -0.0 - 0.0i;
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let graph = compilation
        .symbol_graph()
        .expect("complex constants have symbols");

    let values = compilation
        .semantic_value_store()
        .expect("complex constants have semantic values");

    let mut constants = 0;

    for symbol in graph
        .constants()
        .iter()
        .filter(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
    {
        let definition = bray_symbols::AnyConstantDefinitionId::Constant(symbol.id());

        let owner =
            GenericOwnerId::try_new(definition.into_any()).expect("constant owns its substitution");

        let substitution =
            GenericSubstitutionData::try_new(owner, [], []).expect("constant is non-generic");

        let substitution = values
            .intern_generic_substitution(substitution)
            .expect("empty substitution interns");

        let constant = compilation
            .constant_instance(bray_symbols::ConstantInstanceKey::new(
                definition,
                substitution,
                None,
            ))
            .expect("source complex constant evaluates");

        assert!(
            constant.diagnostics().is_empty(),
            "{:?}",
            constant.diagnostics()
        );

        let value = values.constant_value_data(constant.value().value());

        let expected = match graph
            .member_name(symbol.id().into())
            .expect("source constant has its declared name")
            .as_str()
        {
            "positive" => ConstantValueKind::Complex {
                real: bray_symbols::RealConstantBits::Binary32(0x3f800000),
                imaginary: bray_symbols::RealConstantBits::Binary32(0x40000000),
            },
            "negative" => ConstantValueKind::Complex {
                real: bray_symbols::RealConstantBits::Binary64(0x3ff0000000000000),
                imaginary: bray_symbols::RealConstantBits::Binary64(0xc000000000000000),
            },
            "signed_zero" => ConstantValueKind::Complex {
                real: bray_symbols::RealConstantBits::Binary64(0x8000000000000000),
                imaginary: bray_symbols::RealConstantBits::Binary64(0x8000000000000000),
            },
            name => panic!("unexpected source constant {name}"),
        };

        assert_eq!(value.kind(), &expected);
        constants += 1;
    }

    assert_eq!(constants, 3);
}

#[test]
fn typed_real_operands_still_require_explicit_complex_construction() {
    for expression in [
        "real + 2.0i",
        "real - 2.0i",
        "real + imaginary",
        "real - imaginary",
        "-real + 2.0i",
        "-real - 2.0i",
    ] {
        let source = format!(
            r#"
            module app;

            func sample(pos real: r64, pos imaginary: r64) -> c128
            {{
                return {expression};
            }}
        "#
        );

        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "sample");

        let types = compilation
            .expression_types(key.clone())
            .expect("invalid source reports type errors");

        let errors = types
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
            .collect::<Vec<_>>();

        assert!(!errors.is_empty(), "{:?}", types.diagnostics());

        for diagnostic in errors {
            bray_testing::assert_goal_state_diagnostic(diagnostic);
        }

        assert!(
            compilation
                .lowered_unit(key)
                .expect("invalid source remains checkable")
                .value()
                .is_none()
        );
    }

    let compilation = compilation(
        r#"
        module app;

        func sample(pos real: r64, pos imaginary: r64) -> c128
        {
            return (real, imaginary) as c128;
        }
    "#,
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "sample"))
        .expect("explicit typed construction lowers");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );

    assert!(
        lowered
            .value()
            .as_ref()
            .expect("explicit construction has a lowered body")
            .mir()
            .expect("explicit construction has MIR")
            .is_valid()
    );
}

#[test]
fn complex_literal_context_flows_from_calls_and_returns() {
    let compilation = compilation(
        r#"
        module app;

        func identity(pos value: c64) -> c64
        {
            return value;
        }

        func make() -> c64
        {
            return 1.0 + 2.0i;
        }

        func sample()
        {
            let value = identity(1.0 - 2.0i);
            let second = make();
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["make", "sample"] {
        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, name))
            .expect("contextual complex source lowers");

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );

        assert!(
            lowered
                .value()
                .as_ref()
                .expect("contextual complex body lowers")
                .mir()
                .expect("contextual complex body has MIR")
                .is_valid()
        );
    }
}

#[test]
fn complex_arithmetic_constants_cover_every_advertised_operation() {
    let compilation = compilation(
        r#"
        module app;

        const LEFT: c128 = 3.0 + 4.0i;
        const RIGHT: c128 = 1.0 + 1.0i;
        const sum: c128 = LEFT + RIGHT;
        const difference: c128 = LEFT - RIGHT;
        const product: c128 = LEFT * RIGHT;
        const quotient: c128 = LEFT / RIGHT;
        const power: c128 = RIGHT ** (2.0 + 0.0i);
        const negation: c128 = -LEFT;
        const ZERO: c128 = 0.0 + 0.0i;
        const infinity: c128 = RIGHT / ZERO;
        const nan: c128 = ZERO / ZERO;
        const identity: c128 = nan ** ZERO;
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let graph = compilation
        .symbol_graph()
        .expect("arithmetic constants have symbols");

    let values = compilation
        .semantic_value_store()
        .expect("arithmetic constants have values");

    let expected = [
        ("LEFT", [0x4008_0000_0000_0000, 0x4010_0000_0000_0000]),
        ("RIGHT", [0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000]),
        ("sum", [0x4010_0000_0000_0000, 0x4014_0000_0000_0000]),
        ("difference", [0x4000_0000_0000_0000, 0x4008_0000_0000_0000]),
        ("product", [0xbff0_0000_0000_0000, 0x401c_0000_0000_0000]),
        ("quotient", [0x400c_0000_0000_0000, 0x3fe0_0000_0000_0000]),
        ("power", [0, 0x4000_0000_0000_0000]),
        ("negation", [0xc008_0000_0000_0000, 0xc010_0000_0000_0000]),
        ("ZERO", [0, 0]),
        ("infinity", [0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000]),
        ("nan", [0x7ff8_0000_0000_0000, 0x7ff8_0000_0000_0000]),
        ("identity", [0x3ff0_0000_0000_0000, 0]),
    ];

    let mut count = 0;

    for symbol in graph
        .constants()
        .iter()
        .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
    {
        let name = graph
            .member_name(symbol.id().into())
            .expect("source constant has a name");

        let definition = bray_symbols::AnyConstantDefinitionId::Constant(symbol.id());

        let owner =
            GenericOwnerId::try_new(definition.into_any()).expect("constant owns its substitution");

        let substitution =
            GenericSubstitutionData::try_new(owner, [], []).expect("constant is non-generic");

        let substitution = values
            .intern_generic_substitution(substitution)
            .expect("empty substitution interns");

        let constant = compilation
            .constant_instance(bray_symbols::ConstantInstanceKey::new(
                definition,
                substitution,
                None,
            ))
            .expect("source arithmetic constant evaluates");

        assert!(
            constant.diagnostics().is_empty(),
            "{:?}",
            constant.diagnostics()
        );

        let bits = expected
            .iter()
            .find(|(expected, _)| *expected == name.as_str())
            .expect("source constant has an expected result")
            .1;

        let value = values.constant_value_data(constant.value().value());

        assert_eq!(
            value.kind(),
            &ConstantValueKind::Complex {
                real: bray_symbols::RealConstantBits::Binary64(bits[0]),
                imaginary: bray_symbols::RealConstantBits::Binary64(bits[1]),
            },
            "{name}"
        );

        count += 1;
    }

    assert_eq!(count, expected.len());
}

#[test]
fn complex_runtime_operations_keep_selected_representations_and_numeric_helpers() {
    for (name, role) in [
        ("c32", RepresentationRole::ScalarC32),
        ("c64", RepresentationRole::ScalarC64),
        ("c128", RepresentationRole::ScalarC128),
        ("c256", RepresentationRole::ScalarC256),
    ] {
        let source = format!(
            r#"
            module app;

            func sample(pos left: {name}, pos right: {name})
            {{
                let sum = left + right;
                let difference = left - right;
                let product = left * right;
                let quotient = left / right;
                let power = left ** right;
                let negation = -left;
                let mut assigned = left;

                assigned += right;
                assigned -= right;
                assigned *= right;
                assigned /= right;
                assigned **= right;
            }}
        "#
        );

        let compilation = compilation(&source);

        if compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(role)
            .is_none()
        {
            continue;
        }

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "sample");

        let lowered = compilation
            .lowered_unit(key)
            .expect("complex operators lower");

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );

        let mir = lowered
            .value()
            .as_ref()
            .expect("checked source lowers")
            .mir()
            .expect("ordinary function has MIR");

        assert!(mir.is_valid());

        let roles = bray_codegen::demanded_runtime_references_for_mir(mir)
            .into_iter()
            .map(|reference| reference.role())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            roles,
            [
                bray_runtime_interface::RuntimeAbiRole::ComplexMultiply,
                bray_runtime_interface::RuntimeAbiRole::ComplexDivide,
                bray_runtime_interface::RuntimeAbiRole::ComplexPower
            ]
            .into_iter()
            .collect()
        );
    }
}
