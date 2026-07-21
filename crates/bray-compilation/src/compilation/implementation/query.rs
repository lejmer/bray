use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    GenericConstraintTemplate, GenericParameterSymbolId, ImplementationCandidate,
    ImplementationCandidateSet, ImplementationCoherenceEvidence, ImplementationCoherenceFact,
    ImplementationCoherenceParticipant, ImplementationHeadTemplateFact,
    ImplementationRequirementKey, ImplementationSymbolId, ImportedInterfaceId,
    ImportedSymbolSkeleton, SymbolFactRequest, SymbolGraph, SymbolKey,
};

use super::index::{ImplementationHeader, ImplementationHeaderIndex};
use super::matching::{ImplementationMatchError, match_implementation_header};
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

    fn implementation_header_index(
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
        let symbols = self.symbol_graph()?;
        let values = self.semantic_value_store()?;
        let facts = self.binder_facts(cancellation)?;

        let mut headers = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for implementation in local_trait_implementations(self, symbols) {
            cancellation.check()?;

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

            diagnostics = diagnostics
                .merged(head.diagnostics())
                .merged(coherence.diagnostics());

            let Some(trait_application) = coherence.value().trait_application() else {
                continue;
            };

            let key = symbols
                .symbol_key(implementation.into_any())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            // Header records retain their stable key independently of the symbol graph snapshot.
            headers.push(ImplementationHeader::new(
                key.clone(),
                implementation,
                coherence.value().subject(),
                trait_application,
                head.value().generic().parameters().iter().copied(),
                head.value().generic().constraints().iter().copied(),
                [],
            ));
        }

        collect_imported_headers(self, cancellation, &mut headers, &mut diagnostics)?;

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

        let mut matched = Vec::new();

        for header in compatible {
            cancellation.check()?;

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

        // The exact result retains diagnostics independently of the shared index fact.
        let diagnostics = index.diagnostics().clone();

        Ok(DiagnosticResult::new(candidates, diagnostics))
    }
}

