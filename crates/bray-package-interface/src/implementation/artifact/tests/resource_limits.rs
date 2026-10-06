use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_symbols::{InterfaceSymbolId, NativeSymbolContract};
use bray_target::NativeTarget;

use crate::implementation::hash::compute_payload_hash;
use crate::{
    InterfaceExecutableTemplate, InterfaceValidationError, InterfaceValidationLimits,
    PackageImplementationArtifactBuildError, encode_package_interface,
};

use super::super::encoding::encode_artifact;
use super::super::{ARTIFACT_HASH_OFFSET, PackageImplementationArtifact};
use super::support::{artifact_fixture, generic_callable_owner, native_digest};

#[test]
fn artifacts_bound_executable_payloads_by_the_validation_policy() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let template = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::ROOT,
        1,
        vec![1_u8; 1_200],
    )
    .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let result = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [template],
        [],
        [],
        InterfaceValidationLimits::default().with_blob_length(1_000),
    );

    assert_eq!(
        result,
        Err(PackageImplementationArtifactBuildError::InvalidArtifact(
            InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::BlobLength,
                actual: 1_200,
                maximum: 1_000,
            }
        ))
    );
}

#[test]
fn artifacts_bound_directory_entries_independently_of_interface_sections() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let template =
        InterfaceExecutableTemplate::new(owner, bray_ir::MirExecutableTemplateId::ROOT, 1, [1_u8])
            .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let accepted = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [template.clone()],
        [],
        [],
        InterfaceValidationLimits::default().with_section_count(0),
    );

    assert!(accepted.is_ok());

    let rejected = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [template],
        [],
        [],
        InterfaceValidationLimits::default().with_implementation_entry_count(1),
    );

    assert_eq!(
        rejected,
        Err(PackageImplementationArtifactBuildError::InvalidArtifact(
            InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::ImplementationEntryCount,
                actual: 2,
                maximum: 1,
            }
        ))
    );
}

#[test]
fn concurrent_demands_share_one_lazily_decompressed_payload() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let template = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::ROOT,
        1,
        vec![0_u8; 4_096],
    )
    .unwrap_or_else(|| panic!("compressed executable payload must be valid"));

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [template],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("compressed artifact must validate: {error:?}"));

    let first_artifact = artifact.clone();
    let second_artifact = artifact.clone();

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            first_artifact
                .executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT)
                .unwrap_or_else(|error| panic!("first demand must decode: {error:?}"))
                .unwrap_or_else(|| panic!("first demand must find the template"))
        });

        let second = scope.spawn(|| {
            second_artifact
                .executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT)
                .unwrap_or_else(|error| panic!("second demand must decode: {error:?}"))
                .unwrap_or_else(|| panic!("second demand must find the template"))
        });

        (
            first
                .join()
                .unwrap_or_else(|_| panic!("first demand must complete")),
            second
                .join()
                .unwrap_or_else(|_| panic!("second demand must complete")),
        )
    });

    assert!(std::ptr::eq(first.payload(), second.payload()));
}

