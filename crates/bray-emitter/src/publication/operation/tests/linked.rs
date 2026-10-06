use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_linker::{
    DebugLinkPolicy, LinkFailure, LinkInput, LinkInputId, LinkInputKind, LinkInputMode,
    LinkInputProvenance, LinkInputSource, LinkModel, LinkOutcome, LinkPlan, LinkPlanBuilder,
    LinkPolicy, LinkTarget, LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement,
    LinkedProductKind, LinkerDriverIdentity, LinkerDriverKind, PlannedLinkedArtifact,
    SectionGarbageCollectionPolicy, StagingDestination, StagingDestinationId, StagingPathKey,
};
use bray_target::{CodeModel, ObjectFormat, RelocationModel, TargetArchitecture};
use bray_testing::assert_goal_state_diagnostic_kind;

use super::fixtures::{always_cancelled, file_bytes, never_cancelled};

use crate::test_support::{product_identity, target_identity};
use crate::{
    ArtifactId, ArtifactKind, ArtifactProducer, ArtifactPublisher, ArtifactRequirement,
    ArtifactRole, EmissionFailure, EmissionPlan, EmissionRequest, EmissionStatus, LinkerProducerId,
    OutputSink, PlannedArtifact, PlannedArtifactDestination, ProductKind, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};

#[test]
fn linked_products_and_companions_publish_from_validated_staging() {
    let cases = [
        linked_case(
            LinkedProductKind::Executable,
            ArtifactKind::Executable,
            LinkedArtifactKind::Executable,
            None,
        ),
        linked_case(
            LinkedProductKind::SharedLibrary,
            ArtifactKind::SharedLibrary,
            LinkedArtifactKind::SharedLibrary,
            Some(LinkedArtifactKind::ImportLibrary),
        ),
        linked_case(
            LinkedProductKind::Executable,
            ArtifactKind::Executable,
            LinkedArtifactKind::Executable,
            Some(LinkedArtifactKind::DebugCompanion),
        ),
        linked_case(
            LinkedProductKind::StaticLibrary,
            ArtifactKind::StaticLibrary,
            LinkedArtifactKind::StaticLibrary,
            Some(LinkedArtifactKind::PlatformCompanion),
        ),
    ];

    for case in cases {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let fixture = linked_publication_fixture(directory.path(), case);

        let outcome = ArtifactPublisher::new(&never_cancelled).publish_linked(
            &fixture.emission,
            [],
            &fixture.link,
            &fixture.outcome,
        );

        assert!(
            matches!(outcome.status(), EmissionStatus::Complete),
            "unexpected linked emission outcome: {outcome:#?}"
        );

        assert_eq!(
            outcome.artifacts().artifacts().len(),
            fixture.final_artifacts.len()
        );

        let generation = outcome
            .generation()
            .unwrap_or_else(|| panic!("managed publication must expose its generation"));

        for (record, artifact) in outcome
            .artifacts()
            .artifacts()
            .iter()
            .zip(&fixture.final_artifacts)
        {
            let path = generation
                .artifact_path(record.id())
                .unwrap_or_else(|| panic!("published artifact path must resolve"));

            assert_eq!(file_bytes(&path), artifact.bytes);
            assert!(!artifact.staging_path.exists());
        }

        assert!(!fixture.input_path.exists());
    }
}

#[test]
fn linked_output_validation_preserves_every_existing_destination() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let case = linked_case(
        LinkedProductKind::SharedLibrary,
        ArtifactKind::SharedLibrary,
        LinkedArtifactKind::SharedLibrary,
        Some(LinkedArtifactKind::ImportLibrary),
    );

    let fixture = linked_publication_fixture(directory.path(), case);
    let missing = &fixture.final_artifacts[1];

    std::fs::remove_file(&missing.staging_path)
        .unwrap_or_else(|error| panic!("test companion staging must be removed: {error}"));

    for artifact in &fixture.final_artifacts {
        std::fs::write(&artifact.final_path, b"existing")
            .unwrap_or_else(|error| panic!("test destination must be written: {error}"));
    }

    let outcome = ArtifactPublisher::new(&never_cancelled).publish_linked(
        &fixture.emission,
        [],
        &fixture.link,
        &fixture.outcome,
    );

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
    ));

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::EmissionArtifactReadFailed
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionArtifactReadFailed,
    );

    for artifact in &fixture.final_artifacts {
        assert_eq!(file_bytes(&artifact.final_path), b"existing");
        assert!(!artifact.staging_path.exists());
    }

    assert!(!fixture.input_path.exists());
}

