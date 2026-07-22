use std::sync::Arc;

use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AvailableCompilerKnownSymbols, ImplementationCoherenceDomainKey,
    ImplementationParticipationEvidence, ImplementationParticipationFact,
    ImplementationParticipationSet, ImplementationSymbolId, ParticipatingImplementation,
    SemanticFactResult, SymbolOrigin,
};

use super::super::Compilation;
use crate::fact::{CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns the implementations participating in one package coherence domain.
    pub fn implementation_participation(
        &self,
        domain: ImplementationCoherenceDomainKey,
    ) -> Result<Arc<SemanticFactResult<ImplementationParticipationFact>>, FactQueryError> {
        self.implementation_participation_with_cancellation(domain, &self.state.cancellation)
    }

    pub(in crate::compilation) fn implementation_participation_with_cancellation(
        &self,
        domain: ImplementationCoherenceDomainKey,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<Arc<SemanticFactResult<ImplementationParticipationFact>>, FactQueryError> {
        if domain.package() != self.package_identity() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        // The cache and runtime each own the domain identity after this request returns.
        let cell = self
            .state
            .implementation_participation
            .cell(domain.clone())?;

        let fact_key = CompilationFactKey::ImplementationParticipation(domain.clone());

        let result =
            cell.get_or_compute(&self.state.fact_runtime, fact_key, cancellation, || {
                self.compute_implementation_participation(domain, cancellation)
                    .map(Arc::new)
            })?;

        // The caller owns the immutable publication independently of the map cell.
        Ok(Arc::clone(result))
    }

    fn compute_implementation_participation(
        &self,
        domain: ImplementationCoherenceDomainKey,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<SemanticFactResult<ImplementationParticipationFact>, FactQueryError> {
        cancellation.check()?;

        let symbols = self.symbol_graph()?;
        let available_compiler_known = self.available_compiler_known_symbols();
        let mut participating = Vec::new();

        let implementations = symbols
            .unnamed_trait_implementations()
            .iter()
            .map(|implementation| {
                (
                    implementation.key(),
                    ImplementationSymbolId::from(implementation.id()),
                    implementation.origin(),
                )
            })
            .chain(
                symbols
                    .named_trait_implementations()
                    .iter()
                    .map(|implementation| {
                        (
                            implementation.key(),
                            ImplementationSymbolId::from(implementation.id()),
                            implementation.origin(),
                        )
                    }),
            );

        for (key, implementation, origin) in implementations {
            cancellation.check()?;

            let Some(evidence) =
                participation_evidence(origin, implementation, available_compiler_known)
            else {
                continue;
            };

            // The published fact owns its stable key independently of the symbol graph borrow.
            let participant =
                ParticipatingImplementation::try_new(key.clone(), implementation, evidence)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

            participating.push(participant);
        }

        // TODO(compilation): Include imported named implementations when explicit using binding
        // publishes their resolved identities and syntax anchors.
        let participation = ImplementationParticipationSet::try_new(domain, participating)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(DiagnosticResult::without_diagnostics(participation))
    }
}

fn participation_evidence(
    origin: SymbolOrigin,
    implementation: ImplementationSymbolId,
    available_compiler_known: &AvailableCompilerKnownSymbols,
) -> Option<ImplementationParticipationEvidence> {
    match origin {
        SymbolOrigin::Source => Some(ImplementationParticipationEvidence::declared()),
        SymbolOrigin::CompilerKnown
            if available_compiler_known.contains(implementation.into_any()) =>
        {
            Some(ImplementationParticipationEvidence::compiler_known())
        }
        SymbolOrigin::CompilerKnown => None,
        SymbolOrigin::Imported | SymbolOrigin::CompilerProvided | SymbolOrigin::Synthesized => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{
        ImplementationCoherenceDomainKey, ImplementationParticipationEvidence,
        ImplementationSymbolId,
    };

    use crate::fact::{CompilationFactKey, FactCellTestEvent};
    use crate::test_support::{
        FactTestGate, compilation, encoded_semantic_dependency, package_identity, source_input,
    };
    use crate::{Compilation, CompilationRequest};

    const IMPLEMENTATIONS: &str = r#"module app;

trait Provides
{
}

struct First
{
}

struct Second
{
}

impl FirstProvides = First(Provides)
{
}

impl Second(Provides)
{
}

impl First
{
}
"#;

    #[test]
    fn participation_includes_supported_origins_in_canonical_order() {
        let compilation = compilation(IMPLEMENTATIONS);
        let domain = domain();

        let result = compilation
            .implementation_participation(domain.clone())
            .unwrap_or_else(|error| panic!("participation fact must publish: {error:?}"));

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().domain(), &domain);

        let implementations = result.value().implementations();

        assert!(
            implementations
                .windows(2)
                .all(|pair| pair[0].key() < pair[1].key())
        );

        assert_eq!(
            implementations
                .iter()
                .filter(|implementation| {
                    implementation.evidence() == &ImplementationParticipationEvidence::declared()
                })
                .count(),
            2
        );

        assert!(implementations.iter().any(|implementation| {
            implementation.evidence() == &ImplementationParticipationEvidence::compiler_known()
        }));

        assert!(implementations.iter().all(|implementation| {
            !matches!(
                implementation.implementation(),
                ImplementationSymbolId::Inherent(_)
            )
        }));
    }

    #[test]
    fn participation_rejects_an_unselected_coherence_domain() {
        let compilation = compilation(IMPLEMENTATIONS);

        let package = bray_symbols::PackageIdentity::try_new("other.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        assert!(
            compilation
                .implementation_participation(ImplementationCoherenceDomainKey::new(package))
                .is_err()
        );
    }

    #[test]
    fn repeated_and_concurrent_requests_share_one_publication() {
        let compilation = Arc::new(compilation(IMPLEMENTATIONS));
        let domain = domain();
        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        compilation
            .state
            .implementation_participation
            .cell(domain.clone())
            .unwrap_or_else(|error| panic!("participation cell must exist: {error:?}"))
            .set_test_observer(gate.observer())
            .unwrap_or_else(|error| panic!("participation observer must attach: {error:?}"));

        let (first, second) = std::thread::scope(|scope| {
            let first_compilation = Arc::clone(&compilation);
            let first_domain = domain.clone();

            let first =
                scope.spawn(move || first_compilation.implementation_participation(first_domain));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let second_compilation = Arc::clone(&compilation);

            let second =
                scope.spawn(move || second_compilation.implementation_participation(domain));

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            (
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first participation thread must not panic")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second participation thread must not panic")),
            )
        });

        let first = first.unwrap_or_else(|error| panic!("first request must succeed: {error:?}"));

        let second =
            second.unwrap_or_else(|error| panic!("second request must succeed: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn compiler_known_participants_follow_the_selected_target() {
        let compilation = compilation("module app;");
        let domain = domain();

        let result = compilation
            .implementation_participation(domain.clone())
            .unwrap_or_else(|error| panic!("participation fact must publish: {error:?}"));

        assert!(!result.value().implementations().is_empty());

        let available = compilation.available_compiler_known_symbols();

        for participant in result.value().implementations() {
            assert_eq!(
                participant.evidence(),
                &ImplementationParticipationEvidence::compiler_known()
            );

            assert!(available.contains(participant.implementation().into_any()));
        }

        let dependencies = compilation
            .state
            .fact_runtime
            .dependencies(&CompilationFactKey::ImplementationParticipation(domain))
            .unwrap_or_else(|error| {
                panic!("participation dependencies must be readable: {error:?}")
            })
            .unwrap_or_else(|| panic!("participation fact must be published"));

        assert!(dependencies.contains(&CompilationFactKey::SelectedTarget));
    }

    #[test]
    fn dependency_interfaces_do_not_contribute_or_get_demanded() {
        let fixture = bray_package_interface::test_support::encoded_semantic_test_interface();

        let request =
            CompilationRequest::new(package_identity(), vec![source_input("module app;", 0)])
                .with_dependency_interfaces([encoded_semantic_dependency(&fixture)]);

        let with_dependency = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let baseline = compilation("module app;")
            .implementation_participation(domain())
            .unwrap_or_else(|error| panic!("baseline participation must publish: {error:?}"));

        assert!(
            with_dependency
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );

        let result = with_dependency
            .implementation_participation(domain())
            .unwrap_or_else(|error| panic!("participation fact must publish: {error:?}"));

        assert_eq!(result.as_ref(), baseline.as_ref());

        assert!(
            with_dependency
                .state
                .loaded_dependency_interfaces
                .iter()
                .all(|interface| interface.get().is_none())
        );
    }

    fn domain() -> ImplementationCoherenceDomainKey {
        ImplementationCoherenceDomainKey::new(package_identity())
    }
}
