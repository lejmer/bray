use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::BoundUnitKind;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableContractSet, CallableContractsQuery, CallableDefinitionId, CallableSymbolId,
    ImplementationSymbolId, ModuleSurfaceQuery, NamedTypeSymbolId, SymbolOrigin, SymbolQueryKind,
    SymbolQueryRequest,
};

use super::unit::unit_order_key;
use crate::compilation::Compilation;
use crate::fact::{
    BatchWork, CancellationToken, CompilationFactKey, DiagnosticPublicationOrder, FactQueryError,
    OrderedDiagnosticCollection, SymbolQueryKey, publish_diagnostics,
};

#[derive(Hash)]
pub(in crate::compilation) struct SemanticDiagnosticPublication {
    diagnostics: DiagnosticBag,
    callable_contracts: BTreeMap<CallableSymbolId, Arc<DiagnosticResult<CallableContractSet>>>,
}

impl Compilation {
    /// Returns diagnostics produced by binding and semantic analysis of this package.
    pub fn semantic_diagnostics(&self) -> &DiagnosticBag {
        crate::compilation::boundary::expect_uncancelled_query(
            "semantic_diagnostics",
            self.semantic_diagnostics_with_cancellation(&self.state.cancellation),
        )
    }

    pub(in crate::compilation) fn semantic_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
            CompilationFactKey::SemanticDiagnostics,
            &self.state.semantic_diagnostics,
            cancellation,
            |cancellation| self.compute_semantic_diagnostics(cancellation),
        )
        .map(|publication| &publication.diagnostics)
    }

    pub(in crate::compilation) fn published_callable_contract(
        &self,
        callable: CallableSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<DiagnosticResult<CallableContractSet>>>, FactQueryError> {
        let Some(publication) = self
            .state
            .semantic_diagnostics
            .get_if_published(&CompilationFactKey::SemanticDiagnostics)
        else {
            return Ok(None);
        };

        let Some(contracts) = publication.callable_contracts.get(&callable) else {
            return Ok(None);
        };

        let key = CompilationFactKey::from(SymbolQueryKey::new(
            callable.into_any(),
            SymbolQueryKind::CallableContracts,
        ));

        self.state.fact_runtime.check_request_cycle(&key)?;
        cancellation.check()?;
        self.state.fact_runtime.record_completed_request(&key)?;

        Ok(Some(Arc::clone(contracts)))
    }

    /// Returns diagnostics for the current whole-package check request.
    pub fn check_diagnostics(&self) -> &DiagnosticBag {
        crate::compilation::boundary::expect_uncancelled_query(
            "check_diagnostics",
            self.check_diagnostics_with_cancellation(&self.state.cancellation),
        )
    }

    pub(in crate::compilation) fn check_diagnostics_with_cancellation(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
            CompilationFactKey::CheckDiagnostics,
            &self.state.check_diagnostics,
            cancellation,
            |cancellation| self.compute_check_diagnostics(cancellation),
        )
    }

    fn compute_check_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let diagnostics = self.state.fact_runtime.map_indexed(4, |index| {
            cancellation.check()?;

            let diagnostics = match index {
                0 => self.source_diagnostics(),
                1 => self.syntax_tree_result().diagnostics(),
                2 => self.imported_diagnostics(),
                3 => self.semantic_diagnostics_with_cancellation(cancellation)?,
                _ => unreachable!("scheduled diagnostic index must be in range"),
            };

            cancellation.check()?;

            Ok::<_, FactQueryError>(diagnostics)
        })?;

        let diagnostics = diagnostics.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(publish_diagnostics(
            self.state.fact_runtime.profile(),
            ordered_diagnostic_collections(diagnostics),
        ))
    }

    fn has_incomplete_callable_parameters(&self) -> bool {
        if !self.syntax_tree_result().diagnostics().has_errors() {
            return false;
        }

        let mut incomplete = false;

        bray_syntax::walk_syntax_tree(self.syntax_tree(), |event| {
            if let bray_syntax::SyntaxWalkEvent::EnterNode(node) = event {
                let missing_name = node
                    .cast::<bray_syntax::ParameterSyntax>()
                    .is_some_and(|parameter| parameter.identifier_token().is_missing())
                    || node
                        .cast::<bray_syntax::PredicateParameterSyntax>()
                        .is_some_and(|parameter| parameter.identifier_token().is_missing());

                if !missing_name {
                    return bray_syntax::SyntaxWalkControl::Continue;
                }

                incomplete = true;

                return bray_syntax::SyntaxWalkControl::Stop;
            }

            bray_syntax::SyntaxWalkControl::Continue
        });

        incomplete
    }

    fn compute_semantic_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<SemanticDiagnosticPublication, FactQueryError> {
        // Recovered nameless parameters cannot form complete callable signatures.
        // Other syntax recovery can still publish useful semantic diagnostics.
        if self.source_diagnostics().has_errors() || self.has_incomplete_callable_parameters() {
            return Ok(SemanticDiagnosticPublication {
                diagnostics: DiagnosticBag::new(),
                callable_contracts: BTreeMap::new(),
            });
        }

        let source_graph = self.product_source_graph()?;

        let symbols = self.symbol_graph()?;

        // Publication owns diagnostic bags after their analysis results are released.
        let mut sources = Vec::new();
        let binder = self.binding_context(cancellation)?;

        for module in symbols
            .modules()
            .iter()
            .filter(|module| module.origin() == SymbolOrigin::Source)
        {
            let surface = binder
                .resolve_symbol_query(SymbolQueryRequest::<ModuleSurfaceQuery>::new(module.id()))
                .map_err(crate::compilation::binder::binding_query_error)?;

            sources.push(surface.diagnostics().clone());
        }

        let mut callables_without_bodies = Vec::new();
        let mut callable_order = BTreeMap::new();
        let contract_start = sources.len();
        let mut checked_contracts = BTreeMap::new();

        for (ordinal, callable) in source_graph
            .declarations()
            .declarations()
            .iter()
            .filter_map(|declaration| symbols.symbol_for_declaration(declaration.id()))
            .filter_map(CallableSymbolId::try_from_any)
            .enumerate()
        {
            callable_order.insert(callable, ordinal);
            sources.push(DiagnosticBag::new());

            let body = match CallableDefinitionId::try_new(callable.into_any()) {
                Some(definition) => self.callable_body_key(definition)?,
                None => None,
            };

            if body.is_none() {
                callables_without_bodies.push((ordinal, callable));
            }
        }

        let contract_results =
            self.state
                .fact_runtime
                .map_indexed(callables_without_bodies.len(), |index| {
                    cancellation.check()?;

                    let (ordinal, callable) = callables_without_bodies[index];

                    let contracts = binder
                        .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                            callable,
                        ))
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    // Each worker returns its bag without retaining the callable analysis.
                    Ok::<_, FactQueryError>((ordinal, callable, contracts))
                })?;

        for contracts in contract_results {
            let (ordinal, callable, contracts) = contracts?;

            sources[contract_start + ordinal] = contracts.diagnostics().clone();
            checked_contracts.insert(callable, contracts);
        }

        for subject in symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| NamedTypeSymbolId::from(symbol.id()))
            .chain(
                symbols
                    .unions()
                    .iter()
                    .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                    .map(|symbol| NamedTypeSymbolId::from(symbol.id())),
            )
        {
            sources.push(
                self.declared_type_representation(subject)?
                    .diagnostics()
                    .clone(),
            );
        }

        for implementation in symbols
            .unnamed_trait_implementations()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| ImplementationSymbolId::from(symbol.id()))
            .chain(
                symbols
                    .named_trait_implementations()
                    .iter()
                    .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                    .map(|symbol| ImplementationSymbolId::from(symbol.id())),
            )
        {
            let coherence = binder
                .resolve_symbol_query(SymbolQueryRequest::<
                    bray_symbols::ImplementationCoherenceQuery,
                >::new(implementation))
                .map_err(crate::compilation::binder::binding_query_error)?;

            if coherence.diagnostics().has_errors() {
                // Diagnostic publication owns this source bag after the query result is released.
                sources.push(coherence.diagnostics().clone());

                continue;
            }

            sources.push(
                self.trait_implementation_conformance(implementation)?
                    .diagnostics()
                    .clone(),
            );
        }

        // Scheduled diagnostics retain shared identities independently of the inventory.
        let mut roots = self
            .declared_unit_keys()?
            .iter()
            .cloned()
            .map(unit_order_key)
            .collect::<Vec<_>>();

        // Larger units usually take longer to check. Starting them first keeps
        // the longest work from trailing the batch. Publication restores
        // source order below.
        roots.sort_by_key(|(_, range, _, _)| std::cmp::Reverse(range.len()));

        let mut units = self
            .state
            .fact_runtime
            .complete_batch(roots, cancellation, |(_, _, _, key)| {
                // Contract and unit diagnostics reuse this worker's checked body before eviction.
                let contracts = if key.kind() == BoundUnitKind::CallableBody {
                    let callable = symbols
                        .symbol_for_key(key.declared_owner())
                        .and_then(CallableSymbolId::try_from_any)
                        .expect("declared callable body must have a callable owner");

                    let output = self.published_source_output(key, cancellation)?;

                    let contracts = match output
                        .and_then(|output| output.callable_contract.as_ref().map(Arc::clone))
                    {
                        Some(contracts) => contracts,
                        None => binder
                            .resolve_symbol_query(
                                SymbolQueryRequest::<CallableContractsQuery>::new(callable),
                            )
                            .map_err(crate::compilation::binder::binding_query_error)?,
                    };

                    let ordinal = *callable_order
                        .get(&callable)
                        .expect("declared callable body must have a diagnostic publication order");

                    Some((ordinal, callable, contracts))
                } else {
                    None
                };

                let (sources, nested) = self.semantic_unit_diagnostic_work(key, cancellation)?;

                Ok::<_, FactQueryError>(BatchWork::new((contracts, sources), nested))
            })
            .map_err(|error| error.into_fact_query_error())?;

        units.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        for (_, (contracts, unit_sources)) in units {
            if let Some((ordinal, callable, contracts)) = contracts {
                sources[contract_start + ordinal] = contracts.diagnostics().clone();
                checked_contracts.insert(callable, contracts);
            }

            sources.extend(unit_sources);
        }

        let coherence = self.implementation_coherence_diagnostics(cancellation)?;
        let execution_guarantees = self.execution_guarantee_diagnostics()?;
        let callable_overloads = self.callable_overload_diagnostics(cancellation)?;
        let foreign_callables = self.foreign_callable_diagnostics(cancellation)?;
        let product = self.product_semantics_with_cancellation(cancellation)?;

        let mut bags = std::iter::once(source_graph.diagnostics())
            .chain(sources.iter())
            .chain([
                coherence,
                execution_guarantees,
                callable_overloads,
                foreign_callables,
                product.diagnostics(),
            ])
            .collect::<Vec<_>>();

        // Incomplete binding must not turn absent reference data into unused-import warnings.
        let unused_usings = if !self.syntax_tree_result().diagnostics().has_errors()
            && bags.iter().all(|bag| !bag.has_errors())
        {
            self.unused_using_diagnostics(cancellation)?
        } else {
            DiagnosticBag::new()
        };

        bags.push(&unused_usings);

        let collections = ordered_diagnostic_collections(bags);

        Ok(SemanticDiagnosticPublication {
            diagnostics: publish_diagnostics(self.state.fact_runtime.profile(), collections),
            callable_contracts: checked_contracts,
        })
    }
}

pub(super) fn ordered_diagnostic_collections<'diagnostic>(
    diagnostics: impl IntoIterator<Item = &'diagnostic DiagnosticBag>,
) -> Vec<OrderedDiagnosticCollection> {
    diagnostics
        .into_iter()
        .enumerate()
        .map(|(ordinal, diagnostics)| {
            OrderedDiagnosticCollection::new(DiagnosticPublicationOrder::new(ordinal), diagnostics)
        })
        .collect()
}
