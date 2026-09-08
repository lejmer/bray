use std::collections::BTreeMap;
use std::sync::Arc;

use bray_codegen::{
    BackendArtifactSet, CodegenFailure, CodegenMappings, CodegenOptions, CodegenStatus,
    CodegenTarget, CodegenUnit, CodegenUnitKey,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_emitter::{BackendContributionMergeError, BackendContributionSet, EmissionPlan};

use super::{CodegenPreparationError, Compilation};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    /// Returns the backend contributions demanded by one immutable emission plan.
    pub fn emission_backend_contributions(
        &self,
        plan: &EmissionPlan,
        units: &[CodegenUnit],
        mappings: &[CodegenMappings],
        target: &CodegenTarget,
        options: &CodegenOptions,
    ) -> Result<DiagnosticResult<BackendContributionSet>, EmissionCodegenError> {
        self.emission_backend_contributions_with_cancellation(
            plan,
            units,
            mappings,
            target,
            options,
            &self.state.cancellation,
        )
    }

    /// Returns planned backend contributions while observing caller cancellation.
    pub fn emission_backend_contributions_with_cancellation(
        &self,
        plan: &EmissionPlan,
        units: &[CodegenUnit],
        mappings: &[CodegenMappings],
        target: &CodegenTarget,
        options: &CodegenOptions,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<BackendContributionSet>, EmissionCodegenError> {
        crate::profile::profile_operation(
            self.state.fact_runtime.profile(),
            crate::profile::ProfileOperation::EmissionCodeGeneration,
            || {
                self.emission_backend_contributions_inner(
                    plan,
                    units,
                    mappings,
                    target,
                    options,
                    cancellation,
                )
            },
            crate::profile::result_outcome,
        )
    }

    fn emission_backend_contributions_inner(
        &self,
        plan: &EmissionPlan,
        units: &[CodegenUnit],
        mappings: &[CodegenMappings],
        target: &CodegenTarget,
        options: &CodegenOptions,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<BackendContributionSet>, EmissionCodegenError> {
        let requests = plan.backend_requests();

        let inputs = CodegenUnitLookup::try_new(units, mappings)
            .map_err(|kind| EmissionCodegenError::new(kind, DiagnosticBag::new()))?;

        let outcomes = self
            .state
            .fact_runtime
            .map_indexed(requests.len(), |index| {
                let request = &requests[index];

                let Some(unit) = inputs.unit(request.unit()) else {
                    return Err(EmissionCodegenErrorKind::MissingUnit(
                        request.unit().clone(),
                    ));
                };

                let Some(mappings) = inputs.mappings(request.unit()) else {
                    return Err(EmissionCodegenErrorKind::MissingMappings(
                        request.unit().clone(),
                    ));
                };

                self.codegen_artifact_with_cancellation(
                    unit,
                    mappings,
                    target,
                    options,
                    request,
                    cancellation,
                )
                .map_err(|error| EmissionCodegenErrorKind::Request {
                    unit: request.unit().clone(),
                    error: Box::new(error),
                })
            })
            .map_err(|error| {
                EmissionCodegenError::new(
                    EmissionCodegenErrorKind::Query(error),
                    DiagnosticBag::new(),
                )
            })?;

        finish_backend_contributions(plan, requests, &outcomes, cancellation)
    }
}

/// Failure to obtain complete backend contributions for an emission plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionCodegenError {
    kind: EmissionCodegenErrorKind,
    diagnostics: DiagnosticBag,
}

impl EmissionCodegenError {
    pub(super) fn new(kind: EmissionCodegenErrorKind, diagnostics: DiagnosticBag) -> Self {
        let diagnostics = match &kind {
            EmissionCodegenErrorKind::Request { error, .. } => match error.as_ref() {
                CodegenPreparationError::Diagnostics(produced) => diagnostics.merged(produced),
                _ => diagnostics,
            },
            _ => diagnostics,
        };

        Self { kind, diagnostics }
    }

    /// Returns the exact failed demand or validation operation.
    pub const fn kind(&self) -> &EmissionCodegenErrorKind {
        &self.kind
    }

    /// Returns code generation diagnostics in deterministic plan order.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }
}

/// Structured reason planned backend contributions could not be obtained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionCodegenErrorKind {
    /// The supplied inputs contain the same code generation unit more than once.
    DuplicateUnit(CodegenUnitKey),
    /// The supplied inputs contain mappings for the same code generation unit more than once.
    DuplicateMappings(CodegenUnitKey),
    /// The plan names a code generation unit absent from the supplied inputs.
    MissingUnit(CodegenUnitKey),
    /// The plan names a code generation unit without realization mappings.
    MissingMappings(CodegenUnitKey),
    /// One exact code generation input request could not be formed or evaluated.
    Request {
        /// Unit whose input request failed.
        unit: CodegenUnitKey,
        /// Exact request failure.
        error: Box<CodegenPreparationError>,
    },
    /// A backend reported failure for one planned code generation unit.
    Generation {
        /// Unit whose generation failed.
        unit: CodegenUnitKey,
        /// Backend-owned failure category.
        failure: CodegenFailure,
    },
    /// A backend observed cancellation before completing one planned unit.
    Cancelled(CodegenUnitKey),
    /// The shared query scheduler could not execute the planned requests.
    Query(FactQueryError),
    /// Completed backend sets violated the emission plan.
    InvalidContributions(BackendContributionMergeError),
}

