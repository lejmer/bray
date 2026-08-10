use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_symbols::{
    ImplementationCoherenceFact, ImplementationSymbolId, InherentImplementationSymbolId,
    NamedTypeSymbolId, SymbolFactRequest, SymbolOrigin, TypeData,
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
        self.query_fact_with_cancellation(
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
        let facts = self.binder_facts(cancellation)?;

        let source_graph = self.product_source_graph()?;

        let source_module_parts = source_declaration_module_parts(source_graph.declarations());

        let imported = facts
            .imported_symbols()
            .map_err(binder::binder_fact_error)?;

        let mut implementations = facts
            .symbols()
            .inherent_implementations()
            .iter()
            .filter(|implementation| {
                implementation.origin() != SymbolOrigin::Source
                    || source_symbol_contribution_gate(
                        source_graph,
                        facts.symbols(),
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

        implementations = sort_symbols_by_key(&facts, implementations)?;

        let mut by_subject = BTreeMap::<_, Vec<_>>::new();

        for implementation in implementations {
            cancellation.check()?;

            let subject = match facts
                .imported_fact_address(implementation.into())
                .map_err(binder::binder_fact_error)?
            {
                Some(address) => {
                    let imported = binder::imported_implementation(&facts, address)
                        .map_err(binder::binder_fact_error)?;

                    if imported.value().trait_application().is_some() {
                        continue;
                    }

                    imported.value().subject().ty()
                }
                None => {
                    let coherence = facts
                        .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                            ImplementationSymbolId::from(implementation),
                        ))
                        .map_err(binder::binder_fact_error)?;

                    if coherence.value().trait_application().is_some() {
                        continue;
                    }

                    coherence.value().subject()
                }
            };

            let data = facts
                .semantic_values()
                .type_data(subject)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

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
