use bray_emitter::{
    ArtifactContribution, ArtifactPublisher, EmissionOutcome, EmissionPlan, OutputSinkResolver,
};
use bray_linker::{LinkOutcome, LinkPlan, LinkStatus, Linker};

use super::Compilation;
use crate::QueryPriority;
use crate::fact::{CancellationToken, FactQueryError};

/// Failure while scheduling or validating linked-product publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkedProductEmissionError {
    /// Lazy compiler work could not be completed.
    Query(FactQueryError),
    /// Emitter outcome diagnostics contradict the claimed terminal status.
    Outcome(bray_emitter::EmissionOutcomeBuildError),
}

impl Compilation {
    /// Links and publishes one native product through its immutable emission plan.
    pub fn emit_linked_product(
        &self,
        linker: &Linker,
        emission: &EmissionPlan,
        link: &LinkPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
    ) -> Result<EmissionOutcome, LinkedProductEmissionError> {
        self.emit_linked_product_with_cancellation(
            linker,
            emission,
            link,
            contributions,
            None,
            &self.state.cancellation,
        )
    }

    /// Links and publishes while observing caller cancellation and indirect sinks.
    pub fn emit_linked_product_with_cancellation(
        &self,
        linker: &Linker,
        emission: &EmissionPlan,
        link: &LinkPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
        resolver: Option<&dyn OutputSinkResolver>,
        cancellation: &CancellationToken,
    ) -> Result<EmissionOutcome, LinkedProductEmissionError> {
        let link_outcome = self
            .link_product_with_cancellation(linker, link, cancellation)
            .map_err(LinkedProductEmissionError::Query)?;

        let publisher = match resolver {
            Some(resolver) => ArtifactPublisher::with_sink_resolver(cancellation, resolver),
            None => ArtifactPublisher::new(cancellation),
        };

        let outcome = crate::profile::profile_operation(
            self.state.fact_runtime.profile(),
            crate::profile::ProfileOperation::ArtifactPublication,
            || publisher.publish_linked(emission, contributions, link, &link_outcome),
            crate::profile::result_outcome,
        );

        outcome.map_err(LinkedProductEmissionError::Outcome)
    }

    /// Links one validated native product plan into staging without publishing final outputs.
    pub fn link_product(
        &self,
        linker: &Linker,
        plan: &LinkPlan,
    ) -> Result<LinkOutcome, FactQueryError> {
        self.link_product_with_cancellation(linker, plan, &self.state.cancellation)
    }

    /// Links into staging while observing caller cancellation without publishing final outputs.
    pub fn link_product_with_cancellation(
        &self,
        linker: &Linker,
        plan: &LinkPlan,
        cancellation: &CancellationToken,
    ) -> Result<LinkOutcome, FactQueryError> {
        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.add_metric(
                crate::profile::ProfileMetricKind::LinkInputs,
                u64::try_from(plan.inputs().len()).unwrap_or(u64::MAX),
            );
        }

        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        let profile = self.state.fact_runtime.profile();

        self.state.fact_runtime.run(priority, || {
            let span = profile
                .map(|profile| profile.start(crate::profile::ProfileOperation::Linking, None));

            let outcome = linker.link(plan, cancellation);

            if let Some(span) = span {
                span.finish(link_profile_outcome(outcome.status()));
            }

            Ok(outcome)
        })
    }
}

const fn link_profile_outcome(status: &LinkStatus) -> crate::CompilationProfileOutcome {
    match status {
        LinkStatus::Complete(_) => crate::CompilationProfileOutcome::Completed,
        LinkStatus::Failed(_) => crate::CompilationProfileOutcome::Failed,
        LinkStatus::Cancelled => crate::CompilationProfileOutcome::Cancelled,
    }
}

