use std::collections::BTreeMap;
use std::path::PathBuf;

use bray_linker::{
    DebugLinkPolicy, LinkInput, LinkInputBuildError, LinkInputId, LinkInputKind, LinkInputMode,
    LinkInputProvenance, LinkInputSource, LinkInputSpec, LinkPlan, LinkPlanBuildError,
    LinkPlanBuilder, LinkPlanSelectionError, LinkPolicy, LinkStartupMode, LinkedArtifactKind,
    LinkedProductKind, Linker, LinkerDriverIdentity, PlannedLinkedArtifact, StagingDestination,
    StagingDestinationBuildError, StagingDestinationId, StagingPathKey,
};
use bray_target::TargetIdentity;

use super::{LinkOutputStaging, ProductLinkInputs, StagedArtifact};
use crate::{ArtifactId, ArtifactKind, ArtifactProducer, EmissionPlan};

/// Constructs one immutable native link plan without invoking a linker.
pub fn construct_link_plan(
    emission: &EmissionPlan,
    staged_artifacts: impl IntoIterator<Item = StagedArtifact>,
    output_staging: impl IntoIterator<Item = LinkOutputStaging>,
    inputs: &ProductLinkInputs,
    linker: &Linker,
) -> Result<LinkPlan, LinkPlanConstructionError> {
    if emission.request().target() != inputs.target.identity() {
        return Err(LinkPlanConstructionError::TargetMismatch {
            // The error owns both Arc-backed identities after construction returns.
            planned: emission.request().target().clone(),
            selected: inputs.target.identity().clone(),
        });
    }

    let product_kind = linked_product_kind_for_plan(emission)?;
    let staged_artifacts = staged_artifacts_by_id(staged_artifacts)?;
    let output_staging = output_staging_by_id(output_staging)?;

    let startup_mode = startup_mode(product_kind, inputs);

    linker
        .select_plan(|driver| {
            // Every candidate owns its small path maps and Arc-backed driver identity so it
            // remains a complete immutable plan independently of selection.
            LinkPlanConstructor::new(
                emission,
                product_kind,
                staged_artifacts.clone(),
                output_staging.clone(),
                inputs,
                driver.clone(),
                startup_mode,
            )
            .build()
        })
        .map_err(|error| match error {
            LinkPlanSelectionError::Construction(error) => error,
            LinkPlanSelectionError::Link(failure) => LinkPlanConstructionError::Linker(failure),
        })
}

/// A conflict between an emission plan, staged artifacts, and resolved product link inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkPlanConstructionError {
    /// The emission plan contains no linked product.
    MissingLinkedProduct,
    /// Selected linker target inputs belong to another emission target.
    TargetMismatch {
        /// Target selected by the immutable emission plan.
        planned: TargetIdentity,
        /// Target covered by the resolved linker inputs.
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
    /// No configured linker driver accepts the complete immutable plan.
    Linker(bray_linker::LinkFailure),
}

struct LinkPlanConstructor<'plan> {
    emission: &'plan EmissionPlan,
    product_kind: LinkedProductKind,
    staged_artifacts: BTreeMap<ArtifactId, PathBuf>,
    output_staging: BTreeMap<ArtifactId, (LinkedArtifactKind, PathBuf, StagingPathKey)>,
    inputs: &'plan ProductLinkInputs,
    builder: LinkPlanBuilder,
    next_input: u32,
}

impl<'plan> LinkPlanConstructor<'plan> {
    fn new(
        emission: &'plan EmissionPlan,
        product_kind: LinkedProductKind,
        staged_artifacts: BTreeMap<ArtifactId, PathBuf>,
        output_staging: BTreeMap<ArtifactId, (LinkedArtifactKind, PathBuf, StagingPathKey)>,
        inputs: &'plan ProductLinkInputs,
        driver: LinkerDriverIdentity,
        startup_mode: LinkStartupMode,
    ) -> Self {
        let policy = link_policy(inputs.policy, &output_staging);

        let builder = LinkPlanBuilder::new(
            // The link plan owns the Arc-backed product identity independently of the emission plan.
            emission.request().product().clone(),
            product_kind,
            // The link plan owns target inputs independently of the supplied product inputs.
            inputs.target.clone(),
            driver,
            startup_mode,
            policy,
        );

        Self {
            emission,
            product_kind,
            staged_artifacts,
            output_staging,
            inputs,
            builder,
            next_input: 0,
        }
    }

