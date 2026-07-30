use std::collections::BTreeMap;
use std::path::PathBuf;

use bray_linker::{
    LinkInput, LinkInputBuildError, LinkInputId, LinkInputKind, LinkInputMode,
    LinkInputProvenance, LinkInputSource, LinkInputSpec, LinkPlan, LinkPlanBuildError,
    LinkPlanBuilder, LinkedArtifactKind, LinkedProductKind, PlannedLinkedArtifact,
    StagingDestination, StagingDestinationBuildError, StagingDestinationId, StagingPathKey,
};
use bray_target::TargetIdentity;

use super::{LinkOutputStaging, ProductLinkFacts, StagedArtifact};
use crate::{ArtifactId, ArtifactKind, ArtifactProducer, EmissionPlan};

/// Constructs one immutable native link plan without invoking a linker.
pub fn construct_link_plan(
    emission: &EmissionPlan,
    staged_artifacts: impl IntoIterator<Item = StagedArtifact>,
    output_staging: impl IntoIterator<Item = LinkOutputStaging>,
    facts: &ProductLinkFacts,
) -> Result<LinkPlan, LinkPlanConstructionError> {
    if emission.request().target() != facts.target.identity() {
        return Err(LinkPlanConstructionError::TargetMismatch {
            // The error owns both Arc-backed identities after construction returns.
            planned: emission.request().target().clone(),
            selected: facts.target.identity().clone(),
        });
    }

    let product_kind = linked_product_kind_for_plan(emission)?;
    let staged_artifacts = staged_artifacts_by_id(staged_artifacts)?;
    let output_staging = output_staging_by_id(output_staging)?;

    let constructor = LinkPlanConstructor::new(
        emission,
        product_kind,
        staged_artifacts,
        output_staging,
        facts,
    );

    constructor.build()
}

/// A conflict between an emission plan, staged artifacts, and resolved product link facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkPlanConstructionError {
    /// The emission plan contains no linked product.
    MissingLinkedProduct,
    /// Selected linker target facts belong to another emission target.
    TargetMismatch {
        /// Target selected by the immutable emission plan.
        planned: TargetIdentity,
        /// Target covered by the resolved linker facts.
        selected: TargetIdentity,
    },
    /// More than one staged record names the same planned artifact.
    DuplicateStagedArtifact(ArtifactId),
    /// One planned native input has no completed staged artifact.
    MissingStagedArtifact(ArtifactId),
    /// A staged artifact is not a private native input in the emission plan.
    UnexpectedStagedArtifact(ArtifactId),
    /// A planned staged artifact has no native linker representation.
    UnsupportedStagedArtifact(ArtifactId),
    /// More than one output staging record names the same planned artifact.
    DuplicateOutputStaging(ArtifactId),
    /// One linker-produced artifact has no staging destination.
    MissingOutputStaging(ArtifactId),
    /// An output staging record does not name a linker-produced artifact.
    UnexpectedOutputStaging(ArtifactId),
    /// A linked output category is incompatible with its planned artifact.
    OutputKindMismatch {
        /// Planned emitter artifact.
        artifact: ArtifactId,
        /// Supplied native linked artifact category.
        kind: LinkedArtifactKind,
    },
    /// A target startup input has an incompatible native category.
    InvalidStartupInputKind(LinkInputKind),
    /// A resolved native input has an incompatible native category.
    InvalidNativeInputKind(LinkInputKind),
    /// A target termination input has an incompatible native category.
    InvalidTerminationInputKind(LinkInputKind),
    /// The selected runtime archive does not match the executable-host contract.
    RuntimeContractMismatch,
    /// The link input count exceeds its stable identity range.
    InputOrdinalOverflow,
    /// The linked output count exceeds its stable staging identity range.
    OutputOrdinalOverflow,
    /// A staged artifact could not form a valid native input.
    InvalidInput(LinkInputBuildError),
    /// A linked output could not form a valid staging destination.
    InvalidOutputStaging(StagingDestinationBuildError),
    /// The mapped inputs and outputs violate the native link-plan contract.
    InvalidLinkPlan(LinkPlanBuildError),
}