pub(super) fn product_emission_error(
    kind: super::ProductEmissionErrorKind,
    prior: &bray_diagnostics::DiagnosticBag,
    plan: &EmissionPlan,
) -> super::ProductEmissionError {
    super::ProductEmissionError::new(
        kind,
        prior.clone(),
        plan.request().product(),
        plan.request().target(),
    )
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    use bray_base::Cancellation;
    use bray_codegen::test_support::{codegen_backend_capabilities, codegen_unit_key};
    use bray_codegen::{
        AssemblySyntaxKind, BackendIdentity, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind,
    };
    use bray_diagnostics::DiagnosticBag;
    use bray_emitter::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend, EmissionPlan,
        EmissionPlanner, EmissionRequest, EmissionStatus, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };
    use bray_linker::{
        BinarySymbolName, DebugLinkPolicy, LinkCancellationCapability, LinkDeterminismCapability,
        LinkEnvironmentCapability, LinkFailure, LinkInput, LinkInputId, LinkInputKind,
        LinkInputMode, LinkInputProvenance, LinkInputSource, LinkModel, LinkOutcome, LinkPlan,
        LinkPlanBuilder, LinkPlanCapability, LinkPolicy, LinkResponseFileCapability,
        LinkRuntimeMode, LinkStartupMode, LinkStatus, LinkSymbolRequirement, LinkTarget,
        LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind, Linker,
        LinkerDriver, LinkerDriverCapabilities, LinkerDriverIdentity, LinkerDriverKind,
        LinkerOperationalCapabilities, LinkerTargetCapabilities, PlannedLinkedArtifact,
        SectionGarbageCollectionPolicy, StagingDestination, StagingDestinationId, StagingPathKey,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId,
        RuntimeCapability, RuntimeRoleImplementation,
    };
    use bray_symbols::{ProductIdentity, ProductKind};
    use bray_target::test_support::test_target_profile;
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
        TargetOutputDescription, TargetOutputKind, TargetOutputName,
    };
    use bray_testing::{TemporaryFile, test_async_executable_host_contract_for};

    use super::Compilation;
    use crate::test_support::{package_identity, source_input};
    use crate::{
        CancellationToken, CompilationOptions, CompilationProfileConfiguration,
        CompilationProfileMode, CompilationProfileOutcome, CompilationRequest, SelectedTarget,
        WorkerBudget,
    };

    #[test]
    fn executable_and_shared_library_links_complete_in_staging() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);
        let compilation = compilation(WorkerBudget::serial());

        let executable = basic_plan(
            LinkedProductKind::Executable,
            LinkedArtifactKind::Executable,
            "application.stage",
            Some(synchronous_host()),
        );

        let shared_library = shared_library_plan();

        for (plan, artifact_count) in [(&executable, 1), (&shared_library, 2)] {
            let outcome = compilation
                .link_product(&linker, plan)
                .unwrap_or_else(|error| panic!("test link must run: {error:?}"));

            let LinkStatus::Complete(artifacts) = outcome.status() else {
                panic!("test link must produce complete staging artifacts");
            };

            assert_eq!(artifacts.artifacts().len(), artifact_count);

            assert_eq!(
                artifacts.artifacts()[0].kind(),
                plan.product_kind().primary_artifact_kind()
            );
        }

        assert_eq!(driver.plans(), vec![executable, shared_library]);
    }

    #[test]
    fn linked_product_emission_invokes_the_linker_and_publishes_its_staging() {
        let input = TemporaryFile::write("application.o", b"object");

        let directory = input
            .path()
            .parent()
            .unwrap_or_else(|| panic!("test input must have a containing directory"));

        let staging_path = directory.join("application.stage");
        let emission = executable_emission_plan(directory);
        let link = executable_plan_for(input.path(), &staging_path);
        let driver = Arc::new(RecordingDriver::publishing(b"linked executable"));
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);

        let outcome = compilation(WorkerBudget::serial())
            .emit_linked_product(&linker, &emission, &link, [])
            .unwrap_or_else(|error| panic!("linked test emission must run: {error:?}"));

        assert!(
            matches!(outcome.status(), EmissionStatus::Complete),
            "{:?}: {:?}",
            outcome.status(),
            outcome.diagnostics()
        );

        let generation = outcome
            .generation()
            .unwrap_or_else(|| panic!("linked emission must publish a managed generation"));

        let artifact = &outcome.artifacts().artifacts()[0];

        let published_path = generation
            .artifact_path(artifact.id())
            .unwrap_or_else(|| panic!("linked artifact path must resolve"));

        assert_eq!(
            std::fs::read(&published_path)
                .unwrap_or_else(|error| panic!("published test artifact must be read: {error}")),
            b"linked executable"
        );

        assert!(!input.path().exists());
        assert!(!staging_path.exists());
        assert_eq!(driver.plans(), vec![link]);
    }

    #[test]
    fn async_link_forwards_resolved_host_runtime_and_companion_inputs() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);
        let runtime = runtime_artifact_id();

        let executable_host = test_async_executable_host_contract_for(
            product(),
            link_target().identity().clone(),
            runtime.clone(),
        );

        let mut builder = plan_builder_with_startup(
            LinkedProductKind::Executable,
            DebugLinkPolicy::Companion,
            LinkStartupMode::ExplicitInputs,
        );

        builder.push_input(file_input(
            0,
            LinkInputKind::StartupObject,
            "startup.o",
            LinkInputProvenance::TargetProfile,
        ));

        builder.push_input(file_input(
            1,
            LinkInputKind::RelocatableObject,
            "main.o",
            LinkInputProvenance::Product,
        ));

        builder.push_input(file_input(
            2,
            LinkInputKind::RuntimeComponent,
            "runtime.a",
            LinkInputProvenance::Runtime(runtime.clone()),
        ));

        builder.push_input(
            LinkInput::try_native_library(
                LinkInputId::new(3),
                "pthread",
                LinkInputProvenance::HostConfiguration,
            )
            .unwrap_or_else(|| panic!("test native library name must be valid")),
        );

        builder.push_input(file_input(
            4,
            LinkInputKind::TerminationObject,
            "termination.o",
            LinkInputProvenance::TargetProfile,
        ));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            "application.stage",
        ));

        builder.push_output(planned_output(
            1,
            LinkedArtifactKind::DebugCompanion,
            "application.debug.stage",
        ));

        builder.set_executable_host(executable_host.clone());
        builder.push_exported_symbol(symbol("bray_export"));
        builder.push_retained_symbol(symbol("bray_root_frame"));

        let plan = builder
            .finish()
            .unwrap_or_else(|error| panic!("async test plan must be valid: {error:?}"));

        let outcome = compilation(WorkerBudget::serial())
            .link_product(&linker, &plan)
            .unwrap_or_else(|error| panic!("async test link must run: {error:?}"));

        let LinkStatus::Complete(artifacts) = outcome.status() else {
            panic!("async test link must produce complete staging artifacts");
        };

        assert_eq!(artifacts.artifacts().len(), 2);

        assert_eq!(
            plan.inputs(),
            [
                file_input(
                    0,
                    LinkInputKind::StartupObject,
                    "startup.o",
                    LinkInputProvenance::TargetProfile,
                ),
                file_input(
                    1,
                    LinkInputKind::RelocatableObject,
                    "main.o",
                    LinkInputProvenance::Product,
                ),
                file_input(
                    2,
                    LinkInputKind::RuntimeComponent,
                    "runtime.a",
                    LinkInputProvenance::Runtime(runtime.clone()),
                ),
                LinkInput::try_native_library(
                    LinkInputId::new(3),
                    "pthread",
                    LinkInputProvenance::HostConfiguration,
                )
                .unwrap_or_else(|| panic!("test native library name must be valid")),
                file_input(
                    4,
                    LinkInputKind::TerminationObject,
                    "termination.o",
                    LinkInputProvenance::TargetProfile,
                ),
            ]
        );

        assert_eq!(
            plan.outputs()
                .iter()
                .map(PlannedLinkedArtifact::kind)
                .collect::<Vec<_>>(),
            [
                LinkedArtifactKind::Executable,
                LinkedArtifactKind::DebugCompanion,
            ]
        );

        assert_eq!(plan.retained_symbols(), [symbol("bray_root_frame")]);
        assert_eq!(plan.entry_point(), Some(executable_host.native_entry()));
        assert_eq!(plan.executable_host(), Some(&executable_host));
        assert_eq!(executable_host.native_entry().as_str(), "_bray_host_start");

        assert_eq!(
            executable_host.entries()[0].root(),
            RootExecution::Asynchronous {
                frame: ProtectedAsyncFrameId::new([7; 32]),
            }
        );

        assert_eq!(executable_host.abi_version(), RuntimeAbiVersion::new(1, 0));

        assert_eq!(executable_host.runtime_artifact(), Some(&runtime));

        assert_eq!(
            executable_host
                .runtime()
                .map(|runtime| runtime.abi_version()),
            Some(RuntimeAbiVersion::new(1, 0))
        );

        assert_eq!(
            executable_host.runtime_capabilities(),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ]
        );

        for role in [
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::MainThreadLaneDrive,
        ] {
            assert_eq!(
                executable_host
                    .role_binding(role)
                    .map(|binding| binding.implementation()),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        assert_eq!(
            executable_host
                .role_binding(RuntimeAbiRole::StructuredShutdown)
                .map(|binding| binding.implementation()),
            Some(RuntimeRoleImplementation::CompilerLowering)
        );

        assert_eq!(driver.plans(), vec![plan]);
    }

    #[test]
    fn cancellation_during_link_discards_driver_success() {
        let rendezvous = Arc::new(Barrier::new(2));

        let driver = Arc::new(BlockingDriver {
            capabilities: driver_capabilities(),
            rendezvous: Arc::clone(&rendezvous),
        });

        let linker = linker(driver as Arc<dyn LinkerDriver>);
        let plan = executable_plan("cancelled.stage");
        let cancellation = CancellationToken::new();
        let compilation = profiled_compilation();

        std::thread::scope(|scope| {
            let operation = scope.spawn(|| {
                compilation.link_product_with_cancellation(&linker, &plan, &cancellation)
            });

            rendezvous.wait();
            cancellation.cancel();
            rendezvous.wait();

            let outcome = operation
                .join()
                .unwrap_or_else(|_| panic!("cancelled test link must not terminate"))
                .unwrap_or_else(|error| panic!("cancelled test link must return: {error:?}"));

            assert_eq!(outcome.status(), &LinkStatus::Cancelled);
            assert_eq!(outcome.artifacts(), None);
        });

        assert_link_profile(&compilation, CompilationProfileOutcome::Cancelled);
    }

    #[test]
    fn failed_links_expose_no_partial_staging_result() {
        let driver = Arc::new(RecordingDriver::failing());
        let linker = linker(driver as Arc<dyn LinkerDriver>);
        let plan = executable_plan("failed.stage");

        let compilation = profiled_compilation();

        let outcome = compilation
            .link_product(&linker, &plan)
            .unwrap_or_else(|error| panic!("failed test link must return: {error:?}"));

        assert_eq!(
            outcome.status(),
            &LinkStatus::Failed(LinkFailure::Invocation)
        );

        assert_eq!(outcome.artifacts(), None);
        assert_link_profile(&compilation, CompilationProfileOutcome::Failed);
    }

    #[test]
    fn repeated_links_have_deterministic_outcomes() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);
        let compilation = compilation(WorkerBudget::serial());
        let plan = executable_plan("repeated.stage");

        let first = compilation
            .link_product(&linker, &plan)
            .unwrap_or_else(|error| panic!("first deterministic link must run: {error:?}"));

        let second = compilation
            .link_product(&linker, &plan)
            .unwrap_or_else(|error| panic!("second deterministic link must run: {error:?}"));

        assert_eq!(first, second);
        assert_eq!(driver.plans(), vec![plan.clone(), plan]);
    }

    #[test]
    fn independent_links_run_concurrently_within_the_worker_budget() {
        let observation = Arc::new(ConcurrentLinkObservation::new(2));

        let driver = Arc::new(ConcurrentDriver {
            capabilities: driver_capabilities(),
            observation: Arc::clone(&observation),
        });

        let linker = Arc::new(linker(driver as Arc<dyn LinkerDriver>));

        let compilation = compilation(
            WorkerBudget::new(2)
                .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}")),
        );

        let first_plan = executable_plan("first.stage");
        let second_plan = executable_plan("second.stage");

        std::thread::scope(|scope| {
            let first_compilation = compilation.clone();
            let first_linker = Arc::clone(&linker);

            let first =
                scope.spawn(move || first_compilation.link_product(&first_linker, &first_plan));

            let second_compilation = compilation.clone();
            let second_linker = Arc::clone(&linker);

            let second =
                scope.spawn(move || second_compilation.link_product(&second_linker, &second_plan));

            for result in [first.join(), second.join()] {
                let outcome = result
                    .unwrap_or_else(|_| panic!("concurrent test link must not terminate"))
                    .unwrap_or_else(|error| panic!("concurrent test link must run: {error:?}"));

                assert!(matches!(outcome.status(), LinkStatus::Complete(_)));
            }
        });

        assert_eq!(observation.maximum.load(Ordering::SeqCst), 2);
        assert_eq!(observation.active.load(Ordering::SeqCst), 0);
    }

    fn compilation(worker_budget: WorkerBudget) -> Compilation {
        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input("module app;", 0)],
            CompilationOptions::new(
                worker_budget,
                ProductKind::Executable,
                SelectedTarget::baseline(),
            ),
        );

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn profiled_compilation() -> Compilation {
        let request = CompilationRequest::with_options(
            package_identity(),
            vec![source_input("module app;", 0)],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Executable,
                SelectedTarget::baseline(),
            ),
        )
        .with_profile(CompilationProfileConfiguration::new(
            CompilationProfileMode::Summary,
        ));

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("profiled test compilation must load: {error:?}"))
    }

    fn assert_link_profile(compilation: &Compilation, outcome: CompilationProfileOutcome) {
        let report = compilation
            .profile_report()
            .unwrap_or_else(|| panic!("profiled link must report"));

        let link = report
            .operations
            .iter()
            .find(|operation| {
                report
                    .operation_descriptor(operation.id)
                    .is_some_and(|descriptor| descriptor.name == "compiler.link")
            })
            .unwrap_or_else(|| panic!("link operation statistics must be present"));

        let (completed, failed, cancelled) = match outcome {
            CompilationProfileOutcome::Completed => (1, 0, 0),
            CompilationProfileOutcome::Failed => (0, 1, 0),
            CompilationProfileOutcome::Cancelled => (0, 0, 1),
            CompilationProfileOutcome::Abandoned => (0, 0, 0),
        };

        assert_eq!(link.executions, 1);
        assert_eq!(link.completed, completed);
        assert_eq!(link.failed, failed);
        assert_eq!(link.cancelled, cancelled);
    }

    fn linker(driver: Arc<dyn LinkerDriver>) -> Linker {
        Linker::try_new([driver])
            .unwrap_or_else(|error| panic!("test linker must be valid: {error:?}"))
    }

    fn executable_plan(path: &str) -> LinkPlan {
        basic_plan(
            LinkedProductKind::Executable,
            LinkedArtifactKind::Executable,
            path,
            Some(synchronous_host()),
        )
    }

    fn executable_plan_for(input: &Path, output: &Path) -> LinkPlan {
        let mut builder = plan_builder(LinkedProductKind::Executable, DebugLinkPolicy::None);

        builder.push_input(
            LinkInput::try_new(
                LinkInputId::new(0),
                LinkInputKind::RelocatableObject,
                LinkInputSource::file(input),
                LinkInputProvenance::Product,
                LinkInputMode::Ordinary,
            )
            .unwrap_or_else(|error| panic!("test product input must be valid: {error:?}")),
        );

        builder.push_output(planned_output_for(
            0,
            LinkedArtifactKind::Executable,
            output,
        ));

        builder.set_executable_host(synchronous_host());

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test executable plan must be valid: {error:?}"))
    }

    fn executable_emission_plan(root: &Path) -> EmissionPlan {
        let backend = BackendIdentity::try_new("llvm", "bray-1", "llvm-22")
            .unwrap_or_else(|| panic!("test backend identity must be valid"));

        let capabilities = codegen_backend_capabilities();

        let policy = BackendEmissionPolicy::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Omit,
            Some(LinkableArtifactKind::RelocatableObject),
            BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
        );

        let backend =
            EmissionBackend::try_new(backend, capabilities, [codegen_unit_key(1)], policy)
                .unwrap_or_else(|error| panic!("test emission backend must be valid: {error:?}"));

        let output_names = [
            TargetOutputName::try_new(TargetOutputKind::RelocatableObject, "", ".o")
                .unwrap_or_else(|error| panic!("test object output name must be valid: {error:?}")),
            TargetOutputName::try_new(TargetOutputKind::Executable, "", "").unwrap_or_else(
                |error| panic!("test executable output name must be valid: {error:?}"),
            ),
        ];

        let target = TargetOutputDescription::try_new(test_target_profile(), output_names)
            .unwrap_or_else(|error| panic!("test target outputs must be valid: {error:?}"));

        let request = EmissionRequest::try_new(
            product(),
            ProductKind::Executable,
            Some(synchronous_host()),
            target.identity().clone(),
            RequestedArtifactDestination::FilesystemDirectory(root.to_owned().into()),
            [RequestedArtifact::new(
                ArtifactKind::Executable,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        EmissionPlanner::new(target, Some(backend), None)
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must be valid: {error:?}"))
    }

    fn shared_library_plan() -> LinkPlan {
        let mut builder = plan_builder(LinkedProductKind::SharedLibrary, DebugLinkPolicy::None);

        builder.push_input(file_input(
            0,
            LinkInputKind::RelocatableObject,
            "member.o",
            LinkInputProvenance::Product,
        ));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::SharedLibrary,
            "library.stage",
        ));

        builder.push_output(planned_output(
            1,
            LinkedArtifactKind::PlatformCompanion,
            "library.companion.stage",
        ));

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test shared-library plan must be valid: {error:?}"))
    }

    fn basic_plan(
        product_kind: LinkedProductKind,
        artifact_kind: LinkedArtifactKind,
        path: &str,
        host: Option<bray_runtime_interface::ExecutableHostContract>,
    ) -> LinkPlan {
        let mut builder = plan_builder(product_kind, DebugLinkPolicy::None);

        builder.push_input(file_input(
            0,
            LinkInputKind::RelocatableObject,
            "member.o",
            LinkInputProvenance::Product,
        ));

        builder.push_output(planned_output(0, artifact_kind, path));

        if let Some(host) = host {
            builder.set_executable_host(host);
        }

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"))
    }

    fn plan_builder(product_kind: LinkedProductKind, debug: DebugLinkPolicy) -> LinkPlanBuilder {
        let startup = match product_kind {
            LinkedProductKind::Executable | LinkedProductKind::SharedLibrary => {
                LinkStartupMode::PlatformCompilerDriver
            }
            LinkedProductKind::StaticLibrary => LinkStartupMode::NotApplicable,
        };

        plan_builder_with_startup(product_kind, debug, startup)
    }

    fn plan_builder_with_startup(
        product_kind: LinkedProductKind,
        debug: DebugLinkPolicy,
        startup: LinkStartupMode,
    ) -> LinkPlanBuilder {
        LinkPlanBuilder::new(
            product(),
            product_kind,
            link_target(),
            driver_identity(),
            startup,
            LinkPolicy::new(
                bray_linker::DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                debug,
                None,
            ),
        )
    }

    fn file_input(
        ordinal: u32,
        kind: LinkInputKind,
        path: &str,
        provenance: LinkInputProvenance,
    ) -> LinkInput {
        LinkInput::try_new(
            LinkInputId::new(ordinal),
            kind,
            LinkInputSource::file(path),
            provenance,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test file input must be valid: {error:?}"))
    }

    fn planned_output(ordinal: u32, kind: LinkedArtifactKind, path: &str) -> PlannedLinkedArtifact {
        planned_output_for(ordinal, kind, Path::new(path))
    }

    fn planned_output_for(
        ordinal: u32,
        kind: LinkedArtifactKind,
        path: &Path,
    ) -> PlannedLinkedArtifact {
        let path_key = StagingPathKey::try_new(path.to_string_lossy().into_owned())
            .unwrap_or_else(|| panic!("test staging path key must be valid"));

        let destination =
            StagingDestination::try_new(StagingDestinationId::new(ordinal), path, path_key)
                .unwrap_or_else(|error| {
                    panic!("test staging destination must be valid: {error:?}")
                });

        PlannedLinkedArtifact::new(kind, LinkedArtifactRequirement::Required, destination)
    }

    fn synchronous_host() -> bray_runtime_interface::ExecutableHostContract {
        bray_testing::test_executable_host_contract_for(product(), link_target().identity().clone())
    }

    fn product() -> ProductIdentity {
        ProductIdentity::try_new(package_identity(), "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }

    fn link_target() -> LinkTarget {
        let identity = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        LinkTarget::try_new(
            identity,
            "x86_64-unknown-linux-gnu",
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }

    fn driver_identity() -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
            .unwrap_or_else(|| panic!("test linker identity must be valid"))
    }

    fn driver_capabilities() -> LinkerDriverCapabilities {
        let target = LinkerTargetCapabilities::new(
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            [
                LinkPlanCapability::Product(LinkedProductKind::Executable),
                LinkPlanCapability::Product(LinkedProductKind::SharedLibrary),
                LinkPlanCapability::Input(LinkInputKind::RelocatableObject),
                LinkPlanCapability::Input(LinkInputKind::StartupObject),
                LinkPlanCapability::Input(LinkInputKind::TerminationObject),
                LinkPlanCapability::Input(LinkInputKind::RuntimeComponent),
                LinkPlanCapability::Input(LinkInputKind::NativeLibrary),
                LinkPlanCapability::InputMode(LinkInputMode::Ordinary),
                LinkPlanCapability::Output(LinkedArtifactKind::Executable),
                LinkPlanCapability::Output(LinkedArtifactKind::SharedLibrary),
                LinkPlanCapability::Output(LinkedArtifactKind::DebugCompanion),
                LinkPlanCapability::Output(LinkedArtifactKind::PlatformCompanion),
                LinkPlanCapability::LinkModel(LinkModel::Dynamic),
                LinkPlanCapability::DeadStrip(bray_linker::DeadStripPolicy::Preserve),
                LinkPlanCapability::SectionGarbageCollection(
                    SectionGarbageCollectionPolicy::Preserve,
                ),
                LinkPlanCapability::Debug(DebugLinkPolicy::None),
                LinkPlanCapability::Debug(DebugLinkPolicy::Companion),
                LinkPlanCapability::Symbol(LinkSymbolRequirement::EntryPoint),
                LinkPlanCapability::Symbol(LinkSymbolRequirement::ExportedSymbols),
                LinkPlanCapability::Symbol(LinkSymbolRequirement::RetainedSymbols),
                LinkPlanCapability::Startup(LinkStartupMode::ExplicitInputs),
                LinkPlanCapability::Startup(LinkStartupMode::PlatformCompilerDriver),
                LinkPlanCapability::Runtime(LinkRuntimeMode::ExplicitInput),
            ],
        );

        LinkerDriverCapabilities::try_new(
            driver_identity(),
            [target],
            LinkerOperationalCapabilities::new(
                LinkResponseFileCapability::InlineArguments,
                LinkEnvironmentCapability::NotApplicable,
                LinkCancellationCapability::Cooperative,
                LinkDeterminismCapability::Reproducible,
            ),
        )
        .unwrap_or_else(|error| panic!("test capabilities must be valid: {error:?}"))
    }

    fn runtime_artifact_id() -> RuntimeArtifactId {
        RuntimeArtifactId::try_new("runtime.test")
            .unwrap_or_else(|| panic!("test runtime identity must be valid"))
    }

    fn symbol(name: &str) -> BinarySymbolName {
        BinarySymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test binary symbol must be valid"))
    }

    struct RecordingDriver {
        capabilities: LinkerDriverCapabilities,
        plans: Mutex<Vec<LinkPlan>>,
        completes: bool,
        published_bytes: Option<&'static [u8]>,
    }

    impl RecordingDriver {
        fn completing() -> Self {
            Self {
                capabilities: driver_capabilities(),
                plans: Mutex::new(Vec::new()),
                completes: true,
                published_bytes: None,
            }
        }

        fn failing() -> Self {
            Self {
                capabilities: driver_capabilities(),
                plans: Mutex::new(Vec::new()),
                completes: false,
                published_bytes: None,
            }
        }

        fn publishing(bytes: &'static [u8]) -> Self {
            Self {
                capabilities: driver_capabilities(),
                plans: Mutex::new(Vec::new()),
                completes: true,
                published_bytes: Some(bytes),
            }
        }

        fn plans(&self) -> Vec<LinkPlan> {
            self.plans
                .lock()
                .unwrap_or_else(|_| panic!("test plan log must remain available"))
                .clone()
        }
    }

    impl LinkerDriver for RecordingDriver {
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            &self.capabilities
        }

        fn link(&self, plan: &LinkPlan, _cancellation: &dyn Cancellation) -> LinkOutcome {
            self.plans
                .lock()
                .unwrap_or_else(|_| panic!("test plan log must remain available"))
                .push(plan.clone());

            if !self.completes {
                return LinkOutcome::failed(plan, LinkFailure::Invocation, DiagnosticBag::new());
            }

            if let Some(bytes) = self.published_bytes {
                for output in plan.outputs() {
                    std::fs::write(output.destination().path(), bytes).unwrap_or_else(|error| {
                        panic!("test linked staging must be written: {error}")
                    });
                }

                return complete_with_byte_len(plan, bytes.len());
            }

            complete(plan)
        }
    }

    struct BlockingDriver {
        capabilities: LinkerDriverCapabilities,
        rendezvous: Arc<Barrier>,
    }

    impl LinkerDriver for BlockingDriver {
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            &self.capabilities
        }

        fn link(&self, plan: &LinkPlan, _cancellation: &dyn Cancellation) -> LinkOutcome {
            self.rendezvous.wait();
            self.rendezvous.wait();

            complete(plan)
        }
    }

    struct ConcurrentDriver {
        capabilities: LinkerDriverCapabilities,
        observation: Arc<ConcurrentLinkObservation>,
    }

    impl LinkerDriver for ConcurrentDriver {
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            &self.capabilities
        }

        fn link(&self, plan: &LinkPlan, _cancellation: &dyn Cancellation) -> LinkOutcome {
            let active = self.observation.active.fetch_add(1, Ordering::SeqCst) + 1;

            self.observation.maximum.fetch_max(active, Ordering::SeqCst);
            self.observation.barrier.wait();

            let outcome = complete(plan);

            self.observation.active.fetch_sub(1, Ordering::SeqCst);

            outcome
        }
    }

    struct ConcurrentLinkObservation {
        barrier: Barrier,
        active: AtomicUsize,
        maximum: AtomicUsize,
    }

    impl ConcurrentLinkObservation {
        fn new(participants: usize) -> Self {
            Self {
                barrier: Barrier::new(participants),
                active: AtomicUsize::new(0),
                maximum: AtomicUsize::new(0),
            }
        }
    }

    fn complete(plan: &LinkPlan) -> LinkOutcome {
        complete_with_byte_len(plan, 1)
    }

    fn complete_with_byte_len(plan: &LinkPlan, byte_len: usize) -> LinkOutcome {
        let byte_len = u64::try_from(byte_len)
            .ok()
            .and_then(NonZeroU64::new)
            .unwrap_or_else(|| panic!("test linked artifact length must be nonzero"));

        let artifacts = plan
            .outputs()
            .iter()
            .map(|output| LinkedArtifact::new(output.kind(), output.destination().id(), byte_len));

        LinkOutcome::try_complete(plan, artifacts, DiagnosticBag::new())
            .unwrap_or_else(|error| panic!("test link must complete: {error:?}"))
    }
}
