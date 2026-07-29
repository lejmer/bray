use bray_linker::{LinkOutcome, LinkPlan, Linker};

use super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};
use crate::QueryPriority;

impl Compilation {
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
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.state
            .fact_runtime
            .run(priority, || Ok(linker.link(plan, cancellation)))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    use bray_base::Cancellation;
    use bray_diagnostics::DiagnosticBag;
    use bray_linker::{
        BinarySymbolName, DebugLinkPolicy, LinkFailure, LinkInput, LinkInputId,
        LinkInputKind, LinkInputMode, LinkInputProvenance, LinkInputSource,
        LinkModel, LinkOutcome, LinkPlan, LinkPlanBuilder, LinkPolicy,
        LinkStatus, LinkTarget, LinkedArtifact, LinkedArtifactKind,
        LinkedArtifactRequirement, LinkedProductKind, Linker, LinkerDriver,
        LinkerDriverIdentity, LinkerDriverKind, PlannedLinkedArtifact,
        SectionGarbageCollectionPolicy, StagingDestination,
        StagingDestinationId, StagingPathKey,
    };
    use bray_runtime_interface::RuntimeArtifactId;
    use bray_symbols::{ProductIdentity, ProductKind};
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture,
        TargetIdentity,
    };
    use bray_testing::test_async_executable_host_contract_for;

    use super::Compilation;
    use crate::test_support::{package_identity, source_input};
    use crate::{
        CancellationToken, CompilationOptions, CompilationRequest,
        SelectedTarget, WorkerBudget,
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
    fn async_link_forwards_resolved_host_runtime_and_companion_facts() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);
        let runtime = runtime_artifact_id();

        let executable_host = test_async_executable_host_contract_for(
            product(),
            link_target().identity().clone(),
            runtime.clone(),
        );

        let mut builder = plan_builder(
            LinkedProductKind::Executable,
            DebugLinkPolicy::Companion,
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
            LinkInputProvenance::Runtime(runtime),
        ));

        builder.push_input(native_library(3, "pthread"));

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
        assert_eq!(plan.executable_host(), Some(&executable_host));
        assert_eq!(driver.plans(), vec![plan]);
    }

    #[test]
    fn cancellation_prevents_linker_invocation() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(Arc::clone(&driver) as Arc<dyn LinkerDriver>);
        let plan = executable_plan("cancelled.stage");
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let outcome = compilation(WorkerBudget::serial())
            .link_product_with_cancellation(&linker, &plan, &cancellation)
            .unwrap_or_else(|error| panic!("cancelled test link must return: {error:?}"));

        assert!(matches!(outcome.status(), LinkStatus::Cancelled));
        assert!(driver.plans().is_empty());
    }

    #[test]
    fn failed_links_expose_no_partial_staging_result() {
        let driver = Arc::new(RecordingDriver::failing());
        let linker = linker(driver as Arc<dyn LinkerDriver>);
        let plan = executable_plan("failed.stage");

        let outcome = compilation(WorkerBudget::serial())
            .link_product(&linker, &plan)
            .unwrap_or_else(|error| panic!("failed test link must return: {error:?}"));

        assert_eq!(
            outcome.status(),
            &LinkStatus::Failed(LinkFailure::Invocation)
        );

        assert_eq!(outcome.artifacts(), None);
    }

    #[test]
    fn repeated_links_have_deterministic_outcomes() {
        let driver = Arc::new(RecordingDriver::completing());
        let linker = linker(driver as Arc<dyn LinkerDriver>);
        let compilation = compilation(WorkerBudget::serial());
        let first_plan = executable_plan("first.stage");
        let second_plan = executable_plan("second.stage");

        let first = compilation
            .link_product(&linker, &first_plan)
            .unwrap_or_else(|error| panic!("first deterministic link must run: {error:?}"));

        let second = compilation
            .link_product(&linker, &second_plan)
            .unwrap_or_else(|error| panic!("second deterministic link must run: {error:?}"));

        assert_eq!(first.status(), second.status());
        assert_eq!(first.diagnostics(), second.diagnostics());
    }

    #[test]
    fn independent_links_run_concurrently_within_the_worker_budget() {
        let observation = Arc::new(ConcurrentLinkObservation::new(2));

        let driver = Arc::new(ConcurrentDriver {
            identity: driver_identity(),
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

            let first = scope.spawn(move || {
                first_compilation.link_product(&first_linker, &first_plan)
            });

            let second_compilation = compilation.clone();
            let second_linker = Arc::clone(&linker);

            let second = scope.spawn(move || {
                second_compilation.link_product(&second_linker, &second_plan)
            });

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

    fn shared_library_plan() -> LinkPlan {
        let mut builder = plan_builder(
            LinkedProductKind::SharedLibrary,
            DebugLinkPolicy::None,
        );

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

    fn plan_builder(
        product_kind: LinkedProductKind,
        debug: DebugLinkPolicy,
    ) -> LinkPlanBuilder {
        LinkPlanBuilder::new(
            product(),
            product_kind,
            link_target(),
            driver_identity(),
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

    fn native_library(ordinal: u32, name: &str) -> LinkInput {
        LinkInput::try_new(
            LinkInputId::new(ordinal),
            LinkInputKind::NativeLibrary,
            LinkInputSource::try_native_library(name)
                .unwrap_or_else(|| panic!("test native library name must be valid")),
            LinkInputProvenance::HostConfiguration,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test native library input must be valid: {error:?}"))
    }

    fn planned_output(
        ordinal: u32,
        kind: LinkedArtifactKind,
        path: &str,
    ) -> PlannedLinkedArtifact {
        let path_key = StagingPathKey::try_new(path)
            .unwrap_or_else(|| panic!("test staging path key must be valid"));

        let destination =
            StagingDestination::try_new(StagingDestinationId::new(ordinal), path, path_key)
                .unwrap_or_else(|error| {
                    panic!("test staging destination must be valid: {error:?}")
                });

        PlannedLinkedArtifact::new(
            kind,
            LinkedArtifactRequirement::Required,
            destination,
        )
    }

    fn synchronous_host() -> bray_runtime_interface::ExecutableHostContract {
        bray_testing::test_executable_host_contract_for(
            product(),
            link_target().identity().clone(),
        )
    }

    fn product() -> ProductIdentity {
        ProductIdentity::try_new(package_identity(), "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }

    fn link_target() -> LinkTarget {
        let identity = TargetIdentity::try_new("linux-x86_64")
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
        LinkerDriverIdentity::try_new(
            LinkerDriverKind::EmbeddedLld,
            "lld",
            "1",
            "20",
        )
        .unwrap_or_else(|| panic!("test linker identity must be valid"))
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
        identity: LinkerDriverIdentity,
        plans: Mutex<Vec<LinkPlan>>,
        completes: bool,
    }

    impl RecordingDriver {
        fn completing() -> Self {
            Self {
                identity: driver_identity(),
                plans: Mutex::new(Vec::new()),
                completes: true,
            }
        }

        fn failing() -> Self {
            Self {
                identity: driver_identity(),
                plans: Mutex::new(Vec::new()),
                completes: false,
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
        fn identity(&self) -> &LinkerDriverIdentity {
            &self.identity
        }

        fn supports(&self, _target: &LinkTarget, _product: LinkedProductKind) -> bool {
            true
        }

        fn link(
            &self,
            plan: &LinkPlan,
            _cancellation: &dyn Cancellation,
        ) -> LinkOutcome {
            self.plans
                .lock()
                .unwrap_or_else(|_| panic!("test plan log must remain available"))
                .push(plan.clone());

            if !self.completes {
                return LinkOutcome::failed(
                    LinkFailure::Invocation,
                    DiagnosticBag::new(),
                );
            }

            complete(plan)
        }
    }

    struct ConcurrentDriver {
        identity: LinkerDriverIdentity,
        observation: Arc<ConcurrentLinkObservation>,
    }

    impl LinkerDriver for ConcurrentDriver {
        fn identity(&self) -> &LinkerDriverIdentity {
            &self.identity
        }

        fn supports(&self, _target: &LinkTarget, _product: LinkedProductKind) -> bool {
            true
        }

        fn link(
            &self,
            plan: &LinkPlan,
            _cancellation: &dyn Cancellation,
        ) -> LinkOutcome {
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
        let artifacts = plan.outputs().iter().map(|output| {
            LinkedArtifact::new(
                output.kind(),
                output.destination().id(),
                NonZeroU64::MIN,
            )
        });

        LinkOutcome::try_complete(plan, artifacts, DiagnosticBag::new())
            .unwrap_or_else(|error| panic!("test link must complete: {error:?}"))
    }
}
