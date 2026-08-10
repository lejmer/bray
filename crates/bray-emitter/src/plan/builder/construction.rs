use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use bray_codegen::{
    BackendArtifactId, BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestEntry,
    BackendArtifactRequirement, LinkableArtifactRequirement,
};
use bray_package_interface::InterfaceArtifact;

use super::super::{
    EmissionPlan, EmissionPlanBuildError, PlannedArtifact, PlannedArtifactDestination,
};
use super::validation::{linked_product, should_plan_artifact};
use super::{EmissionPlanner, EmissionPlanningError};
use crate::sink::is_valid_host_file_name;
use crate::{
    ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole,
    DependencyMetadataProducerId, EmissionRequest, LinkerProducerId, OutputSink, RequestedArtifact,
    RequestedArtifactDestination,
};

pub(super) struct PlanBuilder<'planner> {
    planner: &'planner EmissionPlanner,
    request: EmissionRequest,
    package_interface: Option<InterfaceArtifact>,
    artifacts: Vec<PlannedArtifact>,
    backend_entries:
        BTreeMap<bray_codegen::CodegenUnitKey, BTreeMap<BackendArtifactKind, ArtifactRequirement>>,
    next_artifact_ordinals: BTreeMap<ArtifactKind, u32>,
}

impl<'planner> PlanBuilder<'planner> {
    pub(super) fn new(
        planner: &'planner EmissionPlanner,
        request: EmissionRequest,
        package_interface: Option<InterfaceArtifact>,
    ) -> Self {
        Self {
            planner,
            request,
            package_interface,
            artifacts: Vec::new(),
            backend_entries: BTreeMap::new(),
            next_artifact_ordinals: BTreeMap::new(),
        }
    }

    pub(super) fn build(mut self) -> Result<EmissionPlan, EmissionPlanningError> {
        let requested = self.request.artifacts().to_vec();
        let package_interface_available = self.package_interface.is_some();

        for artifact in requested {
            if should_plan_artifact(
                self.planner,
                &self.request,
                artifact,
                package_interface_available,
            ) {
                self.add_requested_artifact(artifact)?;
            }
        }

        if self.planned_linked_product().is_some() {
            self.add_link_inputs()?;
        }

        let backend_requests = self.backend_requests()?;

        let (backend, capability_revision) = if backend_requests.is_empty() {
            (None, None)
        } else {
            let selected = self.planner.backend();

            (
                // The completed plan retains the selected Arc-backed backend identity.
                selected.map(|backend| backend.identity().clone()),
                selected.map(|backend| backend.capabilities().revision()),
            )
        };

        EmissionPlan::try_new(
            self.request,
            backend,
            capability_revision,
            self.artifacts,
            backend_requests,
            self.package_interface,
        )
        .map_err(map_plan_error)
    }

    fn planned_linked_product(&self) -> Option<RequestedArtifact> {
        linked_product(&self.request)
            .filter(|&artifact| should_plan_artifact(self.planner, &self.request, artifact, false))
    }

    fn add_requested_artifact(
        &mut self,
        requested: RequestedArtifact,
    ) -> Result<(), EmissionPlanningError> {
        let Some(backend_kind) = requested.kind().backend_kind() else {
            return self.add_compiler_artifact(requested);
        };

        let Some(backend) = self.planner.backend() else {
            return Err(EmissionPlanningError::MissingBackend);
        };

        let include_unit_ordinal = backend.units().len() > 1;

        for (index, unit) in backend.units().iter().enumerate() {
            let unit_ordinal = artifact_ordinal(index, requested.kind())?;

            // The contribution identity must own its structural unit key independently of policy.
            let backend_id = BackendArtifactId::new(unit.clone(), backend_kind, 0);
            let id = self.next_artifact_id(requested.kind())?;

            let destination =
                self.published_destination(&id, include_unit_ordinal.then_some(unit_ordinal))?;

            self.record_backend_entry(unit, backend_kind, requested.requirement());

            self.artifacts.push(PlannedArtifact::new(
                id,
                requested.requirement(),
                published_backend_role(requested.kind()),
                ArtifactProducer::Backend {
                    artifact: backend_id,
                    // Each planned producer retains the selected Arc-backed backend identity.
                    backend: backend.identity().clone(),
                },
                destination,
            ));
        }

        Ok(())
    }

