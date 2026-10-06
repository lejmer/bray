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

    let original = encode_artifact(&identity, &[], &[], &[], &[], None, &[], &[]).unwrap();

    let replacement =
        encode_artifact(&replacement_identity, &[], &[], &[], &[], None, &[], &[]).unwrap();

    let limits = InterfaceValidationLimits::default();
    let parsed = PackageImplementationArtifact::try_from_bytes(original.clone(), limits).unwrap();

    let replacement_parsed =
        PackageImplementationArtifact::try_from_bytes(replacement.clone(), limits).unwrap();

    assert_eq!(original.len(), replacement.len());
    assert_ne!(parsed.identity(), replacement_parsed.identity());

    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("changed.brayimpl");

    std::fs::write(&path, &original).unwrap();

    let opened = PackageImplementationArtifact::try_open(&path, limits).unwrap();

    std::fs::write(&path, &replacement).unwrap();

    assert!(matches!(
        opened.verify_digest(*blake3::hash(&replacement).as_bytes()),
        Err(InterfaceValidationError::ArtifactHashMismatch { .. })
    ));

    for offset in [
        super::super::CONTENT_HASH_OFFSET,
        original.len() - 1,
        parsed.directory[0].payload.end - 1,
    ] {
        std::fs::write(&path, &original).unwrap();

        let opened = PackageImplementationArtifact::try_open(&path, limits).unwrap();
        let mut changed = original.to_vec();

        changed[offset] ^= 1;
        std::fs::write(&path, &changed).unwrap();

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
    .unwrap();

    let pristine = PackageImplementationArtifact::try_from_bytes(
        Arc::clone(&bytes),
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let entry = pristine
        .directory
        .iter()
        .find(|entry| entry.owner.raw() == 999)
        .unwrap();

    let mut corrupt = bytes.to_vec();

    corrupt[entry.payload.start] ^= 1;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &corrupt).unwrap();

    let artifact = crate::PackageArtifactInput::packed_file(&path, *pristine.artifact_hash())
        .load_implementation()
        .unwrap();

    assert!(
        artifact
            .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .unwrap()
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
    std::fs::write(&path, &metadata_corrupt).unwrap();

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
    .unwrap();

    let bytes = artifact.shared_bytes().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &bytes).unwrap();

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
        .unwrap();

    let mut damaged = bytes.to_vec();

    damaged[entry.payload.start] ^= 1;
    std::fs::write(&path, damaged).unwrap();

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
    .unwrap();

    let bytes = artifact.shared_bytes().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.brayimpl");

    std::fs::write(&path, &bytes).unwrap();

    let selected = crate::PackageArtifactInput::file(&path, None)
        .load_implementation()
        .unwrap();

    let mut damaged = bytes.to_vec();

    *damaged.last_mut().unwrap() ^= 1;
    std::fs::write(&path, damaged).unwrap();

    assert!(matches!(
        selected.verify_all(),
        Err(crate::PackageNativeArtifactError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));

    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(bytes.len() as u64 + 1)
        .unwrap();

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