struct CodegenUnitLookup<'inputs> {
    entries: BTreeMap<&'inputs CodegenUnitKey, CodegenUnitEntry<'inputs>>,
}

impl<'inputs> CodegenUnitLookup<'inputs> {
    fn try_new(
        units: &'inputs [CodegenUnit],
        mappings: &'inputs [CodegenMappings],
    ) -> Result<Self, EmissionCodegenErrorKind> {
        let mut entries = BTreeMap::new();

        for unit in units {
            let entry = entries
                .entry(unit.key())
                .or_insert_with(CodegenUnitEntry::default);

            if entry.unit.is_some() {
                // The error must retain the structural unit identity after the lookup is discarded.
                return Err(EmissionCodegenErrorKind::DuplicateUnit(unit.key().clone()));
            }

            entry.unit = Some(unit);
        }

        for mappings in mappings {
            let entry = entries
                .entry(mappings.unit())
                .or_insert_with(CodegenUnitEntry::default);

            if entry.mappings.is_some() {
                // The error must retain the structural unit identity after the lookup is discarded.
                return Err(EmissionCodegenErrorKind::DuplicateMappings(
                    mappings.unit().clone(),
                ));
            }

            entry.mappings = Some(mappings);
        }

        Ok(Self { entries })
    }

    fn unit(&self, key: &CodegenUnitKey) -> Option<&'inputs CodegenUnit> {
        self.entries.get(key).and_then(|entry| entry.unit)
    }

    fn mappings(&self, key: &CodegenUnitKey) -> Option<&'inputs CodegenMappings> {
        self.entries.get(key).and_then(|entry| entry.mappings)
    }
}

#[derive(Default)]
struct CodegenUnitEntry<'inputs> {
    unit: Option<&'inputs CodegenUnit>,
    mappings: Option<&'inputs CodegenMappings>,
}

fn complete_sets<'outcome>(
    requests: &[bray_codegen::BackendArtifactRequest],
    outcomes: &'outcome [Result<Arc<bray_codegen::CodegenOutcome>, EmissionCodegenErrorKind>],
) -> Result<Vec<&'outcome BackendArtifactSet>, EmissionCodegenErrorKind> {
    let mut sets = Vec::with_capacity(outcomes.len());

    for (request, outcome) in requests.iter().zip(outcomes) {
        let outcome = outcome.as_ref().map_err(Clone::clone)?;

        match outcome.status() {
            CodegenStatus::Complete(artifacts) => sets.push(artifacts.as_ref()),
            CodegenStatus::Failed(failure) => {
                return Err(EmissionCodegenErrorKind::Generation {
                    unit: request.unit().clone(),
                    failure: failure.clone(),
                });
            }
            CodegenStatus::Cancelled => {
                return Err(EmissionCodegenErrorKind::Cancelled(request.unit().clone()));
            }
        }
    }

    Ok(sets)
}