fn local_trait_implementations(
    compilation: &super::super::Compilation,
    symbols: &SymbolGraph,
) -> BTreeSet<ImplementationSymbolId> {
    let source = symbols
        .unnamed_trait_implementations()
        .iter()
        .filter(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
        .map(|symbol| ImplementationSymbolId::from(symbol.id()))
        .chain(
            symbols
                .named_trait_implementations()
                .iter()
                .filter(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
                .map(|symbol| ImplementationSymbolId::from(symbol.id())),
        );

    let compiler_known = compilation
        .available_compiler_known_symbols()
        .declarations()
        .iter()
        .copied()
        .filter_map(ImplementationSymbolId::try_from_any)
        .filter(|implementation| !matches!(implementation, ImplementationSymbolId::Inherent(_)));

    source.chain(compiler_known).collect()
}

fn collect_imported_headers(
    compilation: &super::super::Compilation,
    cancellation: &CancellationToken,
    headers: &mut Vec<ImplementationHeader>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), FactQueryError> {
    let skeleton_result =
        compilation.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

    *diagnostics = diagnostics.merged(skeleton_result.diagnostics());

    let Some(skeleton) = skeleton_result.value() else {
        return Ok(());
    };

    for index in 0..compilation.state.dependency_interfaces.len() {
        cancellation.check()?;

        let interface = ImportedInterfaceId::try_from_index(index)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(result) = compilation
            .imported_semantic_graph_result_with_cancellation(interface, cancellation)?
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        *diagnostics = diagnostics.merged(result.diagnostics());

        let Some(facts) = result.value() else {
            continue;
        };

        for implementation in facts.implementations() {
            let Some(trait_application) = implementation.trait_application() else {
                continue;
            };

            let subject = implementation.subject().ty();
            let implementation = implementation.implementation();

            let key = imported_implementation_key(skeleton, implementation)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let parameters = imported_implementation_parameters(skeleton, implementation)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let owner = bray_symbols::GenericOwnerId::try_new(implementation.into_any())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let constraints = facts
                .constraints()
                .iter()
                .copied()
                .filter(|constraint| constraint.owner() == owner)
                .map(|constraint| GenericConstraintTemplate::Resolved(constraint.constraint()));

            // The indexed header owns its stable key independently of the imported skeleton.
            let key = key.clone();

            // TODO(BRA-235): Retain implementation-owned target dependencies once interface
            // semantic records associate dependencies with their consuming declaration fact.
            headers.push(ImplementationHeader::new(
                key,
                implementation,
                subject,
                trait_application,
                parameters,
                constraints,
                [],
            ));
        }
    }

    Ok(())
}

fn imported_implementation_key(
    symbols: &ImportedSymbolSkeleton,
    implementation: ImplementationSymbolId,
) -> Option<&SymbolKey> {
    match implementation {
        ImplementationSymbolId::Inherent(id) => symbols
            .inherent_implementation(id)
            .map(|symbol| symbol.key()),
        ImplementationSymbolId::UnnamedTrait(id) => symbols
            .unnamed_trait_implementation(id)
            .map(|symbol| symbol.key()),
        ImplementationSymbolId::NamedTrait(id) => symbols
            .named_trait_implementation(id)
            .map(|symbol| symbol.key()),
    }
}

fn imported_implementation_parameters(
    symbols: &ImportedSymbolSkeleton,
    implementation: ImplementationSymbolId,
) -> Option<Vec<GenericParameterSymbolId>> {
    let (types, constants) = match implementation {
        ImplementationSymbolId::Inherent(id) => {
            let symbol = symbols.inherent_implementation(id)?;

            (
                symbol.generic_type_parameters(),
                symbol.generic_const_parameters(),
            )
        }
        ImplementationSymbolId::UnnamedTrait(id) => {
            let symbol = symbols.unnamed_trait_implementation(id)?;

            (
                symbol.generic_type_parameters(),
                symbol.generic_const_parameters(),
            )
        }
        ImplementationSymbolId::NamedTrait(id) => {
            let symbol = symbols.named_trait_implementation(id)?;

            (
                symbol.generic_type_parameters(),
                symbol.generic_const_parameters(),
            )
        }
    };

    let parameters = types
        .iter()
        .copied()
        .map(GenericParameterSymbolId::Type)
        .chain(
            constants
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Const),
        )
        .collect::<Vec<_>>();

    let mut parameters = parameters
        .into_iter()
        .map(|parameter| {
            let ordinal = match parameter {
                GenericParameterSymbolId::Type(id) => symbols.generic_type_parameter(id)?.ordinal(),
                GenericParameterSymbolId::Const(id) => {
                    symbols.generic_const_parameter(id)?.ordinal()
                }
            };

            Some((ordinal, parameter))
        })
        .collect::<Option<Vec<_>>>()?;

    parameters.sort_by_key(|(ordinal, _)| *ordinal);

    Some(
        parameters
            .into_iter()
            .map(|(_, parameter)| parameter)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::SymbolFactProvider;
    use bray_compiler_known::RepresentationRole;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_implementation_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{
        GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        ImplementationCoherenceFact, ImplementationHeadTemplateFact, ImplementationRequirementKey,
        ImplementationSymbolId, NamedTypeSymbolId, PackageIdentity, StructSymbolId,
        SymbolFactRequest, SymbolOrigin, TraitApplicationData, TypeData,
    };

    use crate::test_support::compilation;
    use crate::{CancellationToken, Compilation, CompilationRequest, DependencyInterfaceInput};

    const IMPLEMENTATIONS: &str = r#"module app;

trait Converts<T>
{
}

struct Wrapper<T>
{
}

impl Wrapper<T>(Converts<T>) with(true)
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
        let cancellation = CancellationToken::new();

        let facts = compilation
            .binder_facts(&cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        let implementation = compilation
            .available_compiler_known_symbols()
            .declarations()
            .iter()
            .copied()
            .filter_map(ImplementationSymbolId::try_from_any)
            .filter(|implementation| !matches!(implementation, ImplementationSymbolId::Inherent(_)))
            .find(|implementation| {
                facts
                    .symbol_fact(SymbolFactRequest::<ImplementationHeadTemplateFact>::new(
                        *implementation,
                    ))
                    .is_ok_and(|head| head.value().generic().parameters().is_empty())
            })
            .unwrap_or_else(|| panic!("catalog must expose a non-generic trait implementation"));

        let coherence = facts
            .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                implementation,
            ))
            .unwrap_or_else(|error| panic!("coherence fact must be available: {error:?}"));

        let trait_application = coherence
            .value()
            .trait_application()
            .unwrap_or_else(|| panic!("fixture implementation must implement a trait"));

        let requirement =
            ImplementationRequirementKey::new(coherence.value().subject(), trait_application);

        let candidates = compilation
            .implementation_candidate_set_result(requirement)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        assert!(
            candidates
                .value()
                .candidates()
                .iter()
                .any(|candidate| candidate.implementation() == implementation)
        );
    }

    #[test]
    fn imported_implementation_headers_participate_without_source_rebinding() {
        let fixture = encoded_implementation_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product.clone(),
            "implementation.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let package = PackageIdentity::try_new("example.current")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let source = SourceInput::virtual_text(
            SourceIdentity::new(1),
            "main.bray",
            SourceVersion::new(0),
            "module example.current;",
        );

        let compilation = Compilation::load(
            CompilationRequest::new(package, vec![source]).with_dependency_interfaces([dependency]),
        )
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let interface = compilation
            .dependency_interface_id(&fixture.package, &fixture.product)
            .unwrap_or_else(|| panic!("dependency interface must have a stable ID"));

        let cancellation = CancellationToken::new();

        let imported = compilation
            .imported_semantic_graph_result_with_cancellation(interface, &cancellation)
            .unwrap_or_else(|error| panic!("imported semantics must load: {error:?}"))
            .unwrap_or_else(|| panic!("dependency interface must be present"));

        let imported = imported
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("fixture semantic graph must be valid"));

        let [implementation] = imported.implementations() else {
            panic!("fixture must expose one imported implementation");
        };

        let trait_application = implementation
            .trait_application()
            .unwrap_or_else(|| panic!("fixture implementation must implement a trait"));

        let requirement =
            ImplementationRequirementKey::new(implementation.subject().ty(), trait_application);

        let candidates = compilation
            .implementation_candidate_set_result(requirement)
            .unwrap_or_else(|error| panic!("candidate query must complete: {error:?}"));

        let [candidate] = candidates.value().candidates() else {
            panic!("imported implementation must produce one candidate");
        };

        assert_eq!(candidate.implementation(), implementation.implementation());
        assert!(candidate.constraints().is_empty());
    }

    struct CandidateFixture {
        requirement: ImplementationRequirementKey,
        implementation: ImplementationSymbolId,
        implementation_parameter: GenericParameterSymbolId,
        boolean: bray_symbols::TypeId,
    }

    impl CandidateFixture {
        fn new(compilation: &crate::Compilation) -> Self {
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

            let implementations = symbols
                .unnamed_trait_implementations()
                .iter()
                .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
                .collect::<Vec<_>>();

            let [implementation] = implementations.as_slice() else {
                panic!(
                    "fixture must declare one unnamed trait implementation: {implementations:?}"
                );
            };

            let [implementation_parameter] = implementation.generic_type_parameters() else {
                panic!("fixture implementation must infer one type parameter");
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

            Self {
                requirement: ImplementationRequirementKey::new(subject, trait_application),
                implementation: implementation.id().into(),
                implementation_parameter: GenericParameterSymbolId::Type(*implementation_parameter),
                boolean,
            }
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
}