    fn add_compiler_artifact(
        &mut self,
        requested: RequestedArtifact,
    ) -> Result<(), EmissionPlanningError> {
        let id = self.next_artifact_id(requested.kind())?;
        let destination = self.published_destination(&id, None)?;
        let role = compiler_artifact_role(requested.kind());
        let producer = self.compiler_artifact_producer(requested.kind());

        self.artifacts.push(PlannedArtifact::new(
            id,
            requested.requirement(),
            role,
            producer,
            destination,
        ));

        Ok(())
    }

    fn add_link_inputs(&mut self) -> Result<(), EmissionPlanningError> {
        let Some(backend) = self.planner.backend() else {
            return Err(EmissionPlanningError::MissingBackend);
        };

        let Some(linkable_kind) = backend.policy().linkable_artifact() else {
            return Err(EmissionPlanningError::MissingLinkableArtifact);
        };

        let backend_kind = linkable_kind.artifact_kind();
        let artifact_kind = ArtifactKind::from(backend_kind);

        let Some(linked_product) = linked_product(&self.request) else {
            return Err(EmissionPlanningError::MissingLinkedProduct);
        };

        let requirement = linked_product.requirement();

        for unit in backend.units() {
            let id = self.next_artifact_id(artifact_kind)?;

            // The contribution identity must own its structural unit key independently of policy.
            let backend_id = BackendArtifactId::new(unit.clone(), backend_kind, 0);

            self.record_backend_entry(unit, backend_kind, requirement);

            self.artifacts.push(PlannedArtifact::new(
                id,
                requirement,
                ArtifactRole::LinkInput,
                ArtifactProducer::Backend {
                    artifact: backend_id,
                    // Each planned producer retains the selected Arc-backed backend identity.
                    backend: backend.identity().clone(),
                },
                PlannedArtifactDestination::Stage,
            ));
        }

        Ok(())
    }