fn finish_backend_contributions(
    plan: &EmissionPlan,
    requests: &[bray_codegen::BackendArtifactRequest],
    outcomes: &[Result<Arc<bray_codegen::CodegenOutcome>, EmissionCodegenErrorKind>],
    cancellation: &CancellationToken,
) -> Result<DiagnosticResult<BackendContributionSet>, EmissionCodegenError> {
    let diagnostics = DiagnosticBag::merged_all(
        outcomes
            .iter()
            .filter_map(|outcome| outcome.as_ref().ok())
            .map(|outcome| outcome.diagnostics()),
    );

    let sets = match complete_sets(requests, outcomes) {
        Ok(sets) => sets,
        Err(kind) => return Err(EmissionCodegenError::new(kind, diagnostics)),
    };

    let contributions = match BackendContributionSet::try_from_backend(plan, sets, cancellation) {
        Ok(contributions) => contributions,
        Err(error) => {
            return Err(EmissionCodegenError::new(
                EmissionCodegenErrorKind::InvalidContributions(error),
                diagnostics,
            ));
        }
    };

    Ok(DiagnosticResult::new(contributions, diagnostics))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier, Condvar, Mutex};

    use bray_codegen::test_support::{
        CodegenRequestFixture, codegen_backend_capabilities, codegen_request_for_seed_and_backend,
    };
    use bray_codegen::{
        ArtifactContent, BackendArtifactContribution, BackendArtifactKind, BackendCapabilities,
        BackendIdentity, CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration,
        CodegenFailure, CodegenOutcome, CodegenRequest, CodegenRuntimeMetadata,
    };
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
    };
    use bray_emitter::{
        ArtifactKind, ArtifactProducer, ArtifactRequirement, BackendEmissionPolicy,
        EmissionBackend, EmissionPlan, EmissionPlanner, EmissionRequest, OutputSinkId,
        ReplacementPolicy, RequestedArtifact, RequestedArtifactDestination,
    };
    use bray_symbols::{ProductIdentity, ProductKind};
    use bray_target::{TargetOutputDescription, TargetOutputKind, TargetOutputName};

    use super::{CodegenUnitLookup, Compilation, EmissionCodegenErrorKind};
    use crate::test_support::{package_identity, source_input};
    use crate::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};

    #[test]
    fn planned_codegen_demand_is_parallel_but_merges_in_plan_order() {
        let backend = backend_identity();
        let first = codegen_request_for_seed_and_backend(1, backend.clone());
        let second = codegen_request_for_seed_and_backend(2, backend.clone());
        let plan = emission_plan(&first, &second);
        let slow_unit = plan.backend_requests()[0].unit().clone();
        let observer = Arc::new(GenerationObserver::new(slow_unit.clone()));

        let compilation = compilation_with_backend(&first, Arc::clone(&observer));

        let units = vec![
            second.request().unit().clone(),
            first.request().unit().clone(),
        ];

        let mappings = vec![
            second.request().mappings().clone(),
            first.request().mappings().clone(),
        ];

        let result = compilation
            .emission_backend_contributions(
                &plan,
                &units,
                &mappings,
                first.request().target(),
                first.request().options(),
            )
            .unwrap_or_else(|error| panic!("planned code generation must complete: {error:?}"));

        let expected_units = plan
            .backend_requests()
            .iter()
            .map(|request| request.unit().clone())
            .collect::<Vec<_>>();

        let actual_units = result
            .value()
            .contributions()
            .iter()
            .map(|contribution| {
                let ArtifactProducer::Backend { artifact, .. } = contribution.producer() else {
                    panic!("planned code generation must retain backend producers");
                };

                artifact.unit().clone()
            })
            .collect::<Vec<_>>();

        assert_eq!(actual_units, expected_units);

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.id().raw())
                .collect::<Vec<_>>(),
            [10, 20]
        );

        let observed = observer.snapshot();

        assert_eq!(
            observed.completed,
            expected_units.into_iter().rev().collect::<Vec<_>>()
        );

        assert_eq!(observed.requested.len(), 2);

        assert!(
            observed
                .requested
                .iter()
                .all(|kinds| { kinds.as_slice() == [BackendArtifactKind::RelocatableObject] })
        );
    }

    #[test]
    fn codegen_unit_lookup_rejects_duplicate_units_and_mappings() {
        let fixture = codegen_request_for_seed_and_backend(1, backend_identity());

        let units = [
            fixture.request().unit().clone(),
            fixture.request().unit().clone(),
        ];

        let duplicate_units = CodegenUnitLookup::try_new(&units, &[]);

        assert!(matches!(
            duplicate_units,
            Err(EmissionCodegenErrorKind::DuplicateUnit(unit))
                if unit == *fixture.request().unit().key()
        ));

        let mappings = [
            fixture.request().mappings().clone(),
            fixture.request().mappings().clone(),
        ];

        let duplicate_mappings = CodegenUnitLookup::try_new(&[], &mappings);

        assert!(matches!(
            duplicate_mappings,
            Err(EmissionCodegenErrorKind::DuplicateMappings(unit))
                if unit == *fixture.request().unit().key()
        ));
    }

    fn emission_plan(
        first: &CodegenRequestFixture,
        second: &CodegenRequestFixture,
    ) -> EmissionPlan {
        let request = first.request();
        let target = request.target();
        let capabilities = capabilities(request);

        let backend = EmissionBackend::try_new(
            request.backend().clone(),
            capabilities,
            [
                second.request().unit().key().clone(),
                request.unit().key().clone(),
            ],
            BackendEmissionPolicy::default(),
        )
        .unwrap_or_else(|error| panic!("test emission backend must be valid: {error:?}"));

        let name = TargetOutputName::try_new(TargetOutputKind::RelocatableObject, "", ".o")
            .unwrap_or_else(|error| panic!("test output name must be valid: {error:?}"));

        let target = TargetOutputDescription::try_new(target.profile().clone(), [name])
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"));

        let product = ProductIdentity::try_new(package_identity(), "library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let collector = OutputSinkId::try_new("test.codegen")
            .unwrap_or_else(|| panic!("test collector identity must be valid"));

        let request = EmissionRequest::try_new(
            product,
            ProductKind::Library,
            None,
            target.identity().clone(),
            RequestedArtifactDestination::Memory(collector),
            [RequestedArtifact::new(
                ArtifactKind::RelocatableObject,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        EmissionPlanner::new(target, Some(backend), None)
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must be valid: {error:?}"))
    }

    fn compilation_with_backend(
        fixture: &CodegenRequestFixture,
        observer: Arc<GenerationObserver>,
    ) -> Compilation {
        let generator = Arc::new(RecordingCodeGenerator {
            identity: fixture.request().backend().clone(),
            capabilities: capabilities(fixture.request()),
            observer,
        });

        let selected = generator.identity().clone();

        let registry = CodeGeneratorRegistry::try_new([generator as Arc<dyn CodeGenerator>])
            .unwrap_or_else(|error| panic!("test backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, selected)
            .unwrap_or_else(|error| panic!("test backend must select: {error:?}"));

        let workers = WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"));

        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input("module app;", 0)],
            CompilationOptions::new(workers, ProductKind::Library, SelectedTarget::baseline()),
        );

        Compilation::load_with_codegen(request, codegen)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn capabilities(_request: CodegenRequest<'_>) -> BackendCapabilities {
        codegen_backend_capabilities()
    }

    fn backend_identity() -> BackendIdentity {
        BackendIdentity::try_new("llvm", "bray-1", "llvm-test")
            .unwrap_or_else(|| panic!("test backend identity must be valid"))
    }

    struct RecordingCodeGenerator {
        identity: BackendIdentity,
        capabilities: BackendCapabilities,
        observer: Arc<GenerationObserver>,
    }

    impl CodeGenerator for RecordingCodeGenerator {
        fn identity(&self) -> &BackendIdentity {
            &self.identity
        }

        fn capabilities(&self) -> &BackendCapabilities {
            &self.capabilities
        }

        fn validate_target(
            &self,
            _target: &bray_codegen::CodegenTarget,
        ) -> Result<(), CodegenFailure> {
            Ok(())
        }

        fn generate(&self, request: CodegenRequest<'_>) -> CodegenOutcome {
            let kinds = request
                .artifacts()
                .entries()
                .iter()
                .map(|entry| entry.id().kind())
                .collect();

            let ordinal = self.observer.complete(request.unit().key(), kinds);
            let bytes = [u8::try_from(ordinal).unwrap_or(u8::MAX)];

            let content = ArtifactContent::try_memory(bytes.as_slice())
                .unwrap_or_else(|error| panic!("test content must be valid: {error:?}"));

            let contributions = request.artifacts().entries().iter().map(|entry| {
                BackendArtifactContribution::new(
                    entry.id().clone(),
                    content.clone(),
                    request.backend().clone(),
                    request.capability_revision(),
                    request.target().identity().clone(),
                    None,
                )
            });

            let diagnostic = Diagnostic::new(
                DiagnosticId::new(ordinal),
                DiagnosticKind::EmissionInvalidContribution,
                SeverityKind::Warning,
            )
            .with_arg(DiagnosticArg::artifact_ordinal(ordinal));

            CodegenOutcome::try_complete(
                request,
                contributions,
                CodegenRuntimeMetadata::default(),
                DiagnosticBag::single(diagnostic),
            )
            .unwrap_or_else(|error| panic!("test code generation must complete: {error:?}"))
        }
    }

    struct GenerationObserver {
        slow_unit: bray_codegen::CodegenUnitKey,
        started: Barrier,
        fast_completed: (Mutex<bool>, Condvar),
        state: Mutex<GenerationObservation>,
    }

    impl GenerationObserver {
        fn new(slow_unit: bray_codegen::CodegenUnitKey) -> Self {
            Self {
                slow_unit,
                started: Barrier::new(2),
                fast_completed: (Mutex::new(false), Condvar::new()),
                state: Mutex::new(GenerationObservation::default()),
            }
        }

        fn complete(
            &self,
            unit: &bray_codegen::CodegenUnitKey,
            kinds: Vec<BackendArtifactKind>,
        ) -> u32 {
            self.state
                .lock()
                .unwrap_or_else(|_| panic!("test observation must remain available"))
                .requested
                .push(kinds);

            self.started.wait();

            let slow = unit == &self.slow_unit;

            if slow {
                let (completed, changed) = &self.fast_completed;

                let completed = completed
                    .lock()
                    .unwrap_or_else(|_| panic!("test completion gate must remain available"));

                let waited = changed
                    .wait_while(completed, |completed| !*completed)
                    .unwrap_or_else(|_| panic!("test completion gate must remain available"));

                drop(waited);
            }

            self.state
                .lock()
                .unwrap_or_else(|_| panic!("test observation must remain available"))
                .completed
                .push(unit.clone());

            if !slow {
                let (completed, changed) = &self.fast_completed;

                let mut completed = completed
                    .lock()
                    .unwrap_or_else(|_| panic!("test completion gate must remain available"));

                // Publish the observed completion before allowing the other worker to finish.
                *completed = true;

                changed.notify_all();
            }

            if slow { 10 } else { 20 }
        }

        fn snapshot(&self) -> GenerationObservation {
            self.state
                .lock()
                .unwrap_or_else(|_| panic!("test observation must remain available"))
                .clone()
        }
    }

    #[derive(Clone, Default)]
    struct GenerationObservation {
        requested: Vec<Vec<BackendArtifactKind>>,
        completed: Vec<bray_codegen::CodegenUnitKey>,
    }
}