#[test]
fn invalid_linked_output_length_preserves_the_existing_destination() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let case = linked_case(
        LinkedProductKind::Executable,
        ArtifactKind::Executable,
        LinkedArtifactKind::Executable,
        None,
    );

    let mut fixture = linked_publication_fixture(directory.path(), case);
    let artifact = &fixture.final_artifacts[0];

    std::fs::write(&artifact.final_path, b"existing")
        .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

    let linked = fixture.link.outputs().iter().map(|output| {
        LinkedArtifact::new(output.kind(), output.destination().id(), NonZeroU64::MIN)
    });

    fixture.outcome = LinkOutcome::complete(linked, DiagnosticBag::new());

    let outcome = ArtifactPublisher::new(&never_cancelled).publish_linked(
        &fixture.emission,
        [],
        &fixture.link,
        &fixture.outcome,
    );

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
    ));

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::EmissionArtifactLengthMismatch
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionArtifactLengthMismatch,
    );

    assert_eq!(file_bytes(&artifact.final_path), b"existing");
    assert!(!artifact.staging_path.exists());
    assert!(!fixture.input_path.exists());
}

#[test]
fn link_failure_and_cancellation_preserve_existing_destinations() {
    for failed in [true, false] {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let case = linked_case(
            LinkedProductKind::Executable,
            ArtifactKind::Executable,
            LinkedArtifactKind::Executable,
            None,
        );

        let mut fixture = linked_publication_fixture(directory.path(), case);
        let artifact = &fixture.final_artifacts[0];

        std::fs::write(&artifact.final_path, b"existing")
            .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

        fixture.outcome = if failed {
            LinkOutcome::failed(&fixture.link, LinkFailure::Invocation, DiagnosticBag::new())
        } else {
            LinkOutcome::cancelled(DiagnosticBag::new())
        };

        let outcome = ArtifactPublisher::new(&never_cancelled).publish_linked(
            &fixture.emission,
            [],
            &fixture.link,
            &fixture.outcome,
        );

        assert!(!matches!(outcome.status(), EmissionStatus::Complete));
        assert_eq!(file_bytes(&artifact.final_path), b"existing");
        assert!(!artifact.staging_path.exists());
        assert!(!fixture.input_path.exists());
    }
}

#[test]
fn cancellation_before_linked_validation_cleans_private_staging() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let case = linked_case(
        LinkedProductKind::Executable,
        ArtifactKind::Executable,
        LinkedArtifactKind::Executable,
        None,
    );

    let fixture = linked_publication_fixture(directory.path(), case);
    let artifact = &fixture.final_artifacts[0];

    std::fs::write(&artifact.final_path, b"existing")
        .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

    let outcome = ArtifactPublisher::new(&always_cancelled).publish_linked(
        &fixture.emission,
        [],
        &fixture.link,
        &fixture.outcome,
    );

    assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
    assert_eq!(file_bytes(&artifact.final_path), b"existing");
    assert!(!artifact.staging_path.exists());
    assert!(!fixture.input_path.exists());
}

#[derive(Clone, Copy)]
struct LinkedPublicationCase {
    product: LinkedProductKind,
    artifact: ArtifactKind,
    linked: LinkedArtifactKind,
    companion: Option<LinkedArtifactKind>,
}

struct LinkedPublicationFixture {
    emission: EmissionPlan,
    link: LinkPlan,
    outcome: LinkOutcome,
    input_path: PathBuf,
    final_artifacts: Vec<LinkedFinalArtifact>,
}

struct LinkedFinalArtifact {
    final_path: PathBuf,
    staging_path: PathBuf,
    bytes: &'static [u8],
}

const fn linked_case(
    product: LinkedProductKind,
    artifact: ArtifactKind,
    linked: LinkedArtifactKind,
    companion: Option<LinkedArtifactKind>,
) -> LinkedPublicationCase {
    LinkedPublicationCase {
        product,
        artifact,
        linked,
        companion,
    }
}