    fn build(mut self) -> Result<LinkPlan, LinkPlanConstructionError> {
        self.push_startup_inputs()?;
        self.push_staged_inputs()?;
        self.validate_native_inputs()?;
        self.push_runtime_input()?;
        self.push_native_inputs_matching(is_ordinary_archive_input)?;
        self.push_native_inputs_matching(is_platform_provider_input)?;
        self.push_native_inputs_matching(is_native_library_input)?;
        self.push_termination_inputs()?;
        self.push_outputs()?;
        self.push_product_inputs();

        if let Some((artifact, _)) = self.staged_artifacts.pop_first() {
            return Err(LinkPlanConstructionError::UnexpectedStagedArtifact(
                artifact,
            ));
        }

        if let Some((artifact, _)) = self.output_staging.pop_first() {
            return Err(LinkPlanConstructionError::UnexpectedOutputStaging(artifact));
        }

        self.builder
            .finish()
            .map_err(LinkPlanConstructionError::InvalidLinkPlan)
    }

    fn push_startup_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        // The link plan owns input specifications independently of the product-input borrow.
        for input in self.inputs.startup_inputs.iter().cloned() {
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
        if self.product_kind == LinkedProductKind::StaticLibrary {
            return Ok(());
        }

        let Some(runtime) = &self.inputs.runtime else {
            return Ok(());
        };

        for component in runtime.components() {
            let id = self.next_input_id()?;

            self.builder.push_input(LinkInput::runtime_component(
                id,
                runtime.contract().artifact(),
                component,
            ));
        }

        Ok(())
    }

    fn validate_native_inputs(&self) -> Result<(), LinkPlanConstructionError> {
        for input in self.inputs.native_inputs.iter() {
            if !matches!(
                input.kind(),
                LinkInputKind::Archive | LinkInputKind::NativeLibrary | LinkInputKind::Framework
            ) {
                return Err(LinkPlanConstructionError::InvalidNativeInputKind(
                    input.kind(),
                ));
            }
        }

        Ok(())
    }