#[test]
fn packed_storage_above_interface_ceiling_keeps_metadata_and_payload_bounds() {
    use std::io::{Seek, SeekFrom, Write};

    use crate::implementation::hash::{compute_metadata_hash, compute_payload_content_hash};
    use crate::{InterfaceLimit, InterfaceSectionCompatibility, InterfaceSectionEncoding};

    let fixture = artifact_fixture();

    let identity = super::super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let bytes = encode_artifact(&identity, &[], &[], &[], &[], None, &[], &[]).unwrap();

    let artifact = PackageImplementationArtifact::try_from_bytes(
        bytes.clone(),
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let original_offset =
        usize::try_from(u64::from_le_bytes(bytes[32..40].try_into().unwrap())).unwrap();

    let mut header = bytes[..super::super::HEADER_LENGTH].to_vec();
    let mut directory_bytes = bytes[original_offset..].to_vec();
    let payload = vec![0; 16 * 1024 * 1024];
    let mut offset = original_offset;

    for ordinal in 1..=17_u8 {
        let mut entry = artifact.directory[0].clone();

        entry.owner = InterfaceSymbolId::new(u32::MAX);
        entry.raw_kind = u8::MAX;
        entry.kind = None;
        entry.compatibility = InterfaceSectionCompatibility::PreserveOpaque;
        entry.encoding = InterfaceSectionEncoding::Raw;
        entry.discriminator = [ordinal; 32];
        entry.family_size = 0;
        entry.platform_service = None;
        entry.decoded_length = payload.len() as u64;
        entry.record_count = 1;
        entry.payload = offset..offset + payload.len();

        entry.content_hash = compute_payload_content_hash(
            entry.owner,
            entry.raw_kind,
            entry.discriminator,
            &payload,
        );

        entry.checksum = compute_payload_hash(&entry, &payload);

        let mut encoded = crate::wire::WireEncoder::new();

        encoded.write_u32(entry.owner.raw());
        encoded.write_u8(entry.raw_kind);
        encoded.write_u8(entry.compatibility.wire_value());
        encoded.write_u8(entry.encoding.wire_value());
        encoded.write_u8(0);
        encoded.write_u16(crate::InterfaceSectionRevision::CURRENT.raw());
        encoded.write_u16(0);
        encoded.write_bytes(&entry.discriminator);
        encoded.write_u32(0);
        encoded.write_u32(0);
        encoded.write_u64(offset as u64);
        encoded.write_u64(payload.len() as u64);
        encoded.write_u64(entry.decoded_length);
        encoded.write_u64(entry.record_count);
        encoded.write_bytes(&entry.checksum);
        encoded.write_bytes(&entry.content_hash);

        directory_bytes.extend_from_slice(encoded.bytes());
        offset = entry.payload.end;
    }

    let file_length = offset + directory_bytes.len();

    header[24..32].copy_from_slice(&(file_length as u64).to_le_bytes());
    header[32..40].copy_from_slice(&(offset as u64).to_le_bytes());
    header[40..48].copy_from_slice(&(directory_bytes.len() as u64).to_le_bytes());

    let metadata_hash = compute_metadata_hash(&header, &directory_bytes).unwrap();

    header[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32].copy_from_slice(&metadata_hash);

    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("large.brayimpl");
    let mut file = std::fs::File::create(&path).unwrap();

    file.write_all(&header).unwrap();

    file.write_all(&bytes[super::super::HEADER_LENGTH..original_offset])
        .unwrap();

    file.seek(SeekFrom::Start(offset as u64)).unwrap();
    file.write_all(&directory_bytes).unwrap();
    file.sync_all().unwrap();

    let limits = InterfaceValidationLimits::default();

    assert!(file_length as u64 > limits.maximum(InterfaceLimit::FileSize));

    let loaded =
        PackageImplementationArtifact::try_open(&path, limits.with_decoded_allocation(128 * 1024))
            .unwrap();

    assert_eq!(loaded.identity(), artifact.identity());
    assert!(loaded.access_statistics().bytes_read < 16 * 1024);
    assert_eq!(loaded.access_statistics().bytes_decompressed, 0);

    assert!(matches!(
        loaded.shared_bytes(),
        Err(InterfaceValidationError::ResourceLimitExceeded {
            limit: InterfaceLimit::DecodedAllocation,
            ..
        })
    ));

    assert!(matches!(
        PackageImplementationArtifact::try_open(
            &path,
            limits.with_implementation_file_size(limits.maximum(InterfaceLimit::FileSize)),
        ),
        Err(InterfaceValidationError::ResourceLimitExceeded {
            limit: InterfaceLimit::ImplementationFileSize,
            ..
        })
    ));

    for (restricted, expected) in [
        (
            limits.with_decoded_allocation(1),
            InterfaceLimit::DecodedAllocation,
        ),
        (
            limits.with_blob_length(payload.len() as u64 - 1),
            InterfaceLimit::BlobLength,
        ),
        (
            limits.with_implementation_entry_count(17),
            InterfaceLimit::ImplementationEntryCount,
        ),
    ] {
        assert!(matches!(
            PackageImplementationArtifact::try_open(&path, restricted),
            Err(InterfaceValidationError::ResourceLimitExceeded { limit, .. }) if limit == expected
        ));
    }

    let loaded = PackageImplementationArtifact::try_open(&path, limits).unwrap();

    loaded.verify_all().unwrap();

    assert!(loaded.decoded.iter().all(|cell| cell.get().is_none()));

    let mut digest = blake3::Hasher::new();

    digest.update(&header);
    digest.update(&bytes[super::super::HEADER_LENGTH..original_offset]);

    for _ in 0..17 {
        digest.update(&payload);
    }

    digest.update(&directory_bytes);

    let expected = *digest.finalize().as_bytes();
    let input = crate::PackageArtifactInput::file(&path, Some(expected));
    let loaded = input.load_implementation().unwrap();

    assert_eq!(loaded.identity(), artifact.identity());
    assert_eq!(loaded.access_statistics().opens, 1);
    assert!(loaded.decoded.iter().all(|cell| cell.get().is_none()));

    assert_eq!(
        loaded.access_statistics().payload_bytes_hashed,
        file_length as u64
            + artifact.access_statistics().payload_bytes_hashed
            + artifact.directory[0].payload.len() as u64
    );

    file.seek(SeekFrom::Start(offset as u64 - 1)).unwrap();
    file.write_all(&[1]).unwrap();
    file.sync_all().unwrap();

    let input = crate::PackageArtifactInput::file(&path, Some(expected));

    assert!(matches!(
        input.load_implementation(),
        Err(crate::PackageArtifactLoadError::Validation(
            InterfaceValidationError::ArtifactHashMismatch { .. }
        ))
    ));
}

#[test]
fn native_units_above_sixteen_mib_use_the_bounded_allocation_budget() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let payload: Arc<[u8]> = vec![7_u8; 24 * 1024 * 1024].into();
    let digest = native_digest(&payload);

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [NativeUnit::new(
            digest,
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([]),
            [],
        )],
        [],
    )
    .unwrap();

    let encoded = index.encode().unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &interface,
        &fixture.bundle,
        &encoded,
        &[(digest.bytes(), payload.clone())],
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    assert_eq!(artifact.native_artifact().unwrap(), Some(index));

    assert_eq!(
        artifact.native_unit_bytes(digest.bytes()).unwrap(),
        Some(payload)
    );

    artifact.verify_all().unwrap();

    let error = PackageImplementationArtifact::try_from_bytes(
        artifact.shared_bytes().unwrap(),
        InterfaceValidationLimits::default().with_decoded_allocation(16 * 1024 * 1024),
    );

    assert!(matches!(
        error,
        Err(InterfaceValidationError::ResourceLimitExceeded {
            limit: crate::InterfaceLimit::DecodedAllocation,
            actual: 25_165_824,
            maximum: 16_777_216,
        })
    ));
}

