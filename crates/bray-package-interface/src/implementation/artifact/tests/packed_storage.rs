use bray_native_artifact::{NativeArtifactIndex, NativeUnitKind};
use bray_symbols::InterfaceSymbolId;
use bray_target::NativeTarget;

use crate::{
    InterfaceConstantCallableBody, InterfaceValidationError, InterfaceValidationLimits,
    encode_package_interface,
};

use super::super::encoding::encode_artifact;
use super::super::{ImplementationPayloadKind, PackageImplementationArtifact};
use super::support::{artifact_fixture, native_digest};

#[test]
fn packed_file_reads_only_metadata_and_demanded_bodies_and_shares_parallel_cache() {
    let fixture = artifact_fixture();

    let identity = super::super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let bodies = std::iter::once(fixture.body.clone())
        .chain((1..10_000).map(|owner| {
            InterfaceConstantCallableBody::new(
                InterfaceSymbolId::new(10_000 + owner),
                fixture.body.template().clone(),
            )
        }))
        .collect::<Vec<_>>();

    let bytes = encode_artifact(&identity, &bodies, &[], &[], &[], None, &[], &[]).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.brayimpl");

    std::fs::write(&path, &bytes).unwrap();

    let input = crate::PackageArtifactInput::packed_file(
        &path,
        PackageImplementationArtifact::metadata_digest(&bytes).unwrap(),
    );

    let artifact = input.load_implementation().unwrap();
    let metadata = artifact.access_statistics();

    assert_eq!(metadata.opens, 1);
    assert_eq!(metadata.reads, 3);
    assert!(metadata.bytes_read < bytes.len() as u64);
    assert!(metadata.payload_bytes_hashed < 4096);

    assert_eq!(
        artifact
            .decoded
            .iter()
            .filter(|cell| cell.get().is_some())
            .count(),
        0
    );

    let surface = fixture.bundle.surface();
    let owner = fixture.body.owner();

    std::thread::scope(|scope| {
        for _ in 0..8 {
            let artifact = artifact.clone();

            scope.spawn(move || {
                assert!(
                    artifact
                        .constant_callable_body(owner, surface)
                        .unwrap()
                        .is_some()
                )
            });
        }
    });

    let demanded = artifact.access_statistics();

    assert_eq!(demanded.reads, metadata.reads + 1);

    assert_eq!(
        artifact
            .decoded
            .iter()
            .filter(|cell| cell.get().is_some())
            .count(),
        1
    );

    assert_eq!(
        input
            .clone()
            .load_implementation()
            .unwrap()
            .access_statistics(),
        demanded
    );

    assert!(demanded.payload_bytes_hashed < 8192);
    artifact.verify_all().unwrap();

    // Complete acquisition verification streams unread bodies without retaining them.
    assert_eq!(
        artifact
            .decoded
            .iter()
            .filter(|cell| cell.get().is_some())
            .count(),
        1
    );
}

#[test]
fn open_file_snapshot_survives_path_replacement_and_new_input_detects_corruption() {
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

    let input = crate::PackageArtifactInput::file(&path, None);
    let old = input.load_implementation().unwrap();

    std::fs::rename(&path, directory.path().join("old.brayimpl")).unwrap();
    std::fs::write(&path, b"corrupt replacement").unwrap();

    assert!(
        old.constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .unwrap()
            .is_some()
    );

    assert!(
        input
            .clone()
            .load_implementation()
            .unwrap()
            .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .unwrap()
            .is_some()
    );

    assert!(
        crate::PackageArtifactInput::file(&path, None)
            .load_implementation()
            .is_err()
    );
}