    fn push_native_inputs_matching(
        &mut self,
        include: fn(&LinkInputSpec) -> bool,
    ) -> Result<(), LinkPlanConstructionError> {
        if self.product_kind == LinkedProductKind::StaticLibrary {
            return Ok(());
        }

        // The link plan owns input specifications independently of the product-input borrow.
        for input in self
            .inputs
            .native_inputs
            .iter()
            .filter(|input| include(input))
            .cloned()
            .collect::<Vec<_>>()
        {
            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_termination_inputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        // The link plan owns input specifications independently of the product-input borrow.
        for input in self.inputs.termination_inputs.iter().cloned() {
            if input.kind() != LinkInputKind::TerminationObject {
                return Err(LinkPlanConstructionError::InvalidTerminationInputKind(
                    input.kind(),
                ));
            }

            self.push_input(input)?;
        }

        Ok(())
    }

    fn push_outputs(&mut self) -> Result<(), LinkPlanConstructionError> {
        let mut destination_ordinal = 0_u32;

        for planned in self
            .emission
            .artifacts()
            .iter()
            .filter(|artifact| matches!(artifact.producer(), ArtifactProducer::Linker(_)))
        {
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

    fn push_product_inputs(&mut self) {
        if self.product_kind == LinkedProductKind::StaticLibrary {
            return;
        }

        if let Some(runtime) = &self.inputs.runtime {
            // The completed link plan owns the selected runtime artifact identity.
            self.builder
                .set_runtime_artifact(runtime.contract().artifact().clone());
        }

        if let Some(entry_point) = &self.inputs.entry_point {
            // The completed link plan owns the Arc-backed binary name.
            self.builder.set_entry_point(entry_point.clone());
        }

        // The link plan owns binary names independently of the product-input borrow.
        for symbol in self.inputs.exported_symbols.iter().cloned() {
            self.builder.push_exported_symbol(symbol);
        }

        for symbol in self.inputs.retained_symbols.iter().cloned() {
            self.builder.push_retained_symbol(symbol);
        }

        // Search-path values must remain available after product inputs are released.
        for path in self.inputs.search_paths.iter().cloned() {
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

fn link_policy(
    policy: LinkPolicy,
    outputs: &BTreeMap<ArtifactId, (LinkedArtifactKind, PathBuf, StagingPathKey)>,
) -> LinkPolicy {
    let debug = if outputs
        .values()
        .any(|(kind, _, _)| *kind == LinkedArtifactKind::DebugCompanion)
    {
        DebugLinkPolicy::Companion
    } else {
        policy.debug()
    };

    LinkPolicy::new(
        policy.dead_strip(),
        policy.section_garbage_collection(),
        debug,
        policy.subsystem(),
    )
    .with_optimization(policy.optimization())
}

fn startup_mode(product: LinkedProductKind, inputs: &ProductLinkInputs) -> LinkStartupMode {
    match product {
        LinkedProductKind::StaticLibrary => LinkStartupMode::NotApplicable,
        LinkedProductKind::Executable | LinkedProductKind::SharedLibrary
            if inputs.startup_inputs.is_empty() && inputs.termination_inputs.is_empty() =>
        {
            LinkStartupMode::PlatformCompilerDriver
        }
        LinkedProductKind::Executable | LinkedProductKind::SharedLibrary => {
            LinkStartupMode::ExplicitInputs
        }
    }
}

fn is_ordinary_archive_input(input: &LinkInputSpec) -> bool {
    input.kind() == LinkInputKind::Archive
        && !matches!(input.provenance(), LinkInputProvenance::PlatformProvider(_))
}

fn is_platform_provider_input(input: &LinkInputSpec) -> bool {
    input.kind() == LinkInputKind::Archive
        && matches!(input.provenance(), LinkInputProvenance::PlatformProvider(_))
}

fn is_native_library_input(input: &LinkInputSpec) -> bool {
    matches!(
        input.kind(),
        LinkInputKind::NativeLibrary | LinkInputKind::Framework
    )
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
        | ArtifactKind::PackageImplementation
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::TestCatalog
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
    use std::collections::BTreeMap;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_base::Cancellation;
    use bray_codegen::{
        AssemblySyntaxKind, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind,
    };
    use bray_linker::{
        DeadStripPolicy, DebugLinkPolicy, LinkCancellationCapability, LinkDeterminismCapability,
        LinkEnvironmentCapability, LinkFailure, LinkInputKind, LinkInputMode, LinkInputProvenance,
        LinkInputSource, LinkInputSpec, LinkModel, LinkOutcome, LinkPlanCapability, LinkPolicy,
        LinkResponseFileCapability, LinkSearchPath, LinkSearchPathKind, LinkStartupMode,
        LinkSubsystem, LinkTarget, LinkTimeOptimizationPolicy, LinkedArtifactKind,
        LinkedProductKind, Linker, LinkerDriver, LinkerDriverCapabilities, LinkerDriverIdentity,
        LinkerDriverKind, LinkerOperationalCapabilities, LinkerTargetCapabilities,
        SectionGarbageCollectionPolicy, StagingPathKey,
    };
    use bray_runtime_interface::{
        BinarySymbolName, RootExecution, RuntimeAbiRole, RuntimeArtifact,
        RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactId,
        RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeArtifactSelection,
        RuntimeCapability,
    };
    use bray_target::{CodeModel, RelocationModel};

    use super::{
        LinkOutputStaging, LinkPlanConstructionError, ProductLinkInputs, StagedArtifact,
        construct_link_plan, link_policy,
    };
    use crate::test_support::{
        backend_capabilities, backend_identity, codegen_unit_key, interface_artifact,
        product_identity, target_identity, target_output_description,
    };
    use crate::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend, EmissionPlan,
        EmissionPlanner, EmissionRequest, ProductKind, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };

    #[test]
    fn link_plans_preserve_canonical_input_order_and_exclude_package_interfaces() {
        let plan = library_plan();
        let staged = staged_artifacts(&plan);
        let outputs = output_staging(&plan);

        let inputs = product_link_inputs()
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
            .with_exported_symbols([binary_symbol("zeta"), binary_symbol("alpha")])
            .with_search_paths([
                search_path(LinkSearchPathKind::Library, "dependencies"),
                search_path(LinkSearchPathKind::Framework, "frameworks"),
            ]);

        let link_plan = construct_link_plan(
            &plan,
            staged.iter().cloned().rev(),
            outputs.iter().cloned().rev(),
            &inputs,
            &linker_for(LinkStartupMode::ExplicitInputs),
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

        assert!(link_plan.inputs().iter().all(|input| {
            !matches!(
                input.source(),
                LinkInputSource::File(path)
                    if path.extension().is_some_and(|extension| extension == "brayi")
            )
        }));
    }

    #[test]
    fn async_executable_plans_preserve_runtime_and_entry_contracts() {
        let (plan, runtime) = async_executable_plan();

        let host = plan
            .request()
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must have a host contract"));

        let inputs = product_link_inputs()
            .with_runtime(runtime.clone())
            .with_entry_point(host.native_entry().clone())
            .with_native_inputs([
                native_library("pthread"),
                file_input(
                    LinkInputKind::Archive,
                    "dependencies/standard-library.a",
                    LinkInputProvenance::HostConfiguration,
                ),
                file_input(
                    LinkInputKind::Archive,
                    "dependencies/platform-provider.a",
                    LinkInputProvenance::PlatformProvider(product_identity().package().clone()),
                ),
            ]);

        let link_plan = construct_link_plan(
            &plan,
            staged_artifacts(&plan),
            output_staging(&plan),
            &inputs,
            &linker_for(LinkStartupMode::PlatformCompilerDriver),
        )
        .unwrap_or_else(|error| panic!("async link plan must construct: {error:?}"));

        assert_eq!(link_plan.entry_point(), Some(host.native_entry()));

        assert_eq!(
            link_plan
                .inputs()
                .iter()
                .map(|input| input.kind())
                .collect::<Vec<_>>(),
            [
                LinkInputKind::RelocatableObject,
                LinkInputKind::RelocatableObject,
                LinkInputKind::RuntimeComponent,
                LinkInputKind::Archive,
                LinkInputKind::Archive,
                LinkInputKind::NativeLibrary,
            ]
        );

        let runtime_input = link_plan
            .inputs()
            .iter()
            .position(|input| matches!(input.provenance(), LinkInputProvenance::Runtime(_)))
            .unwrap_or_else(|| panic!("runtime input must be present"));

        let standard_library = link_plan
            .inputs()
            .iter()
            .position(|input| {
                matches!(input.provenance(), LinkInputProvenance::HostConfiguration)
                    && input.kind() == LinkInputKind::Archive
            })
            .unwrap_or_else(|| panic!("standard-library input must be present"));

        assert!(runtime_input < standard_library);

        assert!(matches!(
            link_plan.inputs()[4].provenance(),
            LinkInputProvenance::PlatformProvider(_)
        ));

        let runtime_inputs = link_plan
            .inputs()
            .iter()
            .filter(|input| matches!(input.provenance(), LinkInputProvenance::Runtime(_)))
            .collect::<Vec<_>>();

        let [runtime_input] = runtime_inputs.as_slice() else {
            panic!("async link plan must contain exactly one runtime input");
        };

        assert_eq!(
            runtime_input.provenance(),
            &LinkInputProvenance::Runtime(runtime.contract().artifact().clone())
        );

        assert_eq!(host.abi_version(), runtime.contract().abi_version());

        assert!(matches!(
            host.entries()[0].root(),
            RootExecution::Asynchronous { .. }
        ));

        assert_eq!(
            host.runtime_capabilities(),
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
            assert!(host.role_binding(role).is_some());
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
                &product_link_inputs(),
                &linker_for(LinkStartupMode::PlatformCompilerDriver),
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
                &product_link_inputs(),
                &linker_for(LinkStartupMode::PlatformCompilerDriver),
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

        let foreign = StagedArtifact::try_new(interface.id().clone(), "stage/application.brayi")
            .unwrap_or_else(|error| panic!("test staged artifact must be valid: {error:?}"));

        assert_eq!(
            construct_link_plan(
                &plan,
                staged_artifacts(&plan).into_iter().chain([foreign]),
                output_staging(&plan),
                &product_link_inputs(),
                &linker_for(LinkStartupMode::PlatformCompilerDriver),
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
        let inputs = product_link_inputs();
        let invocations = Arc::new(AtomicUsize::new(0));

        let driver = Arc::new(CountingDriver {
            capabilities: counting_capabilities(
                linker_driver_identity(),
                LinkStartupMode::PlatformCompilerDriver,
            ),
            invocations: Arc::clone(&invocations),
        });

        let linker = Linker::try_new([Arc::clone(&driver) as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test linker must construct: {error:?}"));

        let first = construct_link_plan(
            &plan,
            staged.iter().cloned(),
            outputs.iter().cloned(),
            &inputs,
            &linker,
        );

        let second = construct_link_plan(
            &plan,
            staged.into_iter().rev(),
            outputs.into_iter().rev(),
            &inputs,
            &linker,
        );

        assert_eq!(first, second);
        assert_eq!(invocations.load(Ordering::SeqCst), 0);

        let link_plan =
            first.unwrap_or_else(|error| panic!("test link plan must construct: {error:?}"));

        assert_eq!(driver.capabilities.validate(&link_plan), Ok(()));

        let _ = linker.link(&link_plan, &|| false);

        assert_eq!(invocations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn link_policy_adjustment_preserves_cross_artifact_optimization() {
        let optimization = LinkTimeOptimizationPolicy::ThinLto {
            jobs: NonZeroUsize::MIN,
        };

        let policy = product_link_inputs().policy.with_optimization(optimization);

        assert_eq!(
            link_policy(policy, &BTreeMap::new()).optimization(),
            optimization
        );
    }

    #[test]
    fn link_plan_inputs_are_immutable_worker_safe_values() {
        assert_send_sync::<ProductLinkInputs>();
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
                RequestedArtifact::new(ArtifactKind::SharedLibrary, ArtifactRequirement::Required),
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

    fn async_executable_plan() -> (EmissionPlan, RuntimeArtifactSelection) {
        let Some(runtime_id) = RuntimeArtifactId::try_new("runtime.test") else {
            panic!("test runtime artifact identity must be valid");
        };

        let host = bray_testing::test_async_executable_host_contract_for(
            product_identity(),
            target_identity(),
            runtime_id,
        );

        let (runtime, _runtime_directory) = runtime_artifact(&host);

        let runtime = runtime
            .select(RuntimeArtifactPurpose::Product, host.requirements())
            .unwrap_or_else(|error| panic!("test runtime must select: {error:?}"));

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

        EmissionPlanner::new(target_output_description(), Some(backend), interface)
    }

    fn staged_artifacts(plan: &EmissionPlan) -> Vec<StagedArtifact> {
        plan.staged_artifacts()
            .enumerate()
            .map(|(index, artifact)| {
                StagedArtifact::try_new(artifact.id().clone(), format!("stage/input-{index}.o"))
                    .unwrap_or_else(|error| panic!("test staged artifact must be valid: {error:?}"))
            })
            .collect()
    }

    fn output_staging(plan: &EmissionPlan) -> Vec<LinkOutputStaging> {
        plan.artifacts()
            .iter()
            .filter(|artifact| matches!(artifact.producer(), crate::ArtifactProducer::Linker(_)))
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
                .unwrap_or_else(|error| panic!("test output staging must be valid: {error:?}"))
            })
            .collect()
    }

    fn product_link_inputs() -> ProductLinkInputs {
        ProductLinkInputs::new(
            link_target(),
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
        LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "22")
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
        LinkInputSpec::try_native_library(name, LinkInputProvenance::HostConfiguration)
            .unwrap_or_else(|| panic!("test native library name must be valid"))
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
    ) -> (RuntimeArtifact, tempfile::TempDir) {
        let contract = host
            .runtime()
            .cloned()
            .unwrap_or_else(|| panic!("async test host must select a runtime"));

        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let bytes = b"!<arch>\n";

        for archive in ["bray_runtime_product.a", "bray_runtime_test.a"] {
            std::fs::write(directory.path().join(archive), bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));
        }

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_file(&directory.path().join("bray_runtime_product.a"))
                .unwrap_or_else(|error| panic!("test runtime archive must hash: {error}")),
        );

        let roles = contract
            .role_bindings()
            .iter()
            .map(bray_runtime_interface::RuntimeRoleBinding::role)
            .filter(|role| *role != bray_runtime_interface::RuntimeAbiRole::TestEntrySelection);

        let component_id = RuntimeArtifactId::try_new("runtime.product")
            .unwrap_or_else(|| panic!("test component identity must be valid"));

        let component = RuntimeArtifactComponentMetadata::try_new(
            component_id.clone(),
            RuntimeArtifactPurpose::Product,
            roles,
            contract.capabilities().iter().copied(),
            "bray_runtime_product.a",
            digest,
        )
        .unwrap_or_else(|error| panic!("test runtime component must be valid: {error:?}"));

        let test_component = RuntimeArtifactComponentMetadata::try_new(
            RuntimeArtifactId::try_new("runtime.test")
                .unwrap_or_else(|| panic!("test component identity must be valid")),
            RuntimeArtifactPurpose::TestRunner,
            contract
                .role_bindings()
                .iter()
                .map(bray_runtime_interface::RuntimeRoleBinding::role),
            contract.capabilities().iter().copied(),
            "bray_runtime_test.a",
            digest,
        )
        .unwrap_or_else(|error| panic!("test runtime component must be valid: {error:?}"));

        let metadata = RuntimeArtifactMetadata::try_new(contract, [component, test_component])
            .unwrap_or_else(|error| panic!("test runtime metadata must be valid: {error:?}"));

        let artifact = RuntimeArtifact::try_new(
            metadata,
            [
                (
                    component_id,
                    directory.path().join("bray_runtime_product.a"),
                ),
                (
                    RuntimeArtifactId::try_new("runtime.test")
                        .unwrap_or_else(|| panic!("test component identity must be valid")),
                    directory.path().join("bray_runtime_test.a"),
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("test runtime artifact must be valid: {error:?}"));

        (artifact, directory)
    }

    struct CountingDriver {
        capabilities: LinkerDriverCapabilities,
        invocations: Arc<AtomicUsize>,
    }

    fn counting_capabilities(
        identity: LinkerDriverIdentity,
        startup: LinkStartupMode,
    ) -> LinkerDriverCapabilities {
        let target = link_target();

        let target = LinkerTargetCapabilities::new(
            target.architecture(),
            target.object_format(),
            [
                LinkPlanCapability::Product(LinkedProductKind::SharedLibrary),
                LinkPlanCapability::Product(LinkedProductKind::Executable),
                LinkPlanCapability::Input(LinkInputKind::RelocatableObject),
                LinkPlanCapability::Input(LinkInputKind::StartupObject),
                LinkPlanCapability::Input(LinkInputKind::TerminationObject),
                LinkPlanCapability::Input(LinkInputKind::Archive),
                LinkPlanCapability::Input(LinkInputKind::NativeLibrary),
                LinkPlanCapability::Input(LinkInputKind::RuntimeComponent),
                LinkPlanCapability::InputMode(LinkInputMode::Ordinary),
                LinkPlanCapability::Output(LinkedArtifactKind::SharedLibrary),
                LinkPlanCapability::Output(LinkedArtifactKind::Executable),
                LinkPlanCapability::Output(LinkedArtifactKind::PlatformCompanion),
                LinkPlanCapability::SearchPath(LinkSearchPathKind::Library),
                LinkPlanCapability::SearchPath(LinkSearchPathKind::Framework),
                LinkPlanCapability::LinkModel(LinkModel::Dynamic),
                LinkPlanCapability::DeadStrip(DeadStripPolicy::RemoveUnreachable),
                LinkPlanCapability::SectionGarbageCollection(
                    SectionGarbageCollectionPolicy::RemoveUnreferenced,
                ),
                LinkPlanCapability::Debug(DebugLinkPolicy::None),
                LinkPlanCapability::Subsystem(LinkSubsystem::Console),
                LinkPlanCapability::Symbol(bray_linker::LinkSymbolRequirement::EntryPoint),
                LinkPlanCapability::Symbol(bray_linker::LinkSymbolRequirement::ExportedSymbols),
                LinkPlanCapability::Symbol(bray_linker::LinkSymbolRequirement::RetainedSymbols),
                LinkPlanCapability::Startup(startup),
                LinkPlanCapability::Runtime(bray_linker::LinkRuntimeMode::ExplicitInput),
            ],
        );

        LinkerDriverCapabilities::try_new(
            identity,
            [target],
            LinkerOperationalCapabilities::new(
                LinkResponseFileCapability::InlineArguments,
                LinkEnvironmentCapability::NotApplicable,
                LinkCancellationCapability::Cooperative,
                LinkDeterminismCapability::Reproducible,
            ),
        )
        .unwrap_or_else(|error| panic!("test capabilities must construct: {error:?}"))
    }

    fn linker_for(startup: LinkStartupMode) -> Linker {
        let driver = Arc::new(CountingDriver {
            capabilities: counting_capabilities(linker_driver_identity(), startup),
            invocations: Arc::new(AtomicUsize::new(0)),
        });

        Linker::try_new([driver as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test linker must construct: {error:?}"))
    }

    impl LinkerDriver for CountingDriver {
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            &self.capabilities
        }

        fn link(
            &self,
            plan: &bray_linker::LinkPlan,
            _cancellation: &dyn Cancellation,
        ) -> LinkOutcome {
            self.invocations.fetch_add(1, Ordering::SeqCst);

            LinkOutcome::failed(
                plan,
                LinkFailure::Invocation,
                bray_diagnostics::DiagnosticBag::new(),
            )
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