    fn compiler_artifact_producer(&mut self, kind: ArtifactKind) -> ArtifactProducer {
        match kind {
            ArtifactKind::PackageInterface => ArtifactProducer::PackageInterface,
            ArtifactKind::PackageImplementation => ArtifactProducer::PackageImplementation,
            ArtifactKind::DependencyMetadata => {
                ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0))
            }
            ArtifactKind::Executable
            | ArtifactKind::StaticLibrary
            | ArtifactKind::SharedLibrary
            | ArtifactKind::LinkedCompanion => ArtifactProducer::Linker(LinkerProducerId::new(0)),
            ArtifactKind::Assembly
            | ArtifactKind::BackendIr
            | ArtifactKind::BackendBitcode
            | ArtifactKind::RelocatableObject
            | ArtifactKind::ExecutableModule
            | ArtifactKind::DebugCompanion => {
                unreachable!("backend artifacts are handled before compiler artifacts")
            }
        }
    }

    fn published_destination(
        &self,
        id: &ArtifactId,
        unit_ordinal: Option<u32>,
    ) -> Result<PlannedArtifactDestination, EmissionPlanningError> {
        let sink = match self.request.destination() {
            RequestedArtifactDestination::FilesystemDirectory(directory) => {
                let stem = output_stem(self.request.product().name(), unit_ordinal);
                let name = self.output_name(id.kind(), &stem)?;

                OutputSink::Filesystem(directory.join(name))
            }
            RequestedArtifactDestination::FilesystemFile(path) => {
                self.validate_explicit_file_name(id.kind(), path)?;

                // The plan owns the final host-supplied path independently of the request.
                OutputSink::Filesystem(path.clone())
            }
            RequestedArtifactDestination::Memory(collector) => OutputSink::Memory {
                // The plan owns the host collector identity independently of the request.
                collector: collector.clone(),
                // Memory publication uses the complete planned identity as its deterministic key.
                artifact: id.clone(),
            },
            RequestedArtifactDestination::Stream(stream) => {
                // The plan owns the host stream identity independently of the request.
                OutputSink::Stream(stream.clone())
            }
        };

        Ok(PlannedArtifactDestination::Publish(sink))
    }

    fn output_name(&self, kind: ArtifactKind, stem: &str) -> Result<String, EmissionPlanningError> {
        let Some(name) = self.planner.target().name(kind.target_output_kind()) else {
            return Err(EmissionPlanningError::MissingOutputName(kind));
        };

        let Some(file_name) = name.file_name(stem) else {
            return Err(EmissionPlanningError::InvalidProductName);
        };

        if !is_valid_host_file_name(OsStr::new(&file_name)) {
            return Err(EmissionPlanningError::InvalidGeneratedFileName(kind));
        }

        Ok(file_name)
    }

    fn validate_explicit_file_name(
        &self,
        kind: ArtifactKind,
        path: &Path,
    ) -> Result<(), EmissionPlanningError> {
        let Some(file_name) = path.file_name() else {
            return Err(EmissionPlanningError::MissingExplicitFileName);
        };

        if !is_valid_host_file_name(file_name) {
            return Err(EmissionPlanningError::InvalidExplicitFileName);
        }

        let Some(name) = self.planner.target().name(kind.target_output_kind()) else {
            return Err(EmissionPlanningError::MissingOutputName(kind));
        };

        if name.suffix().is_empty() {
            return Ok(());
        }

        if file_name
            .to_str()
            .is_some_and(|file_name| file_name.ends_with(name.suffix()))
        {
            return Ok(());
        }

        Err(EmissionPlanningError::ExplicitOutputSuffixMismatch(kind))
    }

    fn next_artifact_id(
        &mut self,
        kind: ArtifactKind,
    ) -> Result<ArtifactId, EmissionPlanningError> {
        let next = self.next_artifact_ordinals.entry(kind).or_default();
        let ordinal = *next;

        let Some(following) = next.checked_add(1) else {
            return Err(EmissionPlanningError::ArtifactOrdinalOverflow(kind));
        };

        *next = following;

        // Every artifact identity independently retains the Arc-backed selected product.
        Ok(ArtifactId::new(
            self.request.product().clone(),
            kind,
            ordinal,
        ))
    }

    fn record_backend_entry(
        &mut self,
        unit: &bray_codegen::CodegenUnitKey,
        kind: BackendArtifactKind,
        requirement: ArtifactRequirement,
    ) {
        // The request-entry index must own unit keys after the selected backend borrow ends.
        self.backend_entries
            .entry(unit.clone())
            .or_default()
            .entry(kind)
            .and_modify(|current| {
                if requirement == ArtifactRequirement::Required {
                    *current = ArtifactRequirement::Required;
                }
            })
            .or_insert(requirement);
    }

    fn backend_requests(&self) -> Result<Vec<BackendArtifactRequest>, EmissionPlanningError> {
        let Some(backend) = self.planner.backend() else {
            return Ok(Vec::new());
        };

        let linkable_artifact = self.planned_linked_product().and_then(|artifact| {
            backend.policy().linkable_artifact().map(|kind| {
                LinkableArtifactRequirement::new(kind, backend_requirement(artifact.requirement()))
            })
        });

        self.backend_entries
            .iter()
            .map(|(unit, entries)| {
                let entries = entries.iter().map(|(&kind, &requirement)| {
                    // Every entry owns the structural unit key carried by its contribution ID.
                    BackendArtifactRequestEntry::new(
                        BackendArtifactId::new(unit.clone(), kind, 0),
                        backend_requirement(requirement),
                    )
                });

                // The request and its entries independently retain the same structural unit key.
                BackendArtifactRequest::try_new(
                    unit.clone(),
                    entries,
                    backend.policy().debug_output(),
                    linkable_artifact,
                    backend.policy().serialization(),
                )
                .map_err(EmissionPlanningError::InvalidBackendRequest)
            })
            .collect()
    }
}

