use std::sync::Arc;

use bray_symbols::InterfaceSymbolId;

use crate::{InterfaceConstantCallableBody, InterfaceValidationError, InterfaceValidationLimits};

use super::super::encoding::encode_artifact;
use super::super::{ImplementationPayloadKind, PackageImplementationArtifact};
use super::support::artifact_fixture;

#[test]
fn streamed_digest_binds_metadata_and_identity_to_the_authenticated_bytes() {
    let fixture = artifact_fixture();

    let identity = super::super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let replacement_identity = crate::PackageImplementationIdentity::new(
        identity.interface().clone(),
        crate::InterfaceContentHash::from_bytes([0xa5; 32]),
        identity.language_revision(),
        identity.dependencies().iter().cloned(),
        identity.configuration().clone(),
        identity.runtime_requirements().iter().cloned(),
    );

    let original = encode_artifact(&identity, &[], &[], &[], &[], None, &[], &[]).expect("fixture implementation artifact must encode");

    let replacement =
        encode_artifact(&replacement_identity, &[], &[], &[], &[], None, &[], &[]).expect("fixture implementation artifact must encode");

    let limits = InterfaceValidationLimits::default();
    let parsed = PackageImplementationArtifact::try_from_bytes(original.clone(), limits).expect("fixture from bytes must succeed");

    let replacement_parsed =
        PackageImplementationArtifact::try_from_bytes(replacement.clone(), limits).expect("fixture from bytes must succeed");

    assert_eq!(original.len(), replacement.len());
    assert_ne!(parsed.identity(), replacement_parsed.identity());

    let temporary = tempfile::tempdir().expect("temporary fixture directory must be created");
    let path = temporary.path().join("changed.brayimpl");

    std::fs::write(&path, &original).expect("fixture contents must be written");

    let opened = PackageImplementationArtifact::try_open(&path, limits).expect("fixture open must succeed");

    std::fs::write(&path, &replacement).expect("fixture contents must be written");

    assert!(matches!(
        opened.verify_digest(*blake3::hash(&replacement).as_bytes()),
        Err(InterfaceValidationError::ArtifactHashMismatch { .. })
    ));

    for offset in [
        super::super::CONTENT_HASH_OFFSET,
        original.len() - 1,
        parsed.directory[0].payload.end - 1,
    ] {
        std::fs::write(&path, &original).expect("fixture contents must be written");

        let opened = PackageImplementationArtifact::try_open(&path, limits).expect("fixture open must succeed");
        let mut changed = original.to_vec();

        changed[offset] ^= 1;
        std::fs::write(&path, &changed).expect("fixture contents must be written");

        let result = opened.verify_digest(*blake3::hash(&changed).as_bytes());

        if offset == parsed.directory[0].payload.end - 1 {
            assert!(matches!(
                result,
                Err(InterfaceValidationError::PayloadChecksumMismatch { .. })
            ));
        } else {
            assert!(matches!(
                result,
                Err(InterfaceValidationError::ArtifactHashMismatch { .. })
            ));
        }
    }
}

