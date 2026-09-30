use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInput, LinkInputId, LinkInputKind, LinkInputMode,
    LinkInputProvenance, LinkInputSource, LinkPlan, LinkPlanBuilder, LinkPlanSelectionError,
    LinkPolicy, LinkStartupMode, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind,
    Linker, PlannedLinkedArtifact, SectionGarbageCollectionPolicy, StagingDestination,
    StagingDestinationId, StagingPathKey,
};

use super::{LinkPlanConstructionError, LinkStaging, ProductLinkInputs, StagedArtifact};
use crate::{ArtifactKind, EmissionPlan};

/// Plans an opaque member archive inside the existing library publication transaction.
pub fn construct_package_archive_plan(
    emission: &EmissionPlan,
    staging: &LinkStaging,
    members: &[StagedArtifact],
    inputs: &ProductLinkInputs,
    linker: &Linker,
) -> Result<LinkPlan, LinkPlanConstructionError> {
    let name = bray_target::TargetOutputName::for_native(
        inputs.target.object_format(),
        bray_target::TargetOutputKind::StaticLibrary,
    )
    .file_name("package-native-opaque")
    .expect("fixed native archive stem must be valid");

    let path = staging.directory().join(name);

    let destination = StagingDestination::try_new(
        StagingDestinationId::new(0),
        path,
        StagingPathKey::try_new("package-native-opaque").expect("fixed staging key must be valid"),
    )
    .map_err(LinkPlanConstructionError::InvalidOutputStaging)?;

    linker
        .select_plan(|driver| {
            let mut builder = LinkPlanBuilder::new(
                emission.request().product().clone(),
                LinkedProductKind::StaticLibrary,
                inputs.target.clone(),
                driver.clone(),
                LinkStartupMode::NotApplicable,
                LinkPolicy::new(
                    DeadStripPolicy::Preserve,
                    SectionGarbageCollectionPolicy::Preserve,
                    DebugLinkPolicy::None,
                    None,
                ),
            );

            for (ordinal, member) in members.iter().enumerate() {
                let planned = emission
                    .artifact(member.artifact())
                    .expect("archive member must belong to emission plan");

                let kind = match planned.id().kind() {
                    ArtifactKind::RelocatableObject => LinkInputKind::RelocatableObject,
                    ArtifactKind::BackendBitcode => LinkInputKind::Bitcode,
                    _ => {
                        return Err(LinkPlanConstructionError::UnsupportedStagedArtifact(
                            member.artifact().clone(),
                        ));
                    }
                };

                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| LinkPlanConstructionError::InputOrdinalOverflow)?;

                builder.push_input(
                    LinkInput::try_new(
                        LinkInputId::new(ordinal),
                        kind,
                        LinkInputSource::file(member.path()),
                        LinkInputProvenance::Package(
                            emission.request().product().package().clone(),
                        ),
                        LinkInputMode::Ordinary,
                    )
                    .map_err(LinkPlanConstructionError::InvalidInput)?,
                );
            }

            builder.push_output(PlannedLinkedArtifact::new(
                LinkedArtifactKind::StaticLibrary,
                LinkedArtifactRequirement::Required,
                destination.clone(),
            ));

            builder
                .finish()
                .map_err(LinkPlanConstructionError::InvalidLinkPlan)
        })
        .map_err(|error| match error {
            LinkPlanSelectionError::Construction(error) => error,
            LinkPlanSelectionError::Link(error) => LinkPlanConstructionError::Linker(error),
        })
}
