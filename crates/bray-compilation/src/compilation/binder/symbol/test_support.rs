use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_symbols::{
    SymbolFactContract, SymbolFactRequest, SymbolFactResult, SymbolGraph, TypeData,
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
    let values = match compilation.semantic_value_store() {
        Ok(values) => values,
        Err(error) => panic!("semantic value store must be available: {error:?}"),
    };

    match values.type_data(ty) {
        Ok(data) => data,
        Err(error) => panic!("semantic type must be interned: {error:?}"),
    }
}