struct LinkPlanConstructor<'plan> {
    emission: &'plan EmissionPlan,
    staged_artifacts: BTreeMap<ArtifactId, PathBuf>,
    output_staging:
        BTreeMap<ArtifactId, (LinkedArtifactKind, PathBuf, StagingPathKey)>,
    facts: &'plan ProductLinkFacts,
    builder: LinkPlanBuilder,
    next_input: u32,
}

impl<'plan> LinkPlanConstructor<'plan> {
    fn new(
        emission: &'plan EmissionPlan,
        product_kind: LinkedProductKind,
        staged_artifacts: BTreeMap<ArtifactId, PathBuf>,
        output_staging: BTreeMap<
            ArtifactId,
            (LinkedArtifactKind, PathBuf, StagingPathKey),
        >,
        facts: &'plan ProductLinkFacts,
    ) -> Self {
        let builder = LinkPlanBuilder::new(
            // The link plan owns the Arc-backed product identity independently of the emission plan.
            emission.request().product().clone(),
            product_kind,
            // The link plan owns target facts independently of the supplied product facts.
            facts.target.clone(),
            // Driver identity participates in the immutable plan and its cache identity.
            facts.driver.clone(),
            facts.policy,
        );

        Self {
            emission,
            staged_artifacts,
            output_staging,
            facts,
            builder,
            next_input: 0,
        }
    }

    fn build(mut self) -> Result<LinkPlan, LinkPlanConstructionError> {
        self.push_startup_inputs()?;
        self.push_staged_inputs()?;
        self.push_runtime_input()?;
        self.push_native_inputs()?;
        self.push_termination_inputs()?;
        self.push_outputs()?;
        self.push_product_facts();

        if let Some((artifact, _)) = self.staged_artifacts.pop_first() {
            return Err(LinkPlanConstructionError::UnexpectedStagedArtifact(
                artifact,
            ));
        }

        if let Some((artifact, _)) = self.output_staging.pop_first() {
            return Err(LinkPlanConstructionError::UnexpectedOutputStaging(
                artifact,
            ));
        }

        self.builder
            .finish()
            .map_err(LinkPlanConstructionError::InvalidLinkPlan)
    }

