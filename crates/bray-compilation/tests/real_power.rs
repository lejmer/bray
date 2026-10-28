use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_compilation::{Compilation, CompilationRequest};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBinaryOperator, MirOperationKind};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    ConstantValueKind, GenericOwnerId, GenericSubstitutionData, PackageIdentity, RealConstantBits,
    SymbolOrigin,
};

fn compilation(source: &str) -> Compilation {
    let source = SourceInput::virtual_text(
        SourceIdentity::new(1),
        "real-power",
        SourceVersion::new(1),
        source,
    );

    let request = CompilationRequest::new(
        PackageIdentity::try_new("real.power.test").expect("test package identity is valid"),
        vec![source],
    );

    Compilation::load(request).expect("real power source loads")
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

fn real_value(role: RepresentationRole, bits: u128) -> RealConstantBits {
    match role {
        RepresentationRole::ScalarR16 => {
            RealConstantBits::Binary16(u16::try_from(bits).expect("binary16 fits"))
        }
        RepresentationRole::ScalarR32 => {
            RealConstantBits::Binary32(u32::try_from(bits).expect("binary32 fits"))
        }
        RepresentationRole::ScalarR64 => {
            RealConstantBits::Binary64(u64::try_from(bits).expect("binary64 fits"))
        }
        RepresentationRole::ScalarR128 => RealConstantBits::Binary128(bits.to_be_bytes()),
        _ => panic!("fixture uses a real representation"),
    }
}

#[test]
fn real_power_constants_preserve_selected_precision_and_ieee_boundaries() {
    for (name, role, fraction, exponent) in [
        ("r16", RepresentationRole::ScalarR16, 10, 5),
        ("r32", RepresentationRole::ScalarR32, 23, 8),
        ("r64", RepresentationRole::ScalarR64, 52, 11),
        ("r128", RepresentationRole::ScalarR128, 112, 15),
    ] {
        let source = format!(
            r#"
            module app;

            const power: {name} = (-2.0) ** 3.0;
            const root: {name} = 9.0 ** 0.5;
            const quarter: {name} = 16.0 ** 0.25;
            const nan: {name} = (-2.0) ** 0.5;
            const infinity: {name} = 0.0 ** -1.0;
            const negative_zero: {name} = (-0.0) ** 3.0;
            const negative_infinity: {name} = (-0.0) ** -3.0;
            const zero_from_infinity: {name} = 2.0 ** -infinity;
            const infinity_from_infinity: {name} = 0.5 ** -infinity;
            const nan_zero_identity: {name} = nan ** -0.0;
            const one_nan_identity: {name} = 1.0 ** nan;
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

        let one = ((1_u128 << (exponent - 1)) - 1) << fraction;
        let sign = 1_u128 << (fraction + exponent);
        let infinity = ((1_u128 << exponent) - 1) << fraction;

        let expected = [
            ("power", (one + (3_u128 << fraction)) | sign),
            (
                "root",
                one + (1_u128 << fraction) + (1_u128 << (fraction - 1)),
            ),
            ("quarter", one + (1_u128 << fraction)),
            ("nan", infinity | (1_u128 << (fraction - 1))),
            ("infinity", infinity),
            ("negative_zero", sign),
            ("negative_infinity", infinity | sign),
            ("zero_from_infinity", 0),
            ("infinity_from_infinity", infinity),
            ("nan_zero_identity", one),
            ("one_nan_identity", one),
        ];

        let graph = compilation.symbol_graph().expect("source symbols publish");

        let values = compilation
            .semantic_value_store()
            .expect("source values publish");

        let mut count = 0;

        for symbol in graph
            .constants()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
        {
            let name = graph
                .member_name(symbol.id().into())
                .expect("constant has a name");

            let expected = expected
                .iter()
                .find(|(key, _)| *key == name.as_str())
                .expect("constant expected")
                .1;

            let definition = bray_symbols::AnyConstantDefinitionId::Constant(symbol.id());

            let owner =
                GenericOwnerId::try_new(definition.into_any()).expect("constant owns substitution");

            let substitution =
                GenericSubstitutionData::try_new(owner, [], []).expect("non-generic constant");

            let substitution = values
                .intern_generic_substitution(substitution)
                .expect("substitution interns");

            let result = compilation
                .constant_instance(bray_symbols::ConstantInstanceKey::new(
                    definition,
                    substitution,
                    None,
                ))
                .expect("constant evaluates");

            assert!(
                result.diagnostics().is_empty(),
                "{:?}",
                result.diagnostics()
            );

            assert_eq!(
                values.constant_value_data(result.value().value()).kind(),
                &ConstantValueKind::Real(real_value(role, expected)),
                "{}",
                name.as_str()
            );

            count += 1;
        }

        assert_eq!(count, expected.len());
    }
}

#[test]
fn ordinary_and_compound_real_powers_demand_the_selected_helper() {
    for (name, role) in [
        ("r16", RepresentationRole::ScalarR16),
        ("r32", RepresentationRole::ScalarR32),
        ("r64", RepresentationRole::ScalarR64),
        ("r128", RepresentationRole::ScalarR128),
        ("i64", RepresentationRole::ScalarI64),
    ] {
        let source = format!(
            r#"
            module app;

            func sample(pos base: {name}, pos exponent: {name}) -> {name}
            {{
                let mut value = base;
                value **= exponent;
                return value ** exponent;
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

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "sample"))
            .expect("power lowers");

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

        let expected_operator = if role == RepresentationRole::ScalarI64 {
            MirBinaryOperator::Exponentiate
        } else {
            MirBinaryOperator::RealExponentiate
        };

        let operators: Vec<_> = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Binary { operator, .. } => Some(*operator),
                _ => None,
            })
            .collect();

        assert_eq!(operators, vec![expected_operator; 2]);

        let roles: std::collections::BTreeSet<_> =
            bray_codegen::demanded_runtime_references_for_mir(mir)
                .into_iter()
                .map(|reference| reference.role())
                .collect();

        let expected = if role == RepresentationRole::ScalarI64 {
            std::collections::BTreeSet::new()
        } else {
            [bray_runtime_interface::RuntimeAbiRole::RealPower]
                .into_iter()
                .collect()
        };

        assert_eq!(roles, expected);
    }
}