fn published_backend_role(kind: ArtifactKind) -> ArtifactRole {
    match kind {
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject => ArtifactRole::Inspection,
        ArtifactKind::ExecutableModule => ArtifactRole::Product,
        ArtifactKind::DebugCompanion => ArtifactRole::Companion,
        ArtifactKind::PackageInterface
        | ArtifactKind::PackageImplementation
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::Executable
        | ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary
        | ArtifactKind::LinkedCompanion => {
            unreachable!("non-backend artifact passed to backend role selection")
        }
    }
}

fn compiler_artifact_role(kind: ArtifactKind) -> ArtifactRole {
    match kind {
        ArtifactKind::PackageInterface
        | ArtifactKind::Executable
        | ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary => ArtifactRole::Product,
        ArtifactKind::PackageImplementation
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::LinkedCompanion => ArtifactRole::Companion,
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::ExecutableModule
        | ArtifactKind::DebugCompanion => {
            unreachable!("backend artifact passed to compiler role selection")
        }
    }
}

const fn backend_requirement(requirement: ArtifactRequirement) -> BackendArtifactRequirement {
    match requirement {
        ArtifactRequirement::Required => BackendArtifactRequirement::Required,
        ArtifactRequirement::Optional => BackendArtifactRequirement::Optional,
    }
}

fn artifact_ordinal(index: usize, kind: ArtifactKind) -> Result<u32, EmissionPlanningError> {
    u32::try_from(index).map_err(|_| EmissionPlanningError::ArtifactOrdinalOverflow(kind))
}

fn output_stem(name: &str, unit_ordinal: Option<u32>) -> String {
    match unit_ordinal {
        Some(ordinal) => format!("{name}.{ordinal}"),
        None => String::from(name),
    }
}

