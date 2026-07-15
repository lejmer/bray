use bray_codegen::{
    AssemblySyntaxKind, BackendArtifactKind, DebugInformationMode, DebugInformationOutputMode,
};
use bray_symbols::ProductKind;

use super::{EmissionPlanner, EmissionPlanningError};
use crate::plan::{EmissionBackend, PackageInterfacePolicy};
use crate::{ArtifactKind, ArtifactRequirement, EmissionRequest, RequestedArtifactDestination};

pub(super) fn validate_request(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    if request.target() != planner.target().identity() {
        // Planning errors retain both Arc-backed target identities after validation returns.
        return Err(EmissionPlanningError::TargetMismatch {
            requested: request.target().clone(),
            selected: planner.target().identity().clone(),
        });
    }

    validate_product_artifacts(planner, request)?;
    validate_destination_shape(planner, request)?;

    if request_needs_backend(request) {
        validate_backend(planner, request)?;
    }

    Ok(())
}

fn validate_product_artifacts(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    for artifact in request.artifacts() {
        if !artifact_matches_product(artifact.kind(), request.product_kind()) {
            return Err(EmissionPlanningError::ProductArtifactMismatch {
                product: request.product_kind(),
                artifact: artifact.kind(),
            });
        }

        if artifact.kind() == ArtifactKind::PackageInterface
            && planner.package_interface_policy() != PackageInterfacePolicy::LibraryProducts
        {
            return Err(EmissionPlanningError::PackageInterfaceDisabled);
        }
    }

    if request.artifact(ArtifactKind::LinkedCompanion).is_some() && !has_linked_product(request) {
        return Err(EmissionPlanningError::MissingLinkedProduct);
    }

    Ok(())
}

fn validate_destination_shape(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    let published_count =
        request
            .artifacts()
            .iter()
            .try_fold(0_usize, |count, artifact| {
                let artifact_count = if artifact.kind().backend_kind().is_some() {
                    planner.backend().map_or(0, |backend| backend.units().len())
                } else {
                    1
                };

                count.checked_add(artifact_count).ok_or(
                    EmissionPlanningError::ArtifactOrdinalOverflow(artifact.kind()),
                )
            })?;

    if published_count > 1
        && matches!(
            request.destination(),
            RequestedArtifactDestination::FilesystemFile(_)
                | RequestedArtifactDestination::Stream(_)
        )
    {
        return Err(EmissionPlanningError::MultipleArtifactsForSingleSink);
    }

    Ok(())
}

fn validate_backend(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    let Some(backend) = planner.backend() else {
        return Err(EmissionPlanningError::MissingBackend);
    };

    if backend.units().is_empty() {
        return Err(EmissionPlanningError::MissingCodegenUnits);
    }

    if !backend
        .capabilities()
        .supports_target_machine(planner.target().machine())
    {
        // Planning errors retain the Arc-backed target identity after validation returns.
        return Err(EmissionPlanningError::UnsupportedBackendTarget(
            planner.target().identity().clone(),
        ));
    }

    for artifact in request.artifacts() {
        if let Some(kind) = artifact.kind().backend_kind()
            && !backend.capabilities().supports_artifact(kind)
        {
            return Err(EmissionPlanningError::UnsupportedBackendArtifact(kind));
        }
    }

    if has_linked_product(request) {
        validate_linkable_artifact(backend)?;
    }

    validate_debug_policy(backend, request)?;
    validate_serialization_policy(backend, request)
}

fn validate_linkable_artifact(backend: &EmissionBackend) -> Result<(), EmissionPlanningError> {
    let Some(kind) = backend.policy().linkable_artifact().artifact_kind() else {
        return Err(EmissionPlanningError::MissingLinkableArtifact);
    };

    if backend.capabilities().supports_artifact(kind) {
        return Ok(());
    }

    Err(EmissionPlanningError::UnsupportedBackendArtifact(kind))
}

fn validate_debug_policy(
    backend: &EmissionBackend,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    let policy = backend.policy();
    let information = policy.debug_information();
    let output = policy.debug_output();
    let companion = request.artifact(ArtifactKind::DebugCompanion);

    if !backend
        .capabilities()
        .supports_debug_information(information)
    {
        return Err(EmissionPlanningError::UnsupportedDebugInformation(
            information,
        ));
    }

    let output_is_valid = matches!(
        (information, output),
        (DebugInformationMode::None, DebugInformationOutputMode::Omit)
            | (
                DebugInformationMode::LineTables | DebugInformationMode::Full,
                DebugInformationOutputMode::Embedded | DebugInformationOutputMode::Separate
            )
    );

    if !output_is_valid {
        return Err(EmissionPlanningError::InvalidDebugOutput {
            information,
            output,
        });
    }

    match (output, companion) {
        (DebugInformationOutputMode::Separate, Some(companion))
            if companion.requirement() == ArtifactRequirement::Required =>
        {
            Ok(())
        }
        (DebugInformationOutputMode::Separate, _) => {
            Err(EmissionPlanningError::MissingRequiredDebugCompanion)
        }
        (DebugInformationOutputMode::Embedded | DebugInformationOutputMode::Omit, Some(_)) => {
            Err(EmissionPlanningError::UnexpectedDebugCompanion)
        }
        (DebugInformationOutputMode::Embedded | DebugInformationOutputMode::Omit, None) => Ok(()),
    }
}

fn validate_serialization_policy(
    backend: &EmissionBackend,
    request: &EmissionRequest,
) -> Result<(), EmissionPlanningError> {
    let serialization = backend.policy().serialization();
    let has_assembly = request.artifact(ArtifactKind::Assembly).is_some();

    if serialization.assembly_syntax_kind() != AssemblySyntaxKind::TargetDefault && !has_assembly {
        return Err(EmissionPlanningError::MissingSerializationArtifact(
            BackendArtifactKind::Assembly,
        ));
    }

    if has_assembly
        && !backend
            .capabilities()
            .supports_assembly_syntax_kind(serialization.assembly_syntax_kind())
    {
        return Err(EmissionPlanningError::UnsupportedAssemblySyntax(
            serialization.assembly_syntax_kind(),
        ));
    }

    if serialization.annotate_backend_ir() && request.artifact(ArtifactKind::BackendIr).is_none() {
        return Err(EmissionPlanningError::MissingSerializationArtifact(
            BackendArtifactKind::BackendIr,
        ));
    }

    Ok(())
}

const fn artifact_matches_product(kind: ArtifactKind, product: ProductKind) -> bool {
    match kind {
        ArtifactKind::Executable | ArtifactKind::ExecutableModule => {
            matches!(product, ProductKind::Executable | ProductKind::Test)
        }
        ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary
        | ArtifactKind::PackageInterface => matches!(product, ProductKind::Library),
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::DebugCompanion
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::LinkedCompanion => true,
    }
}

fn request_needs_backend(request: &EmissionRequest) -> bool {
    has_linked_product(request)
        || request
            .artifacts()
            .iter()
            .any(|artifact| artifact.kind().backend_kind().is_some())
}

pub(super) fn has_linked_product(request: &EmissionRequest) -> bool {
    [
        ArtifactKind::Executable,
        ArtifactKind::StaticLibrary,
        ArtifactKind::SharedLibrary,
    ]
    .into_iter()
    .any(|kind| request.artifact(kind).is_some())
}
