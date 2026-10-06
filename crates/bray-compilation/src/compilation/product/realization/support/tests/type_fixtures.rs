use std::collections::BTreeMap;

use bray_codegen::{CodegenTarget, CodegenTypeMapping};
use bray_symbols::{NamedTypeSymbolId, SymbolOrigin, TypeData, TypeId};

use crate::compilation::substitution::empty_substitution;
use crate::{CancellationToken, Compilation};

pub(super) fn source_union_type(compilation: &Compilation) -> TypeId {
    let symbols = compilation
        .symbol_graph()
        .expect("test symbol graph must build");

    let union = symbols
        .unions()
        .iter()
        .find(|union| union.origin() == SymbolOrigin::Source)
        .expect("test source must declare one union");

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let substitution =
        empty_substitution(values, union.id().into()).expect("union substitution must intern");

    values
        .intern_type(TypeData::Named {
            definition: NamedTypeSymbolId::Union(union.id()),
            substitution,
        })
        .expect("union type must intern")
}

pub(super) fn realized_types(
    compilation: &Compilation,
    target: &CodegenTarget,
    demanded: impl IntoIterator<Item = TypeId>,
) -> BTreeMap<TypeId, CodegenTypeMapping> {
    compilation
        .codegen_types(
            demanded.into_iter().collect(),
            None,
            target,
            &CancellationToken::new(),
        )
        .unwrap_or_else(|error| panic!("types must realize: {error:?}"))
        .into_iter()
        .map(|mapping| (mapping.ty(), mapping))
        .collect()
}