fn map_plan_error(error: EmissionPlanBuildError) -> EmissionPlanningError {
    match error {
        EmissionPlanBuildError::DuplicateSink(sink) => EmissionPlanningError::OutputCollision(sink),
        EmissionPlanBuildError::Empty
        | EmissionPlanBuildError::BackendCapabilityIdentityMismatch
        | EmissionPlanBuildError::ForeignProduct(_)
        | EmissionPlanBuildError::DuplicateArtifact(_)
        | EmissionPlanBuildError::MemoryArtifactIdentityMismatch(_)
        | EmissionPlanBuildError::MissingRequestedArtifact(_)
        | EmissionPlanBuildError::UnrequestedPublishedArtifact(_)
        | EmissionPlanBuildError::RequirementMismatch(_)
        | EmissionPlanBuildError::RoleDestinationMismatch(_)
        | EmissionPlanBuildError::KindRoleMismatch(_)
        | EmissionPlanBuildError::ProducerKindMismatch(_)
        | EmissionPlanBuildError::BackendIdentityMismatch(_)
        | EmissionPlanBuildError::MissingBackend(_)
        | EmissionPlanBuildError::BackendArtifactKindMismatch(_)
        | EmissionPlanBuildError::DuplicateBackendRequest(_)
        | EmissionPlanBuildError::MissingBackendRequest(_)
        | EmissionPlanBuildError::UnmappedBackendRequest(_)
        | EmissionPlanBuildError::BackendRequirementMismatch(_)
        | EmissionPlanBuildError::MissingPackageInterfaceArtifact
        | EmissionPlanBuildError::UnexpectedPackageInterfaceArtifact
        | EmissionPlanBuildError::PackageInterfaceProductMismatch => {
            EmissionPlanningError::InconsistentPlan
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_codegen::{
        AssemblySyntaxKind, BackendArtifactKind, BackendArtifactRequirement, BackendCapabilities,
        BackendOutputCapabilities, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind, LinkableArtifactRequirement,
    };
    use bray_target::TargetOutputKind;

    use super::super::{EmissionPlanner, EmissionPlanningError};
    use crate::test_support::{
        backend_capabilities, backend_identity, codegen_unit_key, emission_request_for,
        interface_artifact, output_name, target_output_description, target_output_description_from,
    };
    use crate::{
        ArtifactKind, ArtifactRequirement, BackendEmissionPolicy, EmissionBackend, OutputSink,
        PlannedArtifactDestination, ProductKind, RequestedArtifact, RequestedArtifactDestination,
    };

    #[test]
    fn planning_is_deterministic_and_derives_exact_per_unit_backend_requests() {
        let first_unit = codegen_unit_key(1);
        let second_unit = codegen_unit_key(2);
        let policy = backend_policy(Some(LinkableArtifactKind::RelocatableObject));

        let first_planner = planner(
            backend_capabilities(),
            [second_unit.clone(), first_unit.clone()],
            policy,
        );

        let second_planner = planner(
            backend_capabilities(),
            [first_unit.clone(), second_unit.clone()],
            policy,
        );

        let request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::Executable, ArtifactRequirement::Required),
                RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Optional),
            ],
        );

        let Ok(first_plan) = first_planner.plan(request.clone()) else {
            panic!("test emission plan must be valid");
        };

        let Ok(second_plan) = second_planner.plan(request) else {
            panic!("test emission plan must be valid");
        };

        assert_eq!(first_plan, second_plan);
        assert_eq!(first_plan.backend_requests().len(), 2);
        assert_eq!(first_plan.backend_requests()[0].unit(), &first_unit);
        assert_eq!(first_plan.backend_requests()[1].unit(), &second_unit);

        for request in first_plan.backend_requests() {
            assert_eq!(
                request
                    .entries()
                    .iter()
                    .map(|entry| (entry.id().kind(), entry.requirement()))
                    .collect::<Vec<_>>(),
                [
                    (
                        BackendArtifactKind::RelocatableObject,
                        BackendArtifactRequirement::Required,
                    ),
                    (
                        BackendArtifactKind::Assembly,
                        BackendArtifactRequirement::Optional,
                    ),
                ]
            );

            assert_eq!(
                request.linkable_artifact(),
                Some(LinkableArtifactRequirement::new(
                    LinkableArtifactKind::RelocatableObject,
                    BackendArtifactRequirement::Required,
                ))
            );
        }

        let published_paths = first_plan
            .published_artifacts()
            .map(|artifact| match artifact.destination() {
                PlannedArtifactDestination::Publish(OutputSink::Filesystem(path)) => path.as_path(),
                PlannedArtifactDestination::Publish(
                    OutputSink::Memory { .. } | OutputSink::Stream(_),
                )
                | PlannedArtifactDestination::Stage => {
                    panic!("test artifacts must publish to filesystem paths")
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(
            published_paths,
            [
                Path::new("out/application.0.s"),
                Path::new("out/application.1.s"),
                Path::new("out/application"),
            ]
        );

        assert_eq!(first_plan.staged_artifacts().count(), 2);
    }

    #[test]
    fn package_interfaces_plan_without_backend_participation() {
        let planner = EmissionPlanner::new(
            target_output_description(),
            None,
            Some(interface_artifact()),
        );

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
        );

        let Ok(plan) = planner.plan(request) else {
            panic!("test package-interface plan must be valid");
        };

        assert_eq!(plan.backend(), None);
        assert!(plan.backend_requests().is_empty());

        let Some(artifact) = plan.published_artifacts().next() else {
            panic!("test plan must publish a package interface");
        };

        assert_eq!(artifact.id().kind(), ArtifactKind::PackageInterface);

        assert_eq!(
            artifact.destination(),
            &PlannedArtifactDestination::Publish(OutputSink::Filesystem(
                "out/application.brayi".into()
            ))
        );
    }

    #[test]
    fn package_interface_planning_validates_exact_product_identity() {
        let foreign = bray_package_interface::test_support::interface_artifact();
        let actual = foreign.identity().clone();
        let planner = EmissionPlanner::new(target_output_description(), None, Some(foreign));

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
        );

        let expected = request.product().clone();

        assert_eq!(
            planner.plan(request),
            Err(EmissionPlanningError::PackageInterfaceProductMismatch { expected, actual })
        );
    }

    #[test]
    fn package_interface_planning_rejects_unrequested_completed_artifacts() {
        let planner = EmissionPlanner::new(
            target_output_description(),
            None,
            Some(interface_artifact()),
        );

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::DependencyMetadata,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            planner.plan(request),
            Err(EmissionPlanningError::UnexpectedPackageInterfaceArtifact)
        );
    }

    #[test]
    fn unavailable_optional_package_interfaces_are_omitted() {
        let planner = EmissionPlanner::new(target_output_description(), None, None);

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(
                    ArtifactKind::PackageInterface,
                    ArtifactRequirement::Optional,
                ),
                RequestedArtifact::new(
                    ArtifactKind::DependencyMetadata,
                    ArtifactRequirement::Required,
                ),
            ],
        );

        let Ok(plan) = planner.plan(request) else {
            panic!("optional unavailable package interface must not invalidate the plan");
        };

        assert!(plan.package_interface().is_none());

        assert!(
            plan.artifacts()
                .iter()
                .all(|artifact| artifact.id().kind() != ArtifactKind::PackageInterface)
        );
    }

    #[test]
    fn optional_backend_artifacts_are_omitted_when_unavailable() {
        let capabilities = capabilities_with_artifacts([BackendArtifactKind::RelocatableObject]);

        let planner = planner(
            capabilities,
            [codegen_unit_key(1)],
            backend_policy(Some(LinkableArtifactKind::RelocatableObject)),
        );

        let request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::Executable, ArtifactRequirement::Required),
                RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Optional),
            ],
        );

        let Ok(plan) = planner.plan(request) else {
            panic!("test emission plan must be valid");
        };

        assert!(
            plan.artifacts()
                .iter()
                .all(|artifact| artifact.id().kind() != ArtifactKind::Assembly)
        );

        assert_eq!(
            plan.backend_requests()[0].linkable_artifact(),
            Some(LinkableArtifactRequirement::new(
                LinkableArtifactKind::RelocatableObject,
                BackendArtifactRequirement::Required,
            ))
        );
    }

    #[test]
    fn optional_linked_products_preserve_optional_backend_inputs() {
        let planner = EmissionPlanner::new(
            target_output_description(),
            emission_backend(
                backend_capabilities(),
                [codegen_unit_key(1)],
                backend_policy(Some(LinkableArtifactKind::RelocatableObject)),
            ),
            Some(interface_artifact()),
        );

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(
                    ArtifactKind::PackageInterface,
                    ArtifactRequirement::Required,
                ),
                RequestedArtifact::new(ArtifactKind::StaticLibrary, ArtifactRequirement::Optional),
            ],
        );

        let Ok(plan) = planner.plan(request) else {
            panic!("test emission plan must be valid");
        };

        let Some(staged) = plan.staged_artifacts().next() else {
            panic!("test emission plan must stage a link input");
        };

        assert_eq!(staged.requirement(), ArtifactRequirement::Optional);
        assert_eq!(plan.backend_requests()[0].optional().count(), 1);

        assert_eq!(
            plan.backend_requests()[0].linkable_artifact(),
            Some(LinkableArtifactRequirement::new(
                LinkableArtifactKind::RelocatableObject,
                BackendArtifactRequirement::Optional,
            ))
        );
    }

    #[test]
    fn linked_companions_share_one_unambiguous_link_operation() {
        let planner = planner(
            backend_capabilities(),
            [codegen_unit_key(1)],
            backend_policy(Some(LinkableArtifactKind::RelocatableObject)),
        );

        let request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::Executable, ArtifactRequirement::Required),
                RequestedArtifact::new(
                    ArtifactKind::LinkedCompanion,
                    ArtifactRequirement::Required,
                ),
            ],
        );

        let Ok(plan) = planner.plan(request) else {
            panic!("test emission plan must be valid");
        };

        let product = plan
            .published_artifacts()
            .find(|artifact| artifact.id().kind() == ArtifactKind::Executable);

        let companion = plan
            .published_artifacts()
            .find(|artifact| artifact.id().kind() == ArtifactKind::LinkedCompanion);

        assert_eq!(
            product.map(|artifact| artifact.producer()),
            companion.map(|artifact| artifact.producer())
        );
    }

    #[test]
    fn planning_rejects_ambiguous_linked_products() {
        let planner = planner(
            backend_capabilities(),
            [codegen_unit_key(1)],
            backend_policy(Some(LinkableArtifactKind::RelocatableObject)),
        );

        let request = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::StaticLibrary, ArtifactRequirement::Required),
                RequestedArtifact::new(ArtifactKind::SharedLibrary, ArtifactRequirement::Optional),
            ],
        );

        assert_eq!(
            planner.plan(request),
            Err(EmissionPlanningError::MultipleLinkedProducts)
        );

        let requirement_mismatch = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::StaticLibrary, ArtifactRequirement::Optional),
                RequestedArtifact::new(
                    ArtifactKind::LinkedCompanion,
                    ArtifactRequirement::Required,
                ),
            ],
        );

        assert_eq!(
            planner.plan(requirement_mismatch),
            Err(EmissionPlanningError::LinkedCompanionRequirementMismatch)
        );
    }

    #[test]
    fn planning_rejects_backend_capability_and_single_sink_conflicts() {
        let unsupported = planner(
            BackendCapabilities::default(),
            [codegen_unit_key(1)],
            backend_policy(None),
        );

        let assembly_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            unsupported.plan(assembly_request),
            Err(EmissionPlanningError::UnsupportedBackendTarget(
                target_output_description().identity().clone()
            ))
        );

        let missing_assembly = capabilities_with_artifacts([]);

        let unsupported_artifact = planner(
            missing_assembly,
            [codegen_unit_key(1)],
            backend_policy(None),
        );

        let assembly_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            unsupported_artifact.plan(assembly_request),
            Err(EmissionPlanningError::UnsupportedBackendArtifact(
                BackendArtifactKind::Assembly
            ))
        );

        let multiple_units = planner(
            backend_capabilities(),
            [codegen_unit_key(1), codegen_unit_key(2)],
            backend_policy(None),
        );

        let single_file_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemFile("output.s".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            multiple_units.plan(single_file_request),
            Err(EmissionPlanningError::MultipleArtifactsForSingleSink)
        );

        let single_unit = planner(
            backend_capabilities(),
            [codegen_unit_key(1)],
            backend_policy(None),
        );

        let wrong_suffix = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemFile("output.o".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            single_unit.plan(wrong_suffix),
            Err(EmissionPlanningError::ExplicitOutputSuffixMismatch(
                ArtifactKind::Assembly
            ))
        );
    }

    #[test]
    fn planning_rejects_generated_name_collisions_and_ineligible_interfaces() {
        let duplicate_suffix_names = [
            output_name(TargetOutputKind::Assembly, "", ".out"),
            output_name(TargetOutputKind::BackendIr, "", ".out"),
        ];

        let collision_planner = EmissionPlanner::new(
            target_output_description_from(duplicate_suffix_names),
            emission_backend(
                backend_capabilities(),
                [codegen_unit_key(1)],
                backend_policy(None),
            ),
            None,
        );

        let collision_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Required),
                RequestedArtifact::new(ArtifactKind::BackendIr, ArtifactRequirement::Required),
            ],
        );

        assert_eq!(
            collision_planner.plan(collision_request),
            Err(EmissionPlanningError::OutputCollision(
                OutputSink::Filesystem("out/application.out".into())
            ))
        );

        let interface_planner = EmissionPlanner::new(
            target_output_description(),
            None,
            Some(interface_artifact()),
        );

        let executable_interface = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            interface_planner.plan(executable_interface),
            Err(EmissionPlanningError::ProductArtifactMismatch {
                product: ProductKind::Executable,
                artifact: ArtifactKind::PackageInterface,
            })
        );

        let missing_interface_planner =
            EmissionPlanner::new(target_output_description(), None, None);

        let library_interface = emission_request_for(
            ProductKind::Library,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            missing_interface_planner.plan(library_interface),
            Err(EmissionPlanningError::MissingPackageInterfaceArtifact)
        );
    }

    #[cfg(windows)]
    #[test]
    fn planning_rejects_host_invalid_and_equivalent_file_names() {
        let invalid_name_planner = EmissionPlanner::new(
            target_output_description_from([output_name(TargetOutputKind::Assembly, "", ":")]),
            emission_backend(
                backend_capabilities(),
                [codegen_unit_key(1)],
                backend_policy(None),
            ),
            None,
        );

        let assembly_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            invalid_name_planner.plan(assembly_request),
            Err(EmissionPlanningError::InvalidGeneratedFileName(
                ArtifactKind::Assembly
            ))
        );

        let collision_planner = EmissionPlanner::new(
            target_output_description_from([
                output_name(TargetOutputKind::Assembly, "", ".out"),
                output_name(TargetOutputKind::BackendIr, "", ".OUT"),
            ]),
            emission_backend(
                backend_capabilities(),
                [codegen_unit_key(1)],
                backend_policy(None),
            ),
            None,
        );

        let collision_request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [
                RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Required),
                RequestedArtifact::new(ArtifactKind::BackendIr, ArtifactRequirement::Required),
            ],
        );

        assert_eq!(
            collision_planner.plan(collision_request),
            Err(EmissionPlanningError::OutputCollision(
                OutputSink::Filesystem("out/application.OUT".into())
            ))
        );
    }

    #[test]
    fn planning_validates_debug_and_serialization_policy_before_backend_requests() {
        let serialization = BackendSerializationOptions::new(AssemblySyntaxKind::Intel);

        let invalid_policy = BackendEmissionPolicy::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Separate,
            None,
            serialization,
        );

        let planner = planner(
            backend_capabilities(),
            [codegen_unit_key(1)],
            invalid_policy,
        );

        let request = emission_request_for(
            ProductKind::Executable,
            RequestedArtifactDestination::FilesystemDirectory("out".into()),
            [RequestedArtifact::new(
                ArtifactKind::Assembly,
                ArtifactRequirement::Required,
            )],
        );

        assert_eq!(
            planner.plan(request),
            Err(EmissionPlanningError::InvalidDebugOutput {
                information: DebugInformationMode::None,
                output: DebugInformationOutputMode::Separate,
            })
        );
    }

    fn planner(
        capabilities: BackendCapabilities,
        units: impl IntoIterator<Item = bray_codegen::CodegenUnitKey>,
        policy: BackendEmissionPolicy,
    ) -> EmissionPlanner {
        EmissionPlanner::new(
            target_output_description(),
            emission_backend(capabilities, units, policy),
            None,
        )
    }

    fn emission_backend(
        capabilities: BackendCapabilities,
        units: impl IntoIterator<Item = bray_codegen::CodegenUnitKey>,
        policy: BackendEmissionPolicy,
    ) -> Option<EmissionBackend> {
        let Ok(backend) = EmissionBackend::try_new(backend_identity(), capabilities, units, policy)
        else {
            panic!("test emission backend must be valid");
        };

        Some(backend)
    }

    fn capabilities_with_artifacts(
        artifacts: impl IntoIterator<Item = BackendArtifactKind>,
    ) -> BackendCapabilities {
        let complete = backend_capabilities();

        BackendCapabilities::new(
            complete.revision(),
            complete.product_kinds().iter().copied(),
            complete.targets().clone(),
            complete.runtime().clone(),
            complete.optimization().clone(),
            BackendOutputCapabilities::new(
                artifacts,
                complete.outputs().debug_information().iter().copied(),
                complete.outputs().debug_output().iter().copied(),
                complete.outputs().assembly_syntax().iter().copied(),
            ),
            complete.reproducibility(),
        )
    }

    fn backend_policy(linkable: Option<LinkableArtifactKind>) -> BackendEmissionPolicy {
        BackendEmissionPolicy::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Omit,
            linkable,
            BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
        )
    }
}