fn linked_publication_fixture(
    directory: &Path,
    case: LinkedPublicationCase,
) -> LinkedPublicationFixture {
    let product_kind = match case.product {
        LinkedProductKind::Executable => ProductKind::Executable,
        LinkedProductKind::SharedLibrary | LinkedProductKind::StaticLibrary => ProductKind::Library,
    };

    let executable_host = (product_kind == ProductKind::Executable)
        .then(crate::test_support::executable_host_contract);

    let mut requested = vec![RequestedArtifact::new(
        case.artifact,
        ArtifactRequirement::Required,
    )];

    if case.companion.is_some() {
        requested.push(RequestedArtifact::new(
            ArtifactKind::LinkedCompanion,
            ArtifactRequirement::Required,
        ));
    }

    let Ok(request) = EmissionRequest::try_new(
        product_identity(),
        product_kind,
        executable_host,
        target_identity(),
        RequestedArtifactDestination::FilesystemDirectory(directory.to_owned().into()),
        requested,
        ReplacementPolicy::ReplaceExisting,
    ) else {
        panic!("test linked emission request must be valid");
    };

    let primary_final = directory.join("primary.final");
    let primary_staging = directory.join("primary.stage");

    let mut planned = vec![linked_planned_artifact(
        &request,
        case.artifact,
        ArtifactRole::Product,
        primary_final.clone(),
    )];

    let mut final_artifacts = vec![LinkedFinalArtifact {
        final_path: primary_final,
        staging_path: primary_staging.clone(),
        bytes: b"primary linked bytes",
    }];

    if case.companion.is_some() {
        let companion_final = directory.join("companion.final");
        let companion_staging = directory.join("companion.stage");

        planned.push(linked_planned_artifact(
            &request,
            ArtifactKind::LinkedCompanion,
            ArtifactRole::Companion,
            companion_final.clone(),
        ));

        final_artifacts.push(LinkedFinalArtifact {
            final_path: companion_final,
            staging_path: companion_staging,
            bytes: b"companion linked bytes",
        });
    }

    let emission = EmissionPlan::new(request, None, None, planned, [], None);

    let input_path = directory.join("input.o");

    std::fs::write(&input_path, b"object")
        .unwrap_or_else(|error| panic!("test link input must be written: {error}"));

    let mut builder = LinkPlanBuilder::new(
        product_identity(),
        case.product,
        linked_target(case.product),
        linked_driver_identity(),
        match case.product {
            LinkedProductKind::Executable | LinkedProductKind::SharedLibrary => {
                bray_linker::LinkStartupMode::PlatformCompilerDriver
            }
            LinkedProductKind::StaticLibrary => bray_linker::LinkStartupMode::NotApplicable,
        },
        LinkPolicy::new(
            bray_linker::DeadStripPolicy::Preserve,
            SectionGarbageCollectionPolicy::Preserve,
            if case.companion == Some(LinkedArtifactKind::DebugCompanion) {
                DebugLinkPolicy::Companion
            } else {
                DebugLinkPolicy::None
            },
            None,
        ),
    );

    builder.push_input(linked_input(&input_path));

    builder.push_output(linked_output(0, case.linked, &primary_staging));

    if let Some(companion) = case.companion {
        builder.push_output(linked_output(
            1,
            companion,
            &final_artifacts[1].staging_path,
        ));
    }

    if case.product == LinkedProductKind::Executable {
        builder.set_entry_point(
            crate::test_support::executable_host_contract()
                .native_entry()
                .clone(),
        );
    }

    let link = builder
        .finish()
        .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"));

    for artifact in &final_artifacts {
        std::fs::write(&artifact.staging_path, artifact.bytes)
            .unwrap_or_else(|error| panic!("test linked staging must be written: {error}"));
    }

    let artifacts = link
        .outputs()
        .iter()
        .zip(&final_artifacts)
        .map(|(output, artifact)| {
            let Some(byte_len) = NonZeroU64::new(
                u64::try_from(artifact.bytes.len())
                    .unwrap_or_else(|_| panic!("test linked length must be representable")),
            ) else {
                panic!("test linked staging must be nonempty");
            };

            LinkedArtifact::new(output.kind(), output.destination().id(), byte_len)
        });

    let outcome = LinkOutcome::complete(artifacts, DiagnosticBag::new());

    LinkedPublicationFixture {
        emission,
        link,
        outcome,
        input_path,
        final_artifacts,
    }
}

fn linked_planned_artifact(
    request: &EmissionRequest,
    kind: ArtifactKind,
    role: ArtifactRole,
    path: PathBuf,
) -> PlannedArtifact {
    let root = path
        .parent()
        .unwrap_or_else(|| panic!("test linked path must have a parent"));

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("test linked path must have a portable name"));

    PlannedArtifact::new(
        ArtifactId::new(request.product().clone(), kind, 0),
        ArtifactRequirement::Required,
        role,
        ArtifactProducer::Linker(LinkerProducerId::new(0)),
        PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
            root: root.to_owned(),
            artifact: crate::ManagedArtifactPath::try_new(name)
                .unwrap_or_else(|| panic!("test managed path must be valid")),
            published: path,
        }),
    )
}

fn linked_input(path: &Path) -> LinkInput {
    LinkInput::try_new(
        LinkInputId::new(0),
        LinkInputKind::RelocatableObject,
        LinkInputSource::file(path),
        LinkInputProvenance::Product,
        LinkInputMode::Ordinary,
    )
    .unwrap_or_else(|error| panic!("test link input must be valid: {error:?}"))
}

fn linked_output(ordinal: u32, kind: LinkedArtifactKind, path: &Path) -> PlannedLinkedArtifact {
    let Some(path_key) = StagingPathKey::try_new(path.to_string_lossy().into_owned()) else {
        panic!("test staging path identity must be valid");
    };

    let destination =
        StagingDestination::try_new(StagingDestinationId::new(ordinal), path, path_key)
            .unwrap_or_else(|error| panic!("test staging destination must be valid: {error:?}"));

    PlannedLinkedArtifact::new(kind, LinkedArtifactRequirement::Required, destination)
}

fn linked_target(product: LinkedProductKind) -> LinkTarget {
    LinkTarget::try_new(
        target_identity(),
        "x86_64-unknown-linux-gnu",
        TargetArchitecture::X86_64,
        ObjectFormat::Elf,
        RelocationModel::PositionIndependent,
        CodeModel::Small,
        if product == LinkedProductKind::StaticLibrary {
            LinkModel::Static
        } else {
            LinkModel::Dynamic
        },
    )
    .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
}

fn linked_driver_identity() -> LinkerDriverIdentity {
    LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
        .unwrap_or_else(|| panic!("test linker identity must be valid"))
}