#[test]
fn packed_native_index_above_sixteen_mib_has_its_own_bounded_metadata_budget() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let payload: Arc<[u8]> = Arc::from(b"native unit".as_slice());
    let digest = native_digest(&payload);

    let definitions = (0..100_000u32)
        .map(|number| {
            let name = NonEmptySharedStr::try_new(format!("bray_instance_{number:064x}")).unwrap();

            NativeDefinition::new(
                NativeSymbolContract::new(
                    bray_symbols::NativeSymbolIdentity::Name(name),
                    None,
                    bray_symbols::NativeSymbolBinding::Strong,
                    bray_symbols::NativeSymbolPresence::Required,
                ),
                NativeDefinitionSelection::Ordinary,
            )
        })
        .collect::<Vec<_>>();

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [NativeUnit::new(
            digest,
            NativeUnitKind::Object,
            NativeUnitSummary::Exact {
                definitions: definitions.into(),
                references: [].into(),
                roots: [].into(),
            },
            [],
        )],
        [],
    )
    .unwrap();

    let encoded = index.encode().unwrap();

    assert!(encoded.len() > 16 * 1024 * 1024);

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &interface,
        &fixture.bundle,
        &encoded,
        &[(digest.bytes(), payload)],
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    assert_eq!(artifact.native_artifact().unwrap(), Some(index));

    artifact.verify_all().unwrap();

    let error = PackageImplementationArtifact::try_from_bytes(
        artifact.shared_bytes().unwrap(),
        InterfaceValidationLimits::default().with_decoded_allocation(16 * 1024 * 1024),
    );

    assert!(matches!(
        error,
        Err(InterfaceValidationError::ResourceLimitExceeded {
            limit: crate::InterfaceLimit::DecodedAllocation,
            ..
        })
    ));
}

