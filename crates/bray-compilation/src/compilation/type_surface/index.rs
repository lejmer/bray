use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_symbols::{
    ImplementationCoherenceQuery, ImplementationSymbolId, InherentImplementationSymbolId,
    NamedTypeSymbolId, SymbolOrigin, SymbolQueryRequest, TypeData,
};

use super::aggregation::sort_symbols_by_key;
use crate::compilation::Compilation;
use crate::compilation::binder;
use crate::compilation::source_graph::{
    source_declaration_module_parts, source_symbol_contribution_gate,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

#[derive(Hash)]
pub(in crate::compilation) struct InherentImplementationAssociationIndex {
    by_subject: BTreeMap<NamedTypeSymbolId, Arc<[InherentImplementationSymbolId]>>,
}

impl InherentImplementationAssociationIndex {
    pub(super) fn implementations(
        &self,
        subject: NamedTypeSymbolId,
    ) -> &[InherentImplementationSymbolId] {
        self.by_subject
            .get(&subject)
            .map(Arc::as_ref)
            .unwrap_or(&[])
    }
}

impl Compilation {
    pub(super) fn type_associated_implementation_index(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&InherentImplementationAssociationIndex, FactQueryError> {
        self.query_with_cancellation(
            CompilationFactKey::TypeAssociatedImplementationIndex,
            &self.state.type_associated_implementation_index,
            cancellation,
            |cancellation| self.compute_type_associated_implementation_index(cancellation),
        )
    }

    fn compute_type_associated_implementation_index(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<InherentImplementationAssociationIndex, FactQueryError> {
        let binding_context = self.binding_context(cancellation)?;

        let source_graph = self.product_source_graph()?;

        let source_module_parts = source_declaration_module_parts(source_graph.declarations());

        let imported = binding_context
            .imported_symbols()
            .map_err(binder::binding_query_error)?;

        let mut implementations = binding_context
            .symbols()
            .inherent_implementations()
            .iter()
            .filter(|implementation| {
                implementation.origin() != SymbolOrigin::Source
                    || source_symbol_contribution_gate(
                        source_graph,
                        binding_context.symbols(),
                        &source_module_parts,
                        implementation.id().into(),
                    )
                    .is_some()
            })
            .map(|implementation| implementation.id())
            .chain(
                imported
                    .into_iter()
                    .flat_map(|symbols| symbols.inherent_implementations())
                    .map(|implementation| implementation.id()),
            )
            .collect::<Vec<_>>();

        implementations = sort_symbols_by_key(&binding_context, implementations)?;

        let mut by_subject = BTreeMap::<_, Vec<_>>::new();

        for implementation in implementations {
            cancellation.check()?;

            let subject = match binding_context
                .imported_semantic_address(implementation.into())
                .map_err(binder::binding_query_error)?
            {
                Some(address) => {
                    let imported = binder::imported_implementation(&binding_context, address)
                        .map_err(binder::binding_query_error)?;

                    if imported.value().trait_application().is_some() {
                        continue;
                    }

                    imported.value().subject().ty()
                }
                None => {
                    let coherence = binding_context
                        .resolve_symbol_query(
                            SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
                                ImplementationSymbolId::from(implementation),
                            ),
                        )
                        .map_err(binder::binding_query_error)?;

                    if coherence.value().trait_application().is_some() {
                        continue;
                    }

                    coherence.value().subject()
                }
            };

            let data = binding_context
                .semantic_values()
                .type_data(subject)
                .map_err(FactQueryError::SemanticValueStore)?;

            let TypeData::Named { definition, .. } = data.as_ref() else {
                continue;
            };

            by_subject
                .entry(*definition)
                .or_default()
                .push(implementation);
        }

        Ok(InherentImplementationAssociationIndex {
            by_subject: by_subject
                .into_iter()
                .map(|(subject, implementations)| (subject, implementations.into()))
                .collect(),
        })
    }
}