    fn push_startup_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        // The link plan owns input specifications independently of the product-fact borrow.
        for input in self.facts.startup_inputs.iter().cloned() {
            if input.kind() != LinkInputKind::StartupObject {
                return Err(LinkPlanConstructionError::InvalidStartupInputKind(
                    input.kind(),
                ));
            }

            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_staged_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        for planned in self.emission.staged_artifacts() {
            let Some(path) = self.staged_artifacts.remove(planned.id()) else {
                // Construction errors retain the Arc-backed artifact identity.
                return Err(LinkPlanConstructionError::MissingStagedArtifact(
                    planned.id().clone(),
                ));
            };

            let kind = match planned.id().kind() {
                ArtifactKind::RelocatableObject => LinkInputKind::RelocatableObject,
                ArtifactKind::BackendBitcode => LinkInputKind::Bitcode,
                _ => {
                    // Construction errors retain the Arc-backed artifact identity.
                    return Err(LinkPlanConstructionError::UnsupportedStagedArtifact(
                        planned.id().clone(),
                    ));
                }
            };

            let input = LinkInputSpec::try_new(
                kind,
                LinkInputSource::file(path),
                LinkInputProvenance::Product,
                LinkInputMode::Ordinary,
            )
            .map_err(LinkPlanConstructionError::InvalidInput)?;

            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_runtime_input(&mut self) -> Result<(), LinkPlanConstructionError> {
        let Some(runtime) = &self.facts.runtime else {
            return Ok(());
        };

        if let Some(host) = self.emission.request().executable_host()
            && (runtime.contract().validate(host.requirements()).is_err()
                || host
                    .runtime()
                    .is_some_and(|selected| selected != runtime.contract()))
        {
            return Err(LinkPlanConstructionError::RuntimeContractMismatch);
        }

        let id = self.next_input_id()?;

        self.builder
            .push_input(LinkInput::runtime_component(id, runtime));

        Ok(())
    }

    fn push_native_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        // The link plan owns input specifications independently of the product-fact borrow.
        for input in self.facts.native_inputs.iter().cloned() {
            if !matches!(
                input.kind(),
                LinkInputKind::Archive
                    | LinkInputKind::NativeLibrary
                    | LinkInputKind::Framework
            ) {
                return Err(LinkPlanConstructionError::InvalidNativeInputKind(
                    input.kind(),
                ));
            }

            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_termination_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        // The link plan owns input specifications independently of the product-fact borrow.
        for input in self.facts.termination_inputs.iter().cloned() {
            if input.kind() != LinkInputKind::TerminationObject {
                return Err(
                    LinkPlanConstructionError::InvalidTerminationInputKind(
                        input.kind(),
                    ),
                );
            }

            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_outputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        let mut destination_ordinal = 0_u32;

        for planned in self.emission.artifacts().iter().filter(|artifact| {
            matches!(artifact.producer(), ArtifactProducer::Linker(_))
        }) {
            let Some((kind, path, path_key)) = self.output_staging.remove(planned.id()) else {
                // Construction errors retain the Arc-backed artifact identity.
                return Err(LinkPlanConstructionError::MissingOutputStaging(
                    planned.id().clone(),
                ));
            };

            if !planned.id().kind().accepts_linked_kind(kind) {
                return Err(LinkPlanConstructionError::OutputKindMismatch {
                    // Construction errors retain the Arc-backed artifact identity.
                    artifact: planned.id().clone(),
                    kind,
                });
            }

            let destination = StagingDestination::try_new(
                StagingDestinationId::new(destination_ordinal),
                path,
                path_key,
            )
            .map_err(LinkPlanConstructionError::InvalidOutputStaging)?;

            self.builder.push_output(PlannedLinkedArtifact::new(
                kind,
                planned.requirement().linked(),
                destination,
            ));

            destination_ordinal = destination_ordinal
                .checked_add(1)
                .ok_or(LinkPlanConstructionError::OutputOrdinalOverflow)?;
        }

        Ok(())
    }

    fn push_product_facts(&mut self) {
        if let Some(entry_point) = &self.facts.entry_point {
            // The completed link plan owns the Arc-backed binary name.
            self.builder.set_entry_point(entry_point.clone());
        }

        if let Some(host) = self.emission.request().executable_host() {
            // The completed link plan shares the immutable executable-host contract.
            self.builder.set_executable_host(host.clone());
        }

        // The link plan owns binary names independently of the product-fact borrow.
        for symbol in self.facts.exported_symbols.iter().cloned() {
            self.builder.push_exported_symbol(symbol);
        }

        for symbol in self.facts.retained_symbols.iter().cloned() {
            self.builder.push_retained_symbol(symbol);
        }

        // Search-path values must remain available after product facts are released.
        for path in self.facts.search_paths.iter().cloned() {
            self.builder.push_search_path(path);
        }
    }

    fn push_input(&mut self, input: LinkInputSpec) -> Result<(), LinkPlanConstructionError> {
        let id = self.next_input_id()?;

        self.builder.push_input(input.with_id(id));

        Ok(())
    }

    fn next_input_id(&mut self) -> Result<LinkInputId, LinkPlanConstructionError> {
        let id = LinkInputId::new(self.next_input);

        self.next_input = self
            .next_input
            .checked_add(1)
            .ok_or(LinkPlanConstructionError::InputOrdinalOverflow)?;

        Ok(id)
    }
}

fn linked_product_kind_for_plan(
    emission: &EmissionPlan,
) -> Result<LinkedProductKind, LinkPlanConstructionError> {
    emission
        .artifacts()
        .iter()
        .find_map(|artifact| linked_product_kind(artifact.id().kind()))
        .ok_or(LinkPlanConstructionError::MissingLinkedProduct)
}

const fn linked_product_kind(kind: ArtifactKind) -> Option<LinkedProductKind> {
    match kind {
        ArtifactKind::Executable => Some(LinkedProductKind::Executable),
        ArtifactKind::StaticLibrary => Some(LinkedProductKind::StaticLibrary),
        ArtifactKind::SharedLibrary => Some(LinkedProductKind::SharedLibrary),
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::ExecutableModule
        | ArtifactKind::DebugCompanion
        | ArtifactKind::PackageInterface
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::LinkedCompanion => None,
    }
}

fn staged_artifacts_by_id(
    staged_artifacts: impl IntoIterator<Item = StagedArtifact>,
) -> Result<BTreeMap<ArtifactId, PathBuf>, LinkPlanConstructionError> {
    let mut by_id = BTreeMap::new();

    for staged in staged_artifacts {
        match by_id.entry(staged.artifact) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(staged.path);
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                // Construction errors retain the Arc-backed artifact identity.
                return Err(LinkPlanConstructionError::DuplicateStagedArtifact(
                    entry.key().clone(),
                ));
            }
        }
    }

    Ok(by_id)
}

fn output_staging_by_id(
    outputs: impl IntoIterator<Item = LinkOutputStaging>,
) -> Result<
    BTreeMap<ArtifactId, (LinkedArtifactKind, PathBuf, StagingPathKey)>,
    LinkPlanConstructionError,
> {
    let mut by_id = BTreeMap::new();

    for output in outputs {
        match by_id.entry(output.artifact) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((output.kind, output.path, output.path_key));
            }
            std::collections::btree_map::Entry::Occupied(entry) => {
                // Construction errors retain the Arc-backed artifact identity.
                return Err(LinkPlanConstructionError::DuplicateOutputStaging(
                    entry.key().clone(),
                ));
            }
        }
    }

    Ok(by_id)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use bray_base::Cancellation;
    use bray_codegen::{
        AssemblySyntaxKind, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind,
    };
    use bray_diagnostics::DiagnosticBag;
    use bray_linker::{
        DeadStripPolicy, DebugLinkPolicy, LinkFailure, LinkInputKind, LinkInputMode,
        LinkInputProvenance, LinkInputSource, LinkInputSpec, LinkModel, LinkOutcome,
        LinkPolicy, LinkSearchPath, LinkSearchPathKind, LinkSubsystem,
        LinkTarget, LinkedArtifactKind, LinkedProductKind, Linker, LinkerDriver,
        LinkerDriverIdentity, LinkerDriverKind, SectionGarbageCollectionPolicy,
        StagingPathKey,
    };
    use bray_runtime_interface::{
        BinarySymbolName, RootExecution, RuntimeAbiRole, RuntimeArtifact,
        RuntimeArtifactDigest, RuntimeArtifactMetadata, RuntimeArtifactId,
        RuntimeCapability,
    };
    use bray_target::{
        CodeModel, RelocationModel,
    };

    use super::{
        LinkOutputStaging, LinkPlanConstructionError, ProductLinkFacts,
        StagedArtifact, construct_link_plan,
    };
    use crate::test_support::{
        backend_capabilities, backend_identity, codegen_unit_key, interface_artifact,
        product_identity, target_identity, target_output_description,
    };
    use crate::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend,
        EmissionPlan, EmissionPlanner, EmissionRequest, ProductKind,
        RequestedArtifact, RequestedArtifactDestination, ReplacementPolicy,
    };

    #[test]
    fn link_plans_preserve_canonical_input_order_and_exclude_package_interfaces() {
        let plan = library_plan();
        let staged = staged_artifacts(&plan);
        let outputs = output_staging(&plan);

        let facts = product_link_facts()
            .with_startup_inputs([file_input(
                LinkInputKind::StartupObject,
                "crt/start.o",
                LinkInputProvenance::TargetProfile,
            )])
            .with_native_inputs([
                file_input(
                    LinkInputKind::Archive,
                    "dependencies/support.a",
                    LinkInputProvenance::HostConfiguration,
                ),
                native_library("pthread"),
            ])
            .with_termination_inputs([file_input(
                LinkInputKind::TerminationObject,
                "crt/end.o",
                LinkInputProvenance::TargetProfile,
            )])
            .with_entry_point(binary_symbol("library_initialize"))
            .with_exported_symbols([
                binary_symbol("zeta"),
                binary_symbol("alpha"),
            ])
            .with_search_paths([
                search_path(LinkSearchPathKind::Library, "dependencies"),
                search_path(LinkSearchPathKind::Framework, "frameworks"),
            ]);

        let link_plan = construct_link_plan(
            &plan,
            staged.iter().cloned().rev(),
            outputs.iter().cloned().rev(),
            &facts,
        )
        .unwrap_or_else(|error| panic!("link plan must construct: {error:?}"));

        assert_eq!(
            link_plan
                .inputs()
                .iter()
                .map(|input| input.kind())
                .collect::<Vec<_>>(),
            [
                LinkInputKind::StartupObject,
                LinkInputKind::RelocatableObject,
                LinkInputKind::RelocatableObject,
                LinkInputKind::Archive,
                LinkInputKind::NativeLibrary,
                LinkInputKind::TerminationObject,
            ]
        );

        assert_eq!(
            link_plan.exported_symbols(),
            [binary_symbol("alpha"), binary_symbol("zeta")]
        );

        assert_eq!(link_plan.product_kind(), LinkedProductKind::SharedLibrary);

        assert_eq!(
            link_plan
                .outputs()
                .iter()
                .map(|output| output.kind())
                .collect::<Vec<_>>(),
            [
                LinkedArtifactKind::SharedLibrary,
                LinkedArtifactKind::PlatformCompanion,
            ]
        );

        assert!(
            plan.artifacts()
                .iter()
                .any(|artifact| artifact.id().kind() == ArtifactKind::PackageInterface)
        );

        assert!(
            link_plan.inputs().iter().all(|input| {
                !matches!(
                    input.source(),
                    LinkInputSource::File(path)
                        if path.extension().is_some_and(|extension| extension == "brayi")
                )
            })
        );
    }

    #[test]
    fn async_executable_plans_preserve_runtime_and_host_contracts() {
        let (plan, runtime) = async_executable_plan();

        let host = plan
            .request()
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must have a host contract"));

        let facts = product_link_facts().with_runtime(runtime.clone());

        let link_plan = construct_link_plan(
            &plan,
            staged_artifacts(&plan),
            output_staging(&plan),
            &facts,
        )
        .unwrap_or_else(|error| panic!("async link plan must construct: {error:?}"));

        let linked_host = link_plan
            .executable_host()
            .unwrap_or_else(|| panic!("async link plan must retain its host contract"));

        assert_eq!(link_plan.entry_point(), Some(host.native_entry()));
        assert_eq!(linked_host, host);

        let runtime_inputs = link_plan
            .inputs()
            .iter()
            .filter(|input| {
                matches!(input.provenance(), LinkInputProvenance::Runtime(_))
            })
            .collect::<Vec<_>>();

        let [runtime_input] = runtime_inputs.as_slice() else {
            panic!("async link plan must contain exactly one runtime input");
        };

        assert_eq!(
            runtime_input.provenance(),
            &LinkInputProvenance::Runtime(runtime.contract().artifact().clone())
        );

        assert_eq!(
            linked_host.abi_version(),
            runtime.contract().abi_version()
        );

        assert!(matches!(
            linked_host.root(),
            RootExecution::Asynchronous { .. }
        ));

        assert_eq!(
            linked_host.runtime_capabilities(),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ]
        );

        for role in [
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::MainThreadLaneDrive,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            assert!(linked_host.role_binding(role).is_some());
        }
    }

    #[test]
    fn construction_rejects_incomplete_and_foreign_staging() {
        let plan = library_plan();
        let mut staged = staged_artifacts(&plan);

        let missing = staged
            .pop()
            .unwrap_or_else(|| panic!("test plan must have staged inputs"));

        assert_eq!(
            construct_link_plan(
                &plan,
                staged,
                output_staging(&plan),
                &product_link_facts(),
            ),
            Err(LinkPlanConstructionError::MissingStagedArtifact(
                missing.artifact().clone()
            ))
        );

        let mut outputs = output_staging(&plan);

        let missing_output = outputs
            .pop()
            .unwrap_or_else(|| panic!("test plan must have linked outputs"));

        assert_eq!(
            construct_link_plan(
                &plan,
                staged_artifacts(&plan),
                outputs,
                &product_link_facts(),
            ),
            Err(LinkPlanConstructionError::MissingOutputStaging(
                missing_output.artifact().clone()
            ))
        );

        let interface = plan
            .artifacts()
            .iter()
            .find(|artifact| artifact.id().kind() == ArtifactKind::PackageInterface)
            .unwrap_or_else(|| panic!("test plan must contain a package interface"));

        let foreign = StagedArtifact::try_new(
            interface.id().clone(),
            "stage/application.brayi",
        )
        .unwrap_or_else(|error| panic!("test staged artifact must be valid: {error:?}"));

        assert_eq!(
            construct_link_plan(
                &plan,
                staged_artifacts(&plan)
                    .into_iter()
                    .chain([foreign]),
                output_staging(&plan),
                &product_link_facts(),
            ),
            Err(LinkPlanConstructionError::UnexpectedStagedArtifact(
                interface.id().clone()
            ))
        );
    }

    #[test]
    fn construction_is_deterministic_and_never_invokes_a_linker() {
        let plan = library_plan();
        let staged = staged_artifacts(&plan);
        let outputs = output_staging(&plan);
        let facts = product_link_facts();
        let invocations = Arc::new(AtomicUsize::new(0));

        let driver = Arc::new(CountingDriver {
            identity: facts.driver.clone(),
            invocations: Arc::clone(&invocations),
        });

        let linker = Linker::try_new([driver as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test linker must construct: {error:?}"));

        let first = construct_link_plan(
            &plan,
            staged.iter().cloned(),
            outputs.iter().cloned(),
            &facts,
        );

        let second = construct_link_plan(
            &plan,
            staged.into_iter().rev(),
            outputs.into_iter().rev(),
            &facts,
        );

        assert_eq!(first, second);
        assert_eq!(invocations.load(Ordering::SeqCst), 0);

        let link_plan =
            first.unwrap_or_else(|error| panic!("test link plan must construct: {error:?}"));

        let _ = linker.link(&link_plan, &|| false);

        assert_eq!(invocations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn link_plan_inputs_are_immutable_worker_safe_values() {
        assert_send_sync::<ProductLinkFacts>();
        assert_send_sync::<StagedArtifact>();
        assert_send_sync::<LinkOutputStaging>();
    }

    fn library_plan() -> EmissionPlan {
        let request = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Library,
            None,
            target_identity(),
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(
                    ArtifactKind::SharedLibrary,
                    ArtifactRequirement::Required,
                ),
                RequestedArtifact::new(
                    ArtifactKind::PackageInterface,
                    ArtifactRequirement::Required,
                ),
                RequestedArtifact::new(
                    ArtifactKind::LinkedCompanion,
                    ArtifactRequirement::Required,
                ),
            ],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        emission_planner(Some(interface_artifact()))
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must construct: {error:?}"))
    }

    fn async_executable_plan() -> (EmissionPlan, RuntimeArtifact) {
        let Some(runtime_id) = RuntimeArtifactId::try_new("runtime.test") else {
            panic!("test runtime artifact identity must be valid");
        };

        let host = bray_testing::test_async_executable_host_contract_for(
            product_identity(),
            target_identity(),
            runtime_id,
        );

        let runtime = runtime_artifact(&host);

        let request = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Executable,
            Some(host),
            target_identity(),
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::Executable,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        let plan = emission_planner(None)
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must construct: {error:?}"));

        (plan, runtime)
    }

    fn emission_planner(
        interface: Option<bray_package_interface::InterfaceArtifact>,
    ) -> EmissionPlanner {
        let policy = BackendEmissionPolicy::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Omit,
            Some(LinkableArtifactKind::RelocatableObject),
            BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
        );

        let backend = EmissionBackend::try_new(
            backend_identity(),
            backend_capabilities(),
            [codegen_unit_key(2), codegen_unit_key(1)],
            policy,
        )
        .unwrap_or_else(|error| panic!("test emission backend must be valid: {error:?}"));

        EmissionPlanner::new(
            target_output_description(),
            Some(backend),
            interface,
        )
    }

    fn staged_artifacts(plan: &EmissionPlan) -> Vec<StagedArtifact> {
        plan.staged_artifacts()
            .enumerate()
            .map(|(index, artifact)| {
                StagedArtifact::try_new(
                    artifact.id().clone(),
                    format!("stage/input-{index}.o"),
                )
                .unwrap_or_else(|error| {
                    panic!("test staged artifact must be valid: {error:?}")
                })
            })
            .collect()
    }

    fn output_staging(plan: &EmissionPlan) -> Vec<LinkOutputStaging> {
        plan.artifacts()
            .iter()
            .filter(|artifact| {
                matches!(artifact.producer(), crate::ArtifactProducer::Linker(_))
            })
            .enumerate()
            .map(|(index, artifact)| {
                let kind = match artifact.id().kind() {
                    ArtifactKind::Executable => LinkedArtifactKind::Executable,
                    ArtifactKind::SharedLibrary => LinkedArtifactKind::SharedLibrary,
                    ArtifactKind::StaticLibrary => LinkedArtifactKind::StaticLibrary,
                    ArtifactKind::LinkedCompanion => LinkedArtifactKind::PlatformCompanion,
                    _ => panic!("test linker output kind must be supported"),
                };

                let key = StagingPathKey::try_new(format!("output-{index}.stage"))
                    .unwrap_or_else(|| panic!("test staging path key must be valid"));

                LinkOutputStaging::try_new(
                    artifact.id().clone(),
                    kind,
                    format!("stage/output-{index}"),
                    key,
                )
                .unwrap_or_else(|error| {
                    panic!("test output staging must be valid: {error:?}")
                })
            })
            .collect()
    }

    fn product_link_facts() -> ProductLinkFacts {
        ProductLinkFacts::new(
            link_target(),
            linker_driver_identity(),
            LinkPolicy::new(
                DeadStripPolicy::RemoveUnreachable,
                SectionGarbageCollectionPolicy::RemoveUnreferenced,
                DebugLinkPolicy::None,
                Some(LinkSubsystem::Console),
            ),
        )
    }

    fn link_target() -> LinkTarget {
        let profile = bray_target::test_support::test_target_profile();

        LinkTarget::try_new(
            profile.identity().clone(),
            "x86_64-unknown-linux-gnu",
            profile.machine().architecture(),
            profile.machine().object_format(),
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }

    fn linker_driver_identity() -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(
            LinkerDriverKind::EmbeddedLld,
            "lld",
            "1",
            "22",
        )
        .unwrap_or_else(|| panic!("test linker driver identity must be valid"))
    }

    fn file_input(
        kind: LinkInputKind,
        path: &str,
        provenance: LinkInputProvenance,
    ) -> LinkInputSpec {
        LinkInputSpec::try_new(
            kind,
            LinkInputSource::file(path),
            provenance,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test link input must be valid: {error:?}"))
    }

    fn native_library(name: &str) -> LinkInputSpec {
        let source = LinkInputSource::try_native_library(name)
            .unwrap_or_else(|| panic!("test native library name must be valid"));

        LinkInputSpec::try_new(
            LinkInputKind::NativeLibrary,
            source,
            LinkInputProvenance::HostConfiguration,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test native link input must be valid: {error:?}"))
    }

    fn search_path(kind: LinkSearchPathKind, path: &str) -> LinkSearchPath {
        LinkSearchPath::try_new(kind, path)
            .unwrap_or_else(|error| panic!("test search path must be valid: {error:?}"))
    }

    fn binary_symbol(name: &str) -> BinarySymbolName {
        BinarySymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test binary symbol name must be valid"))
    }

    fn runtime_artifact(
        host: &bray_runtime_interface::ExecutableHostContract,
    ) -> RuntimeArtifact {
        let contract = host
            .runtime()
            .cloned()
            .unwrap_or_else(|| panic!("async test host must select a runtime"));

        let digest = RuntimeArtifactDigest::new([7; 32]);

        let metadata = RuntimeArtifactMetadata::try_new(
            contract,
            "bray_runtime.a",
            digest,
        )
        .unwrap_or_else(|error| panic!("test runtime metadata must be valid: {error:?}"));

        RuntimeArtifact::try_new(metadata, "runtime/bray_runtime.a", digest)
            .unwrap_or_else(|error| panic!("test runtime artifact must be valid: {error:?}"))
    }

    struct CountingDriver {
        identity: LinkerDriverIdentity,
        invocations: Arc<AtomicUsize>,
    }

    impl LinkerDriver for CountingDriver {
        fn identity(&self) -> &LinkerDriverIdentity {
            &self.identity
        }

        fn supports(
            &self,
            _target: &LinkTarget,
            _product: LinkedProductKind,
        ) -> bool {
            true
        }

        fn link(
            &self,
            _plan: &bray_linker::LinkPlan,
            _cancellation: &dyn Cancellation,
        ) -> LinkOutcome {
            self.invocations.fetch_add(1, Ordering::SeqCst);

            LinkOutcome::failed(LinkFailure::Invocation, DiagnosticBag::new())
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