#[test]
fn native_unit_identity_authentication_is_shared_by_the_payload_cache() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let payload: Arc<[u8]> = Arc::from(b"native unit".as_slice());
    let digest = native_digest(&payload);

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [NativeUnit::new(
            digest,
            NativeUnitKind::Object,
            NativeUnitSummary::opaque([]),
            [],
        )],
        [],
    )
    .unwrap()
    .encode()
    .unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &interface,
        &fixture.bundle,
        &index,
        &[(digest.bytes(), Arc::clone(&payload))],
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    assert_eq!(
        artifact.native_unit_bytes(digest.bytes()).unwrap().unwrap(),
        payload
    );

    let first = artifact.access_statistics();

    assert_eq!(
        artifact
            .clone()
            .native_unit_bytes(digest.bytes())
            .unwrap()
            .unwrap(),
        payload
    );

    assert_eq!(artifact.access_statistics(), first);
}

#[test]
fn demanded_payloads_share_one_allocation_budget_across_parallel_readers() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        &fixture.bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let target = NativeTarget::for_identity(artifact.identity().configuration().target()).unwrap();

    let payloads = (0..3u8)
        .map(|byte| {
            let bytes = Arc::<[u8]>::from(vec![byte; 512 * 1024]);

            (native_digest(&bytes).bytes(), bytes)
        })
        .collect::<Vec<_>>();

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        payloads.iter().map(|(digest, _)| {
            NativeUnit::new(
                NativeContentDigest::new(*digest),
                NativeUnitKind::OpaqueArchive,
                NativeUnitSummary::opaque([]),
                [],
            )
        }),
        [],
    )
    .unwrap()
    .encode()
    .unwrap();

    let packed = artifact
        .try_native_only_artifact(&[(NativeUnitKind::Object, &index)], &payloads)
        .unwrap();

    let bytes = packed.shared_bytes().unwrap();

    for (parallel, verify_before) in [(false, false), (true, false), (false, true), (true, true)] {
        let loaded = PackageImplementationArtifact::try_from_bytes(
            Arc::clone(&bytes),
            InterfaceValidationLimits::default().with_decoded_allocation(800_000),
        )
        .unwrap();

        if verify_before {
            // Full authentication streams all units without reserving their unused decoded bytes.
            loaded.verify_all().unwrap();

            assert_eq!(
                loaded
                    .decoded
                    .iter()
                    .filter(|cell| cell.get().is_some())
                    .count(),
                1
            );
        }

        let results = if parallel {
            let barrier = std::sync::Barrier::new(payloads.len());

            std::thread::scope(|scope| {
                let readers = payloads
                    .iter()
                    .map(|(digest, _)| {
                        let barrier = &barrier;
                        let loaded = &loaded;

                        scope.spawn(move || {
                            barrier.wait();

                            (*digest, loaded.native_unit_bytes(*digest))
                        })
                    })
                    .collect::<Vec<_>>();

                readers
                    .into_iter()
                    .map(|reader| reader.join().unwrap())
                    .collect::<Vec<_>>()
            })
        } else {
            payloads
                .iter()
                .map(|(digest, _)| (*digest, loaded.native_unit_bytes(*digest)))
                .collect()
        };

        assert_eq!(
            results.iter().filter(|(_, result)| result.is_ok()).count(),
            1
        );

        let statistics = loaded.access_statistics();

        for (digest, result) in results {
            match result {
                Ok(Some(payload)) => assert_eq!(payload.len(), 512 * 1024),
                Err(super::super::native::PackageNativeArtifactError::Validation(
                    InterfaceValidationError::ResourceLimitExceeded {
                        limit: crate::InterfaceLimit::DecodedAllocation,
                        maximum: 800_000,
                        ..
                    },
                )) => {}
                other => panic!("unexpected allocation outcome: {other:?}"),
            }

            let _ = loaded.clone().native_unit_bytes(digest);
        }

        assert_eq!(statistics, loaded.access_statistics());

        // Rejected reservations do not consume memory or prevent streaming verification.
        loaded.verify_all().unwrap();
    }
}
