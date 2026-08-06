use bray_codegen::{
    AssemblySyntaxKind, BackendArtifactKind, DebugInformationMode, DebugInformationOutputMode,
};
use bray_package_interface::InterfaceArtifact;
use bray_symbols::ProductKind;

use super::{EmissionPlanner, EmissionPlanningError};
use crate::plan::EmissionBackend;
use crate::plan::model::package_interface_matches_product;
use crate::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, RequestedArtifact,
    RequestedArtifactDestination,
};

const LINKED_PRODUCT_KINDS: [ArtifactKind; 3] = [
    ArtifactKind::Executable,
    ArtifactKind::StaticLibrary,
    ArtifactKind::SharedLibrary,
];

pub(super) fn validate_request(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
    package_interface: Option<&InterfaceArtifact>,
) -> Result<(), EmissionPlanningError> {
    if request.target() != planner.target().identity() {
        // Planning errors retain both Arc-backed target identities after validation returns.
        return Err(EmissionPlanningError::TargetMismatch {
            requested: request.target().clone(),
            selected: planner.target().identity().clone(),
        });
    }

    validate_product_artifacts(request, package_interface)?;
    validate_destination_shape(planner, request, package_interface.is_some())?;

    if request_uses_backend(request) {
        validate_backend(planner, request)?;
    }

    Ok(())
}

fn validate_product_artifacts(
    request: &EmissionRequest,
    package_interface: Option<&InterfaceArtifact>,
) -> Result<(), EmissionPlanningError> {
    for artifact in request.artifacts() {
        if !artifact_matches_product(artifact.kind(), request.product_kind()) {
            return Err(EmissionPlanningError::ProductArtifactMismatch {
                product: request.product_kind(),
                artifact: artifact.kind(),
            });
        }
    }

    validate_package_interface(request, package_interface)?;

    let linked_product_count = linked_product_count(request);

    if linked_product_count > 1 {
        return Err(EmissionPlanningError::MultipleLinkedProducts);
    }

    let companion = request.artifact(ArtifactKind::LinkedCompanion);
    let product = linked_product(request);

    match (companion, product) {
        (Some(_), None) => Err(EmissionPlanningError::MissingLinkedProduct),
        (Some(companion), Some(product))
            if companion.requirement() == ArtifactRequirement::Required
                && product.requirement() == ArtifactRequirement::Optional =>
        {
            Err(EmissionPlanningError::LinkedCompanionRequirementMismatch)
        }
        _ => Ok(()),
    }
}

fn validate_package_interface(
    request: &EmissionRequest,
    package_interface: Option<&InterfaceArtifact>,
) -> Result<(), EmissionPlanningError> {
    let requested = request.artifact(ArtifactKind::PackageInterface);

    match (requested, package_interface) {
        (None, None) => Ok(()),
        (Some(requested), None) if requested.requirement() == ArtifactRequirement::Required => {
            Err(EmissionPlanningError::MissingPackageInterfaceArtifact)
        }
        (Some(_), None) => Ok(()),
        (None, Some(_)) => Err(EmissionPlanningError::UnexpectedPackageInterfaceArtifact),
        (Some(_), Some(package_interface))
            if !package_interface_matches_product(package_interface, request) =>
        {
            Err(EmissionPlanningError::PackageInterfaceProductMismatch {
                // Planning failures retain identities after both request borrows end.
                expected: request.product().clone(),
                actual: package_interface.identity().clone(),
            })
        }
        (Some(_), Some(_)) => Ok(()),
    }
}

fn validate_destination_shape(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
    package_interface_available: bool,
) -> Result<(), EmissionPlanningError> {
    let published_count =
        request.artifacts().iter().try_fold(
            0_usize,
            |count, artifact| -> Result<usize, EmissionPlanningError> {
                if !should_plan_artifact(planner, request, *artifact, package_interface_available) {
                    return Ok(count);
                }

                let artifact_count = if artifact.kind().backend_kind().is_some() {
                    planner.backend().map_or(0, |backend| backend.units().len())
                } else {
                    1
                };

                count.checked_add(artifact_count).ok_or(
                    EmissionPlanningError::ArtifactOrdinalOverflow(artifact.kind()),
                )
            },
        )?;

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
        return if request_requires_backend(request) {
            Err(EmissionPlanningError::MissingBackend)
        } else {
            Ok(())
        };
    };

    if backend.units().is_empty() {
        return if request_requires_codegen_units(request) {
            Err(EmissionPlanningError::MissingCodegenUnits)
        } else {
            Ok(())
        };
    }

    if !backend
        .capabilities()
        .supports_target_machine(planner.target().machine())
    {
        if request_requires_backend(request) {
            // Planning errors retain the Arc-backed target identity after validation returns.
            return Err(EmissionPlanningError::UnsupportedBackendTarget(
                planner.target().identity().clone(),
            ));
        }

        return Ok(());
    }

    for artifact in request.artifacts() {
        if artifact.requirement() == ArtifactRequirement::Required
            && let Some(kind) = artifact.kind().backend_kind()
            && !backend.capabilities().supports_artifact(kind)
        {
            return Err(EmissionPlanningError::UnsupportedBackendArtifact(kind));
        }
    }

    if linked_product(request)
        .is_some_and(|product| product.requirement() == ArtifactRequirement::Required)
    {
        validate_linkable_artifact(backend)?;
    }

    if has_planned_backend_work(planner, request) {
        validate_debug_policy(backend, request)?;
        validate_serialization_policy(backend, request)?;
    }

    Ok(())
}

