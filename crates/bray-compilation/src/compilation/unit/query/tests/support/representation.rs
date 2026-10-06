use crate::Compilation;

use bray_bound_tree::{BoundExpressionId, CheckedExpressionTypes};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{NamedTypeSymbolId, TypeData};

pub(in crate::compilation::unit::query::tests) fn assert_expression_representation(
    compilation: &Compilation,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    expected: RepresentationRole,
) {
    let Some(result) = types.expression(expression) else {
        panic!("selected expression must have a final type");
    };

    assert_type_representation(compilation, result.ty(), expected);
}

pub(in crate::compilation::unit::query::tests) fn assert_type_representation(
    compilation: &Compilation,
    ty: bray_symbols::TypeId,
    expected: RepresentationRole,
) {
    assert_eq!(type_representation(compilation, ty), Some(expected));
}

pub(in crate::compilation::unit::query::tests) fn type_representation(
    compilation: &Compilation,
    ty: bray_symbols::TypeId,
) -> Option<RepresentationRole> {
    let values = match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic values must be available: {error:?}"),
    };

    let data = values.type_data(ty);

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return None;
    };

    match definition {
        NamedTypeSymbolId::Struct(definition) => compilation
            .available_compiler_known_symbols()
            .symbol_representation(*definition),
        NamedTypeSymbolId::Union(definition) => compilation
            .available_compiler_known_symbols()
            .symbol_representation(*definition),
    }
}
