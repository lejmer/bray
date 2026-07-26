use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    GenericDeclarationTemplateFact, GenericOwnerId, ImplementationCandidate,
    ImplementationCandidateSet, ImplementationCoherenceDomainKey, ImplementationCoherenceEvidence,
    ImplementationCoherenceFact, ImplementationCoherenceParticipant,
    ImplementationHeadTemplateFact, ImplementationRequirementKey, SymbolFactRequest,
};

use super::index::{ImplementationHeader, ImplementationHeaderIndex};
use super::matching::{ImplementationMatchError, match_implementation_header};
use crate::compilation::source_graph::{
    source_declaration_module_parts, source_symbol_contribution_gate,
};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl super::super::Compilation {
    /// Returns uncommitted implementation candidates for one exact requirement.
    pub fn implementation_candidate_set_result(
        &self,
        key: ImplementationRequirementKey,
    ) -> Result<Arc<DiagnosticResult<ImplementationCandidateSet>>, FactQueryError> {
        self.implementation_candidate_set_result_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn implementation_candidate_set_result_with_cancellation(
        &self,
        key: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<ImplementationCandidateSet>>, FactQueryError> {
        let cell = self.state.implementation_candidate_sets.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImplementationCandidateSet(key),
            cancellation,
            || {
                self.compute_implementation_candidate_set(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        // Exact results are Arc-backed so callers do not retain the cache-cell map lock.
        Ok(Arc::clone(result))
    }

    pub(super) fn implementation_header_index(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticResult<Arc<ImplementationHeaderIndex>>, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::ImplementationHeaderIndex,
            &self.state.implementation_index,
            cancellation,
            |cancellation| self.compute_implementation_header_index(cancellation),
        )
    }

    fn compute_implementation_header_index(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Arc<ImplementationHeaderIndex>>, FactQueryError> {
        let values = self.semantic_value_store()?;
        let facts = self.binder_facts(cancellation)?;
        let source_graph = self.product_source_graph()?;
        let declarations = source_graph.declarations();
        let symbols = self.symbol_graph()?;

        let source_module_parts = source_declaration_module_parts(declarations);

        // The coherence-domain key owns its package identity beyond this compilation borrow.
        let domain = ImplementationCoherenceDomainKey::new(self.package_identity().clone());

        let participation =
            self.implementation_participation_with_cancellation(domain, cancellation)?;

        let mut headers = Vec::new();

        for participant in participation.value().implementations() {
            cancellation.check()?;

            let implementation = participant.implementation();

            if let Some(address) = facts
                .imported_fact_address(implementation.into_any())
                .map_err(super::super::binder::binder_fact_error)?
            {
                let imported = super::super::binder::imported_implementation(&facts, address)
                    .map_err(super::super::binder::binder_fact_error)?;

                let owner = GenericOwnerId::try_new(implementation.into_any())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let generic = facts
                    .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                        owner,
                    ))
                    .map_err(super::super::binder::binder_fact_error)?;

                let Some(trait_application) = imported.value().trait_application() else {
                    continue;
                };

                // The generic template reuses the exact implementation publication and diagnostics.
                let diagnostics = generic.diagnostics().clone();

                // Header records retain shallow Arc-backed keys, templates, and target values independently.
                headers.push(ImplementationHeader::new(
                    participant.key().clone(),
                    implementation,
                    imported.value().subject().ty(),
                    trait_application,
                    generic.value().clone(),
                    imported.value().target_dependencies().iter().cloned(),
                    diagnostics,
                ));

                continue;
            }

            let contribution_gate = source_symbol_contribution_gate(
                source_graph,
                symbols,
                &source_module_parts,
                implementation.into_any(),
            );

            let head = facts
                .symbol_fact(SymbolFactRequest::<ImplementationHeadTemplateFact>::new(
                    implementation,
                ))
                .map_err(super::super::binder::binder_fact_error)?;

            let coherence = facts
                .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                    implementation,
                ))
                .map_err(super::super::binder::binder_fact_error)?;

            let diagnostics =
                DiagnosticBag::merged_all([head.diagnostics(), coherence.diagnostics()]);

            let Some(trait_application) = coherence.value().trait_application() else {
                continue;
            };

            // The index owns the immutable generic template beyond the borrowed fact result.
            let generic = head.value().generic().clone();

            // Headers retain their stable key and Arc-backed target dependencies.
            headers.push(ImplementationHeader::new(
                participant.key().clone(),
                implementation,
                coherence.value().subject(),
                trait_application,
                generic,
                contribution_gate
                    .into_iter()
                    .flat_map(|gate| gate.dependencies().iter().cloned()),
                diagnostics,
            ));
        }

        let diagnostics = DiagnosticBag::merged_all(
            headers
                .iter()
                .map(ImplementationHeader::diagnostics)
                .chain([participation.diagnostics()]),
        );

        let index = ImplementationHeaderIndex::try_new(headers, values)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(DiagnosticResult::new(Arc::new(index), diagnostics))
    }

    fn compute_implementation_candidate_set(
        &self,
        key: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ImplementationCandidateSet>, FactQueryError> {
        let index = self.implementation_header_index(cancellation)?;
        let values = self.semantic_value_store()?;

        let trait_application = values
            .trait_application_data(key.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let compatible = index
            .value()
            .compatible_headers(key.subject(), trait_application.definition(), values)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let diagnostics =
            DiagnosticBag::merged_all(compatible.iter().map(|header| header.diagnostics()));

        let mut matched = Vec::new();

        for header in compatible {
            cancellation.check()?;

            if !self.target_dependencies_hold(header.target_dependencies())? {
                continue;
            }

            let substitution = match match_implementation_header(
                header,
                key.subject(),
                key.trait_application(),
                values,
            ) {
                Ok(Some(substitution)) => substitution,
                Ok(None) => continue,
                Err(ImplementationMatchError::InvalidSubstitution)
                | Err(ImplementationMatchError::SemanticValue) => {
                    return Err(FactQueryError::InfrastructureFailure);
                }
            };

            matched.push((header, substitution));
        }

        let candidates = if matched.is_empty() {
            ImplementationCandidateSet::try_new(key, [])
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        } else {
            let coherence = ImplementationCoherenceEvidence::try_new(
                key,
                matched.iter().map(|(header, _)| {
                    // Coherence evidence owns stable keys beyond the borrowed index traversal.
                    ImplementationCoherenceParticipant::new(
                        header.key().clone(),
                        header.implementation(),
                    )
                }),
            )
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let candidates = matched.into_iter().map(|(header, substitution)| {
                // Candidate records independently retain Arc-backed declaration evidence.
                ImplementationCandidate::try_new(
                    header.key().clone(),
                    header.implementation(),
                    substitution,
                    header.constraints().iter().copied(),
                    header.target_dependencies().iter().cloned(),
                    coherence.clone(),
                )
            });

            let candidates = candidates
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            ImplementationCandidateSet::try_new(key, candidates)
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        };

        Ok(DiagnosticResult::new(candidates, diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
    use bray_symbols::{
        GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        ImplementationCoherenceDomainKey, ImplementationRequirementKey, ImplementationSelection,
        ImplementationSymbolId, NamedTraitImplementationSymbolId, NamedTypeSymbolId,
        StructSymbolId, SymbolOrigin, TraitApplicationData, TypeData,
    };
    use bray_target::TargetFactKind;

    use crate::fact::CompilationFactKey;
    use crate::test_support::{
        compilation, encoded_semantic_dependency, package_identity, source_input,
    };
    use crate::{CancellationToken, Compilation, CompilationRequest};

    const IMPLEMENTATIONS: &str = r#"module app;

trait Converts<T>
{
}

struct Wrapper<T>
{
}

impl WrapperConverts = Wrapper<T>(Converts<T>) with(true)
{
}
"#;

    #[test]
    fn exact_queries_specialize_generic_headers_and_preserve_constraints() {
        let compilation = compilation(IMPLEMENTATIONS);
        let fixture = CandidateFixture::new(&compilation);

        let first = compilation
            .implementation_candidate_set_result(fixture.requirement)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        let second = compilation
            .implementation_candidate_set_result(fixture.requirement)
            .unwrap_or_else(|error| panic!("candidate query must remain available: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let [candidate] = first.value().candidates() else {
            panic!("exact generic implementation must produce one candidate");
        };

        assert_eq!(candidate.implementation(), fixture.implementation);
        assert_eq!(candidate.constraints().len(), 1);
        assert!(candidate.target_dependencies().is_empty());
        assert_eq!(candidate.coherence().key(), fixture.requirement);

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let substitution = values
            .generic_substitution_data(candidate.substitution())
            .unwrap_or_else(|error| panic!("candidate substitution must resolve: {error:?}"));

        assert_eq!(
            substitution.argument_for(fixture.implementation_parameter),
            Some(GenericArgument::Type(fixture.boolean))
        );
    }

    #[test]
    fn implementation_selection_proves_constraints_before_committing_a_witness() {
        let accepted = compilation(IMPLEMENTATIONS);
        let fixture = CandidateFixture::new(&accepted);

        let first = accepted
            .implementation_selection_result(fixture.requirement)
            .unwrap_or_else(|error| panic!("implementation selection must complete: {error:?}"));

        let second = accepted
            .implementation_selection_result(fixture.requirement)
            .unwrap_or_else(|error| {
                panic!("implementation selection must remain available: {error:?}")
            });

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let ImplementationSelection::Selected(instance) = first.value() else {
            panic!("satisfied implementation constraints must select one witness");
        };

        let values = accepted
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let instance = values
            .implementation_instance_data(*instance)
            .unwrap_or_else(|error| panic!("selected witness must be available: {error:?}"));

        assert_eq!(instance.definition(), fixture.implementation);

        let rejected = compilation(&IMPLEMENTATIONS.replace("with(true)", "with(false)"));
        let rejected_fixture = CandidateFixture::new(&rejected);

        let selection = rejected
            .implementation_selection_result(rejected_fixture.requirement)
            .unwrap_or_else(|error| panic!("rejected selection must complete: {error:?}"));

        assert_eq!(*selection.value(), ImplementationSelection::Unavailable);
    }

    #[test]
    fn source_headers_retain_target_dependencies_for_applicability() {
        let source = target_gated_implementations("target.scalar.u64");
        let compilation = compilation(&source);
        let fixture = CandidateFixture::new(&compilation);

        let candidates = compilation
            .implementation_candidate_set_result(fixture.requirement)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        let [candidate] = candidates.value().candidates() else {
            panic!("enabled target-gated implementation must produce one candidate");
        };

        let [dependency] = candidate.target_dependencies() else {
            panic!("target-gated implementation must retain one target dependency");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .provider()
                .symbol_target_fact(dependency.fact()),
            Some(TargetFactKind::ScalarU64)
        );
    }

    #[test]
    fn target_disabled_implementations_do_not_enter_active_source_identity() {
        let source = target_gated_implementations("target.atomic.u64");
        let compilation = compilation(&source);
        let requirement = candidate_requirement(&compilation);

        let candidates = compilation
            .implementation_candidate_set_result(requirement.key)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        assert!(candidates.diagnostics().is_empty());
        assert!(candidates.value().candidates().is_empty());

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        assert!(
            symbols
                .named_trait_implementations()
                .iter()
                .all(|symbol| symbol.origin() != SymbolOrigin::Source)
        );
    }

    #[test]
    fn exact_queries_are_independently_cached_and_concurrently_stable() {
        let compilation = compilation(IMPLEMENTATIONS);
        let fixture = CandidateFixture::new(&compilation);

        let unrelated = ImplementationRequirementKey::new(
            fixture.boolean,
            fixture.requirement.trait_application(),
        );

        let results = std::thread::scope(|scope| {
            let first = scope
                .spawn(|| compilation.implementation_candidate_set_result(fixture.requirement));

            let second = scope
                .spawn(|| compilation.implementation_candidate_set_result(fixture.requirement));

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("candidate query thread must not panic"))
                    .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"))
            })
        });

        assert!(Arc::ptr_eq(&results[0], &results[1]));

        let unrelated = compilation
            .implementation_candidate_set_result(unrelated)
            .unwrap_or_else(|error| panic!("unrelated candidate query must complete: {error:?}"));

        assert!(unrelated.value().candidates().is_empty());
        assert_eq!(results[0].value().candidates().len(), 1);
    }

    #[test]
    fn cancelled_candidate_queries_do_not_publish_partial_results() {
        let compilation = compilation(IMPLEMENTATIONS);
        let fixture = CandidateFixture::new(&compilation);
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        assert_eq!(
            compilation.implementation_candidate_set_result_with_cancellation(
                fixture.requirement,
                &cancellation,
            ),
            Err(crate::FactQueryError::Cancelled)
        );

        assert_eq!(
            compilation
                .state
                .implementation_candidate_sets
                .is_published(&fixture.requirement),
            Ok(false)
        );
    }

    #[test]
    fn compiler_known_implementation_headers_participate_by_availability() {
        let compilation = compilation("module app;");

        let Some(key) = CompilerKnownDeclarationKey::try_new("HeapStorageImplementation") else {
            panic!("HeapStorageImplementation must be a valid declaration key");
        };

        let Some(implementation) = compilation
            .available_compiler_known_symbols()
            .declaration_symbol::<NamedTraitImplementationSymbolId>(&key)
        else {
            panic!("catalog must expose the HeapStorage implementation");
        };

        let implementation = ImplementationSymbolId::NamedTrait(implementation);

        let domain = ImplementationCoherenceDomainKey::new(compilation.package_identity().clone());

        let participation = compilation
            .implementation_participation(domain)
            .unwrap_or_else(|error| {
                panic!("implementation participation must complete: {error:?}")
            });

        assert!(
            participation
                .value()
                .implementations()
                .iter()
                .any(|participant| participant.implementation() == implementation)
        );
    }

    #[test]
    fn candidate_indexes_depend_on_participation_without_demanding_dependency_interfaces() {
        let interface = bray_package_interface::test_support::encoded_semantic_test_interface();

        let request =
            CompilationRequest::new(package_identity(), vec![source_input(IMPLEMENTATIONS, 0)])
                .with_dependency_interfaces([encoded_semantic_dependency(&interface)]);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let fixture = CandidateFixture::new(&compilation);
        let domain = ImplementationCoherenceDomainKey::new(package_identity());

        assert_eq!(
            compilation
                .state
                .implementation_participation
                .is_published(&domain),
            Ok(false)
        );

        assert!(
            compilation
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );

        let candidates = compilation
            .implementation_candidate_set_result(fixture.requirement)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        assert_eq!(candidates.value().candidates().len(), 1);

        assert_eq!(
            compilation
                .state
                .implementation_participation
                .is_published(&domain),
            Ok(true)
        );

        assert!(
            compilation
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );

        let dependencies = compilation
            .state
            .fact_runtime
            .dependencies(&CompilationFactKey::ImplementationHeaderIndex)
            .unwrap_or_else(|error| panic!("index dependencies must be readable: {error:?}"))
            .unwrap_or_else(|| panic!("implementation index must be published"));

        assert!(dependencies.contains(&CompilationFactKey::ImplementationParticipation(domain)));
    }

    struct CandidateFixture {
        requirement: ImplementationRequirementKey,
        implementation: ImplementationSymbolId,
        implementation_parameter: GenericParameterSymbolId,
        boolean: bray_symbols::TypeId,
    }

    impl CandidateFixture {
        fn new(compilation: &crate::Compilation) -> Self {
            let requirement = candidate_requirement(compilation);

            let symbols = compilation
                .symbol_graph()
                .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

            let implementations = symbols
                .named_trait_implementations()
                .iter()
                .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                .collect::<Vec<_>>();

            let [implementation] = implementations.as_slice() else {
                panic!("fixture must declare one named trait implementation: {implementations:?}");
            };

            let [implementation_parameter] = implementation.generic_type_parameters() else {
                panic!("fixture implementation must infer one type parameter");
            };

            Self {
                requirement: requirement.key,
                implementation: implementation.id().into(),
                implementation_parameter: GenericParameterSymbolId::Type(*implementation_parameter),
                boolean: requirement.boolean,
            }
        }
    }

    struct CandidateRequirement {
        key: ImplementationRequirementKey,
        boolean: bray_symbols::TypeId,
    }

    fn candidate_requirement(compilation: &crate::Compilation) -> CandidateRequirement {
        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let structures = symbols
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [structure] = structures.as_slice() else {
            panic!("fixture must declare one source structure: {structures:?}");
        };

        let traits = symbols
            .traits()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [trait_symbol] = traits.as_slice() else {
            panic!("fixture must declare one source trait: {traits:?}");
        };

        let boolean_symbol = symbols
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(RepresentationRole::ScalarBool)
            .unwrap_or_else(|| panic!("compiler-known bool must be available"));

        let boolean = named_type(values, boolean_symbol, [], []);

        let subject = named_type(
            values,
            structure.id(),
            structure
                .generic_type_parameters()
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
            [GenericArgument::Type(boolean)],
        );

        let trait_substitution = substitution(
            values,
            trait_symbol.id().into(),
            trait_symbol
                .generic_type_parameters()
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
            [GenericArgument::Type(boolean)],
        );

        let trait_application = values
            .intern_trait_application(TraitApplicationData::new(
                trait_symbol.id(),
                trait_substitution,
            ))
            .unwrap_or_else(|error| panic!("trait application must be valid: {error:?}"));

        CandidateRequirement {
            key: ImplementationRequirementKey::new(subject, trait_application),
            boolean,
        }
    }

    fn named_type(
        values: &bray_symbols::SemanticValueStore,
        definition: StructSymbolId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        arguments: impl IntoIterator<Item = GenericArgument>,
    ) -> bray_symbols::TypeId {
        let substitution = substitution(values, definition.into(), parameters, arguments);

        values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("named type must be valid: {error:?}"))
    }

    fn substitution(
        values: &bray_symbols::SemanticValueStore,
        owner: bray_symbols::AnySymbolId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        arguments: impl IntoIterator<Item = GenericArgument>,
    ) -> bray_symbols::GenericSubstitutionId {
        let owner = GenericOwnerId::try_new(owner)
            .unwrap_or_else(|| panic!("fixture substitution owner must be generic"));

        let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
            .unwrap_or_else(|error| panic!("fixture substitution must be valid: {error:?}"));

        values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("fixture substitution must be interned: {error:?}"))
    }

    fn target_gated_implementations(target_fact: &str) -> String {
        format!(
            r#"@target({target_fact})
module app
{{
    impl WrapperConverts = Wrapper<T>(Converts<T>) with(true)
    {{
    }}
}}

module app
{{
    trait Converts<T>
    {{
    }}

    struct Wrapper<T>
    {{
    }}
}}
"#
        )
    }
}