fn validate_linkable_artifact(backend: &EmissionBackend) -> Result<(), EmissionPlanningError> {
    let Some(kind) = backend.policy().linkable_artifact() else {
        return Err(EmissionPlanningError::MissingLinkableArtifact);
    };

    if backend
        .capabilities()
        .supports_artifact(kind.artifact_kind())
    {
        return Ok(());
    }

    Err(EmissionPlanningError::UnsupportedBackendArtifact(
        kind.artifact_kind(),
    ))
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
    let assembly = request.artifact(ArtifactKind::Assembly);

    if serialization.assembly_syntax_kind() != AssemblySyntaxKind::TargetDefault
        && assembly.is_none()
    {
        return Err(EmissionPlanningError::MissingSerializationArtifact(
            BackendArtifactKind::Assembly,
        ));
    }

    if assembly.is_some_and(|artifact| artifact.requirement() == ArtifactRequirement::Required)
        && !backend
            .capabilities()
            .supports_assembly_syntax_kind(serialization.assembly_syntax_kind())
    {
        return Err(EmissionPlanningError::UnsupportedAssemblySyntax(
            serialization.assembly_syntax_kind(),
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
        | ArtifactKind::PackageInterface
        | ArtifactKind::PackageImplementation => matches!(product, ProductKind::Library),
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::DebugCompanion
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::LinkedCompanion => true,
    }
}

fn request_uses_backend(request: &EmissionRequest) -> bool {
    linked_product(request).is_some()
        || request
            .artifacts()
            .iter()
            .any(|artifact| artifact.kind().backend_kind().is_some())
}

fn request_requires_backend(request: &EmissionRequest) -> bool {
    linked_product(request)
        .is_some_and(|artifact| artifact.requirement() == ArtifactRequirement::Required)
        || request.artifacts().iter().any(|artifact| {
            artifact.requirement() == ArtifactRequirement::Required
                && artifact.kind().backend_kind().is_some()
        })
}

fn request_requires_codegen_units(request: &EmissionRequest) -> bool {
    request.artifacts().iter().any(|artifact| {
        artifact.requirement() == ArtifactRequirement::Required
            && matches!(
                artifact.kind(),
                ArtifactKind::Executable
                    | ArtifactKind::SharedLibrary
                    | ArtifactKind::Assembly
                    | ArtifactKind::BackendIr
                    | ArtifactKind::BackendBitcode
                    | ArtifactKind::RelocatableObject
                    | ArtifactKind::DebugCompanion
            )
    })
}

fn has_planned_backend_work(planner: &EmissionPlanner, request: &EmissionRequest) -> bool {
    linked_product(request)
        .is_some_and(|artifact| should_plan_artifact(planner, request, artifact, false))
        || request.artifacts().iter().any(|artifact| {
            artifact.kind().backend_kind().is_some()
                && should_plan_artifact(planner, request, *artifact, false)
        })
}

pub(super) fn should_plan_artifact(
    planner: &EmissionPlanner,
    request: &EmissionRequest,
    artifact: RequestedArtifact,
    package_interface_available: bool,
) -> bool {
    if artifact.requirement() == ArtifactRequirement::Required {
        return true;
    }

    match artifact.kind() {
        ArtifactKind::PackageInterface => package_interface_available,
        ArtifactKind::Executable | ArtifactKind::StaticLibrary | ArtifactKind::SharedLibrary => {
            optional_linked_product_is_available(planner)
        }
        ArtifactKind::LinkedCompanion => linked_product(request)
            .is_some_and(|product| should_plan_artifact(planner, request, product, false)),
        kind => kind
            .backend_kind()
            .is_none_or(|backend_kind| backend_is_available(planner, backend_kind)),
    }
}

pub(super) fn linked_product(request: &EmissionRequest) -> Option<RequestedArtifact> {
    LINKED_PRODUCT_KINDS
        .into_iter()
        .find_map(|kind| request.artifact(kind))
}

fn linked_product_count(request: &EmissionRequest) -> usize {
    LINKED_PRODUCT_KINDS
        .into_iter()
        .filter(|&kind| request.artifact(kind).is_some())
        .count()
}

fn optional_linked_product_is_available(planner: &EmissionPlanner) -> bool {
    let Some(linkable_kind) = planner
        .backend()
        .and_then(|backend| backend.policy().linkable_artifact())
    else {
        return false;
    };

    backend_is_available(planner, linkable_kind.artifact_kind())
}

fn backend_is_available(planner: &EmissionPlanner, kind: BackendArtifactKind) -> bool {
    planner.backend().is_some_and(|backend| {
        !backend.units().is_empty()
            && backend
                .capabilities()
                .supports_target_machine(planner.target().machine())
            && backend.capabilities().supports_artifact(kind)
    })
}
