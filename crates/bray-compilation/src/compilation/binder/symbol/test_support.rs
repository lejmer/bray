use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueData, ConstantValueId, SymbolFactContract,
    SymbolFactRequest, SymbolFactResult, SymbolGraph, TypeData,
};

use super::super::context::CompilationBinderFacts;
use crate::compilation::Compilation;

pub(super) fn published_fact<C>(
    facts: &CompilationBinderFacts<'_>,
    request: SymbolFactRequest<C>,
) -> Arc<SymbolFactResult<C>>
where
    C: SymbolFactContract,
    for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
{
    match facts.symbol_fact(request) {
        Ok(result) => result,
        Err(error) => panic!("test symbol fact must bind: {error:?}"),
    }
}

pub(super) fn symbol_graph(compilation: &Compilation) -> &SymbolGraph {
    match compilation.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test symbol graph must build: {error:?}"),
    }
}

pub(super) fn type_data(compilation: &Compilation, ty: bray_symbols::TypeId) -> Arc<TypeData> {
    let values = semantic_values(compilation);

    match values.type_data(ty) {
        Ok(data) => data,
        Err(error) => panic!("semantic type must be interned: {error:?}"),
    }
}

pub(super) fn constant_term_data(
    compilation: &Compilation,
    term: ConstantTermId,
) -> Arc<ConstantTermData> {
    let values = semantic_values(compilation);

    match values.constant_term_data(term) {
        Ok(data) => data,
        Err(error) => panic!("semantic constant term must be interned: {error:?}"),
    }
}

pub(super) fn constant_value_data(
    compilation: &Compilation,
    value: ConstantValueId,
) -> Arc<ConstantValueData> {
    let values = semantic_values(compilation);

    match values.constant_value_data(value) {
        Ok(data) => data,
        Err(error) => panic!("semantic constant value must be interned: {error:?}"),
    }
}

fn semantic_values(compilation: &Compilation) -> &bray_symbols::SemanticValueStore {
    match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic value store must be available: {error:?}"),
    }
}