#[test]
fn object_only_selection_leaves_bitcode_payloads_untouched() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        &fixture.bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let target = NativeTarget::for_identity(artifact.identity().configuration().target()).unwrap();
    let producer = native_digest(b"producer");

    let object = NativeArtifactIndex::try_new(target, producer, [], [])
        .unwrap()
        .encode()
        .unwrap();

    let bitcode = NativeArtifactIndex::try_new(target, producer, [], [])
        .unwrap()
        .with_bitcode_toolchain("llvm-test")
        .encode()
        .unwrap();

    let variants = artifact
        .try_native_only_artifact(
            &[
                (NativeUnitKind::Object, &object),
                (NativeUnitKind::Bitcode, &bitcode),
            ],
            &[],
        )
        .unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.brayimpl");
    let mut bytes = variants.shared_bytes().unwrap().to_vec();

    let bitcode_entry = variants
        .directory
        .iter()
        .find(|entry| {
            entry.kind == Some(ImplementationPayloadKind::NativeIndex)
                && entry.discriminator[0] == 2
        })
        .unwrap();

    bytes[bitcode_entry.payload.start] ^= 1;
    std::fs::write(&path, bytes).unwrap();

    let input = crate::PackageArtifactInput::packed_file(&path, *variants.artifact_hash());
    let loaded = input.load_implementation().unwrap();

    assert_eq!(
        loaded.native_representation(None).unwrap().unwrap().0,
        NativeUnitKind::Object
    );

    assert!(loaded.native_indexes[2].get().is_none());

    assert!(matches!(
        loaded.native_representation(Some("llvm-test")),
        Err(crate::PackageNativeArtifactError::Validation(
            InterfaceValidationError::PayloadChecksumMismatch { .. }
        ))
    ));

    assert!(loaded.verify_all().is_err());
}

#[test]
fn packed_read_errors_preserve_path_and_exact_io_cause() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent.brayimpl");

    assert!(
        matches!(crate::PackageArtifactInput::file(&path, None).load_implementation(),
        Err(crate::PackageArtifactLoadError::Validation(InterfaceValidationError::Read { path: actual, kind: std::io::ErrorKind::NotFound })) if actual == path)
    );

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

    std::fs::write(&path, &bytes).unwrap();

    let selected = crate::PackageArtifactInput::file(&path, None)
        .load_implementation()
        .unwrap();

    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(super::super::HEADER_LENGTH as u64)
        .unwrap();

    assert!(
        matches!(selected.constant_callable_body(fixture.body.owner(), fixture.bundle.surface()),
        Err(InterfaceValidationError::Read { path: actual, kind: std::io::ErrorKind::UnexpectedEof }) if actual == path)
    );
}

#[test]
fn packed_read_diagnostic_supplies_the_message_protocol_arguments() {
    let path = std::path::PathBuf::from("missing/library.brayimpl");

    let diagnostic = InterfaceValidationError::Read {
        path: path.clone(),
        kind: std::io::ErrorKind::NotFound,
    }
    .into_diagnostic(bray_diagnostics::DiagnosticId::new(1));

    assert_eq!(
        diagnostic.kind(),
        bray_diagnostics::DiagnosticKind::PackageArtifactReadFailed
    );

    assert!(
        diagnostic
            .args()
            .contains(&bray_diagnostics::DiagnosticArg::file_path(path))
    );

    assert!(
        diagnostic
            .args()
            .contains(&bray_diagnostics::DiagnosticArg::io_error_kind(
                bray_diagnostics::DiagnosticIoErrorKind::NotFound
            ))
    );

    assert!(
        bray_messages::DiagnosticRenderer::english()
            .render(&diagnostic)
            .message()
            .contains("missing/library.brayimpl")
    );
}

#[test]
fn complete_reads_enforce_metadata_promises_and_share_their_immutable_snapshot() {
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
        crate::PackageArtifactInput::packed_file(&path, [0; 32]).read(),
        Err(crate::PackageArtifactLoadError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));

    let input = crate::PackageArtifactInput::packed_file(&path, *artifact.artifact_hash());

    assert_eq!(input.read().unwrap(), bytes);

    std::fs::write(&path, b"replacement").unwrap();

    let loaded = input.load_implementation().unwrap();

    assert_eq!(loaded.access_statistics().opens, 0);

    assert!(
        loaded
            .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
            .unwrap()
            .is_some()
    );

    assert!(
        crate::PackageArtifactInput::packed_file(&path, *artifact.artifact_hash())
            .load_implementation()
            .is_err()
    );
}