#[test]
fn unread_corruption_is_rejected_on_selection_and_complete_verification() {
    let fixture = artifact_fixture();

    let identity = super::super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let second = InterfaceConstantCallableBody::new(
        InterfaceSymbolId::new(999),
        fixture.body.template().clone(),
    );

    let bytes = encode_artifact(
        &identity,
        &[fixture.body.clone(), second],
        &[],
        &[],
        &[],
        None,
        &[],
        &[],
    )
    .expect("fixture implementation artifact must encode");

    let pristine = PackageImplementationArtifact::try_from_bytes(
        Arc::clone(&bytes),
        InterfaceValidationLimits::default(),
    )
    .expect("fixture from bytes must succeed");

    let entry = pristine
        .directory
        .iter()
        .find(|entry| entry.owner.raw() == 999)
        .expect("fixture lookup must find its expected value");

    let mut corrupt = bytes.to_vec();

    corrupt[entry.payload.start] ^= 1;

    let directory = tempfile::tempdir().expect("temporary fixture directory must be created");
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &corrupt).expect("fixture contents must be written");

    let artifact = crate::PackageArtifactInput::packed_file(&path, *pristine.artifact_hash())
        .load_implementation()
        .expect("fixture implementation must load");

    assert!(
        artifact
            .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .expect("fixture `constant_callable_body` call must succeed")
            .is_some()
    );

    assert!(matches!(
        artifact.constant_callable_body(InterfaceSymbolId::new(999), fixture.bundle.surface()),
        Err(InterfaceValidationError::PayloadChecksumMismatch { .. })
    ));

    assert!(matches!(
        artifact.verify_all(),
        Err(crate::PackageNativeArtifactError::Validation(
            InterfaceValidationError::PayloadChecksumMismatch { .. }
        ))
    ));

    let mut metadata_corrupt = bytes.to_vec();
    let last = metadata_corrupt.len() - 1;

    metadata_corrupt[last] ^= 1;
    std::fs::write(&path, &metadata_corrupt).expect("fixture contents must be written");

    assert!(matches!(
        crate::PackageArtifactInput::file(&path, None).load_implementation(),
        Err(crate::PackageArtifactLoadError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));
}

#[test]
fn packed_metadata_and_full_digest_promises_are_distinct_and_enforced() {
    let fixture = artifact_fixture();

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [fixture.body.clone()],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .expect("fixture package implementation artifact must be valid");

    let bytes = artifact.shared_bytes().expect("fixture `shared_bytes` call must succeed");
    let directory = tempfile::tempdir().expect("temporary fixture directory must be created");
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &bytes).expect("fixture contents must be written");

    assert!(matches!(
        crate::PackageArtifactInput::packed_file(&path, [0; 32]).load_implementation(),
        Err(crate::PackageArtifactLoadError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));

    let digest = *blake3::hash(&bytes).as_bytes();

    let entry = artifact
        .directory
        .iter()
        .find(|entry| entry.kind == Some(ImplementationPayloadKind::ConstantCallableBody))
        .expect("fixture lookup must find its expected value");

    let mut damaged = bytes.to_vec();

    damaged[entry.payload.start] ^= 1;
    std::fs::write(&path, damaged).expect("fixture contents must be written");

    assert!(
        crate::PackageArtifactInput::packed_file(&path, *artifact.artifact_hash())
            .load_implementation()
            .is_ok()
    );

    assert!(matches!(
        crate::PackageArtifactInput::file(&path, Some(digest)).load_implementation(),
        Err(crate::PackageArtifactLoadError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));
}

#[test]
fn complete_verification_rechecks_in_place_metadata_and_file_length() {
    let fixture = artifact_fixture();

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [fixture.body.clone()],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .expect("fixture package implementation artifact must be valid");

    let bytes = artifact.shared_bytes().expect("fixture `shared_bytes` call must succeed");
    let directory = tempfile::tempdir().expect("temporary fixture directory must be created");
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &bytes).expect("fixture contents must be written");

    let selected = crate::PackageArtifactInput::file(&path, None)
        .load_implementation()
        .expect("fixture implementation must load");

    let mut damaged = bytes.to_vec();

    *damaged.last_mut().expect("fixture `last_mut` call must succeed") ^= 1;
    std::fs::write(&path, damaged).expect("fixture contents must be written");

    assert!(matches!(
        selected.verify_all(),
        Err(crate::PackageNativeArtifactError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));

    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("fixture file must open")
        .set_len(bytes.len() as u64 + 1)
        .expect("fixture `set_len` call must succeed");

    assert!(matches!(
        selected.verify_all(),
        Err(crate::PackageNativeArtifactError::Validation(
            InterfaceValidationError::Malformed {
                cause: crate::InterfaceMalformedCause::LengthMismatch { .. },
                ..
            }
        ))
    ));
}
