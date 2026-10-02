use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_codegen::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, ReproducibilityLevel,
    RuntimeObservationMode, SizePreference,
};
use bray_native_artifact::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeContentDigest, NativeDefinition,
    NativeDefinitionSelection, NativeIndexError, NativeRoot, NativeUnit, NativeUnitKind,
    NativeUnitSummary,
};
use bray_symbols::{
    ExternalSymbolKey, ForeignCallableDirection, ImportedInterfaceId, InterfaceSymbolId,
    NativeSymbolContract, PackageIdentity, SemanticValueStore, SymbolId,
};
use bray_target::NativeTarget;

use crate::implementation::hash::{compute_artifact_hash, compute_payload_hash};
use crate::{
    CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION,
    ImplementationExternalSymbolIdentity, ImplementationSpecializationArgument,
    ImplementationSpecializationArgumentKind, InterfaceConstantCallableBody,
    InterfaceExecutableTemplate, InterfaceLanguageRevision, InterfaceNativeBinding,
    InterfaceNativeBoundary, InterfacePreSpecializedMir, InterfaceValidationError,
    InterfaceValidationLimits, InterfaceValidationPolicy, LoadedInterfaceSurface,
    PackageImplementationArtifactBuildError, PackageImplementationConfiguration,
    PackageImplementationSpecializationKey, PreSpecializedMirDecodeError,
    ValidatedPackageInterface, construct_imported_symbol_skeletons, encode_package_interface,
};

use super::encoding::encode_artifact;
use super::{
    ARTIFACT_HASH_OFFSET, DIRECTORY_ENTRY_LENGTH, ImplementationPayloadKind,
    PackageImplementationArtifact,
};

#[test]
fn primary_native_selection_preserves_opaque_bitcode_provenance() {
    let fixture = artifact_fixture();
    let encoded = encode_package_interface(&fixture.bundle).unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let payload: Arc<[u8]> = Arc::from(b"native".as_slice());
    let digest = native_digest(&payload);

    for kind in [
        NativeUnitKind::Object,
        NativeUnitKind::Bitcode,
        NativeUnitKind::OpaqueArchive,
    ] {
        for stamped in [false, true] {
            let index = NativeArtifactIndex::try_new(
                target,
                native_digest(b"producer"),
                [NativeUnit::new(
                    digest,
                    kind,
                    NativeUnitSummary::opaque([]),
                    [],
                )],
                [],
            )
            .unwrap();

            let index = if stamped {
                index.with_bitcode_toolchain("llvm-test")
            } else {
                index
            };

            let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
                &encoded,
                &fixture.bundle,
                &index.encode().unwrap(),
                &[(digest.bytes(), Arc::clone(&payload))],
                &[],
                InterfaceValidationLimits::default(),
            )
            .unwrap();

            for requested in [None, Some("llvm-test"), Some("other-llvm")] {
                let expected = if stamped {
                    (requested == Some("llvm-test")).then_some(NativeUnitKind::Bitcode)
                } else {
                    (kind != NativeUnitKind::Bitcode).then_some(NativeUnitKind::Object)
                };

                assert_eq!(
                    artifact
                        .native_representation(requested)
                        .unwrap()
                        .map(|(kind, _)| kind),
                    expected
                );
            }
        }
    }
}

#[test]
fn variant_only_packages_validate_the_complete_native_payload_set() {
    use crate::PackageNativeArtifactError;

    let fixture = artifact_fixture();
    let encoded = encode_package_interface(&fixture.bundle).unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle(
        &encoded,
        &fixture.bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let bytes: Arc<[u8]> = Arc::from(b"bitcode".as_slice());
    let digest = native_digest(&bytes);

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [NativeUnit::new(
            digest,
            NativeUnitKind::Bitcode,
            NativeUnitSummary::opaque([]),
            [],
        )],
        [],
    )
    .unwrap()
    .with_bitcode_toolchain("llvm-test-revision");

    let encoded_index = index.encode().unwrap();
    let variants = [(NativeUnitKind::Bitcode, encoded_index.as_slice())];
    let payloads = [(digest.bytes(), bytes)];

    let native = artifact
        .try_native_only_artifact(&variants, &payloads)
        .unwrap();

    assert_eq!(native.native_artifact(), Ok(None));
    assert_eq!(native.native_variant(NativeUnitKind::Object), Ok(None));

    assert_eq!(native.native_representation(None), Ok(None));
    assert_eq!(native.native_representation(Some("other-llvm")), Ok(None));

    assert_eq!(
        native.native_representation(Some("llvm-test-revision")),
        Ok(Some((NativeUnitKind::Bitcode, index.clone())))
    );

    assert_eq!(
        native.native_variant(NativeUnitKind::Bitcode),
        Ok(Some(index))
    );

    let missing = artifact.try_native_only_artifact(&variants, &[]).unwrap();

    assert_eq!(
        missing.verify_all(),
        Err(PackageNativeArtifactError::MissingUnit(digest))
    );

    let orphan = artifact.try_native_only_artifact(&[], &payloads).unwrap();

    assert_eq!(
        orphan.verify_all(),
        Err(PackageNativeArtifactError::MissingIndex)
    );

    let empty_index = NativeArtifactIndex::try_new(target, native_digest(b"producer"), [], [])
        .unwrap()
        .encode()
        .unwrap();

    let orphan = artifact
        .try_native_only_artifact(&[(NativeUnitKind::Object, &empty_index)], &payloads)
        .unwrap();

    assert_eq!(
        orphan.verify_all(),
        Err(PackageNativeArtifactError::UnindexedUnit(digest.bytes()))
    );
}

#[test]
fn native_representations_in_one_package_must_share_their_producer() {
    let fixture = artifact_fixture();
    let interface = encode_package_interface(&fixture.bundle).unwrap();

    let artifact = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        &fixture.bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let target =
        NativeTarget::for_identity(fixture.bundle.implementation_configuration().target()).unwrap();

    let object = NativeArtifactIndex::try_new(target, native_digest(b"first"), [], [])
        .unwrap()
        .encode()
        .unwrap();

    let bitcode = NativeArtifactIndex::try_new(target, native_digest(b"second"), [], [])
        .unwrap()
        .encode()
        .unwrap();

    let mismatched = artifact
        .try_native_only_artifact(
            &[
                (NativeUnitKind::Object, object.as_slice()),
                (NativeUnitKind::Bitcode, bitcode.as_slice()),
            ],
            &[],
        )
        .unwrap();

    assert_eq!(
        mismatched.verify_all(),
        Err(crate::PackageNativeArtifactError::Index(
            NativeIndexError::WrongProducer {
                expected: native_digest(b"first"),
                actual: native_digest(b"second")
            },
        ))
    );
}

#[test]
fn native_package_units_round_trip_with_exact_source_binding() {
    let fixture = artifact_fixture();

    let encoded = encode_package_interface(&fixture.bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    let target = NativeTarget::for_identity(fixture.bundle.implementation_configuration().target())
        .unwrap_or_else(|| panic!("test package target must be supported"));

    let first = b"independent first bitcode";
    let second = b"independent second bitcode";
    let first_digest = native_digest(first);
    let second_digest = native_digest(second);

    let owner_symbol = fixture
        .bundle
        .surface()
        .symbols()
        .symbol(fixture.body.owner())
        .unwrap_or_else(|| panic!("test callable must be exported"));

    let symbol = NonEmptySharedStr::try_new("bray_test_first")
        .unwrap_or_else(|| panic!("test native name must be valid"));

    let key = PackageImplementationSpecializationKey::new(
        ImplementationExternalSymbolIdentity::new(owner_symbol.key()),
        [],
        [],
        fixture.bundle.implementation_configuration().clone(),
        CURRENT_TEMPLATE_SCHEMA_REVISION,
        fixture.bundle.surface().dependencies().iter().cloned(),
    );

    let producer_options = CodegenOptions::new(
        OptimizationLevel::Full,
        SizePreference::Size,
        DebugInformationMode::LineTables,
        ReproducibilityLevel::ByteForByte,
        RuntimeObservationMode::PerformanceInterval {
            inner_iterations: std::num::NonZeroU64::new(3).expect("test interval must be nonzero"),
        },
    );

    let binding = InterfaceNativeBinding::new(
        fixture.body.owner(),
        key,
        producer_options,
        first_digest.bytes(),
        symbol.clone(),
    )
    .with_main_thread_requirement(true);

    assert!(binding.requires_main_thread());

    let first_unit = NativeUnit::new(
        first_digest,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([NativeDefinition::new(
                NativeSymbolContract::required_name(symbol),
                NativeDefinitionSelection::Ordinary,
            )]),
            references: Arc::from([]),
            roots: Arc::from([]),
        },
        [],
    );

    let second_unit = NativeUnit::new(
        second_digest,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::opaque([]),
        [],
    );

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [second_unit, first_unit],
        [],
    )
    .unwrap_or_else(|error| panic!("test native index must validate: {error:?}"));

    let index_bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("test native index must encode: {error:?}"));

    let payloads = [
        (second_digest.bytes(), Arc::<[u8]>::from(second.as_slice())),
        (first_digest.bytes(), Arc::<[u8]>::from(first.as_slice())),
    ];

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &index_bytes,
        &payloads,
        &[binding.clone()],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native package must encode: {error:?}"));

    let imported = PackageImplementationArtifact::try_from_bytes(
        artifact.shared_bytes().unwrap().to_vec(),
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native package must import: {error:?}"));

    let imported_index = imported
        .native_artifact()
        .unwrap_or_else(|error| panic!("native units must authenticate: {error:?}"))
        .unwrap_or_else(|| panic!("native index must be present"));

    assert_eq!(imported_index, index);

    let cloned = imported.clone();

    let reused = cloned
        .native_artifact()
        .expect("cloned index must authenticate")
        .expect("cloned artifact must have a native index");

    assert!(std::ptr::eq(imported_index.units(), reused.units()));

    assert_eq!(
        imported
            .native_bindings()
            .unwrap_or_else(|error| panic!("bindings must decode: {error:?}")),
        vec![binding.clone()]
    );

    assert_eq!(
        imported.native_binding(binding.owner(), binding.key(), producer_options),
        Ok(Some(binding.clone()))
    );

    assert_eq!(
        imported.native_binding(binding.owner(), binding.key(), CodegenOptions::default()),
        Ok(None)
    );

    assert_eq!(
        imported.native_unit_bytes(first_digest.bytes()).unwrap(),
        Some(Arc::from(first.as_slice()))
    );

    let other_policy = InterfaceNativeBinding::new(
        binding.owner(),
        binding.key().clone(),
        CodegenOptions::default(),
        first_digest.bytes(),
        NonEmptySharedStr::try_new(binding.symbol()).expect("test symbol must be nonempty"),
    );

    let other_artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &index_bytes,
        &payloads,
        &[other_policy],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("other producer policy must encode: {error:?}"));

    assert_ne!(artifact.content_hash(), other_artifact.content_hash());

    let wrong = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &index_bytes,
        &[
            (first_digest.bytes(), Arc::from(b"wrong payload".as_slice())),
            (second_digest.bytes(), Arc::from(second.as_slice())),
        ],
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("outer artifact may encode an inconsistent unit: {error:?}"));

    assert!(wrong.native_artifact().is_ok());

    assert!(matches!(
        wrong.native_unit_bytes(first_digest.bytes()),
        Err(super::native::PackageNativeArtifactError::Validation(
            InterfaceValidationError::NativeUnitDigestMismatch { .. }
        ))
    ));

    let opaque_binding = InterfaceNativeBinding::new(
        fixture.body.owner(),
        binding.key().clone(),
        producer_options,
        second_digest.bytes(),
        NonEmptySharedStr::try_new("bray_test_first")
            .unwrap_or_else(|| panic!("test native name must be valid")),
    );

    let opaque = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &index_bytes,
        &payloads,
        &[opaque_binding.clone()],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| {
        panic!("outer artifact may encode a binding to an opaque unit: {error:?}")
    });

    assert!(
        matches!(opaque.verify_all(), Err(super::native::PackageNativeArtifactError::InvalidBinding(owner))
        if owner == fixture.body.owner())
    );

    let archive_index = NativeArtifactIndex::try_new(
        target,
        index.producer(),
        index.units().iter().map(|unit| {
            if unit.digest() == second_digest {
                NativeUnit::new(
                    second_digest,
                    NativeUnitKind::OpaqueArchive,
                    NativeUnitSummary::opaque([]),
                    [],
                )
            } else {
                unit.clone()
            }
        }),
        [],
    )
    .expect("opaque archive index must validate");

    let archive_index = archive_index
        .encode()
        .expect("opaque archive index must encode");

    let archive = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &archive_index,
        &payloads,
        &[opaque_binding],
        InterfaceValidationLimits::default(),
    )
    .expect("opaque archive binding must publish");

    assert!(
        archive.native_artifact().is_ok(),
        "opaque archives retain authenticated producer bindings"
    );

    let other_target = if target == NativeTarget::X86_64WindowsMsvc {
        NativeTarget::X86_64LinuxGnu
    } else {
        NativeTarget::X86_64WindowsMsvc
    };

    let stale_index = NativeArtifactIndex::try_new(
        other_target,
        index.producer(),
        index.units().iter().cloned(),
        [],
    )
    .unwrap_or_else(|error| panic!("stale target index must encode: {error:?}"));

    let stale_bytes = stale_index
        .encode()
        .unwrap_or_else(|error| panic!("stale target bytes must encode: {error:?}"));

    let stale = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &stale_bytes,
        &payloads,
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("outer artifact may encode a stale target: {error:?}"));

    assert!(matches!(
        stale.native_artifact(),
        Err(super::native::PackageNativeArtifactError::Index(
            NativeIndexError::WrongTarget { .. }
        ))
    ));

    let other_owner = fixture
        .bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.id() != fixture.body.owner())
        .unwrap_or_else(|| panic!("other exported callable must be present"));

    let wrong_key = PackageImplementationSpecializationKey::new(
        ImplementationExternalSymbolIdentity::new(other_owner.key()),
        [],
        [],
        fixture.bundle.implementation_configuration().clone(),
        CURRENT_TEMPLATE_SCHEMA_REVISION,
        fixture.bundle.surface().dependencies().iter().cloned(),
    );

    let wrong_binding = InterfaceNativeBinding::new(
        fixture.body.owner(),
        wrong_key,
        producer_options,
        first_digest.bytes(),
        NonEmptySharedStr::try_new("bray_test_first")
            .unwrap_or_else(|| panic!("test native name must be valid")),
    );

    assert!(matches!(
        PackageImplementationArtifact::try_from_export_bundle_with_native(
            &encoded, &fixture.bundle, &index_bytes, &payloads, &[wrong_binding],
            InterfaceValidationLimits::default(),
        ),
        Err(PackageImplementationArtifactBuildError::InvalidNativeBinding(owner))
            if owner == fixture.body.owner()
    ));
}

#[test]
fn native_package_preserves_function_address_data_initialization_and_group() {
    let fixture = artifact_fixture();

    let encoded = encode_package_interface(&fixture.bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    let target = NativeTarget::for_identity(fixture.bundle.implementation_configuration().target())
        .unwrap_or_else(|| panic!("test package target must be supported"));

    let function = b"function bitcode";
    let data = b"function pointer data";
    let initializer = b"initializer bitcode";
    let function_id = native_digest(function);
    let data_id = native_digest(data);
    let initializer_id = native_digest(initializer);

    let named = |value| {
        NativeSymbolContract::required_name(
            NonEmptySharedStr::try_new(value)
                .unwrap_or_else(|| panic!("test native symbol must be nonempty")),
        )
    };

    let function_unit = NativeUnit::new(
        function_id,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([NativeDefinition::new(
                named("callback"),
                NativeDefinitionSelection::Ordinary,
            )]),
            references: Arc::from([]),
            roots: Arc::from([]),
        },
        [],
    );

    let data_unit = NativeUnit::new(
        data_id,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([NativeDefinition::new(
                named("callback_table"),
                NativeDefinitionSelection::Ordinary,
            )]),
            references: Arc::from([named("callback")]),
            roots: Arc::from([]),
        },
        [],
    );

    let initializer_unit = NativeUnit::new(
        initializer_id,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([]),
            references: Arc::from([named("callback_table")]),
            roots: Arc::from([NativeRoot::Initialization]),
        },
        [],
    );

    let group = NativeCoRetentionGroup::try_new([data_id, initializer_id])
        .unwrap_or_else(|| panic!("two units must form a co-retention group"));

    let index = NativeArtifactIndex::try_new(
        target,
        native_digest(b"producer"),
        [function_unit, data_unit, initializer_unit],
        [group.clone()],
    )
    .unwrap_or_else(|error| panic!("native graph must validate: {error:?}"));

    let index_bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("native graph must encode: {error:?}"));

    let payloads = [
        (function_id.bytes(), Arc::<[u8]>::from(function.as_slice())),
        (data_id.bytes(), Arc::<[u8]>::from(data.as_slice())),
        (
            initializer_id.bytes(),
            Arc::<[u8]>::from(initializer.as_slice()),
        ),
    ];

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded,
        &fixture.bundle,
        &index_bytes,
        &payloads,
        &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native graph must publish: {error:?}"));

    let imported = artifact
        .native_artifact()
        .unwrap_or_else(|error| panic!("native graph must authenticate: {error:?}"))
        .unwrap_or_else(|| panic!("native graph must be present"));

    assert_eq!(imported, index);
    assert_eq!(imported.co_retention_groups(), [group]);

    assert!(
        matches!(imported.units().iter().find(|unit| unit.digest() == initializer_id)
        .map(NativeUnit::summary), Some(NativeUnitSummary::Exact { roots, .. })
        if roots.as_ref() == [NativeRoot::Initialization])
    );
}

fn native_digest(bytes: &[u8]) -> NativeContentDigest {
    NativeContentDigest::new(
        bray_base::sha256_reader(bytes)
            .unwrap_or_else(|error| panic!("test bytes must hash: {error}")),
    )
}

#[test]
fn artifacts_decode_only_the_requested_constant_body() {
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
    .unwrap_or_else(|error| panic!("constant body artifact must validate: {error:?}"));

    assert_eq!(
        artifact.interface_content_hash(),
        fixture.interface.header().content_hash()
    );

    assert_eq!(
        artifact.language_revision(),
        fixture.interface.header().language_revision()
    );

    let body = artifact
        .constant_callable_body(fixture.body.owner(), fixture.bundle.surface())
        .unwrap_or_else(|error| panic!("requested body must decode: {error:?}"));

    assert_eq!(body, Some(fixture.body));
}

#[test]
fn truncated_headers_identify_the_exact_field() {
    let fixture = artifact_fixture();

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [fixture.body],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("implementation artifact must validate: {error:?}"));

    let cases = [
        (7, crate::InterfaceValidationField::Magic),
        (9, crate::InterfaceValidationField::FormatRevision),
        (11, crate::InterfaceValidationField::LanguageRevision),
        (15, crate::InterfaceValidationField::ByteOrderMarker),
        (23, crate::InterfaceValidationField::RequiredFlags),
        (31, crate::InterfaceValidationField::DeclaredFileLength),
        (39, crate::InterfaceValidationField::DirectoryOffset),
        (47, crate::InterfaceValidationField::DirectoryLength),
        (79, crate::InterfaceValidationField::ContentHash),
        (111, crate::InterfaceValidationField::ArtifactHash),
    ];

    for (length, expected_field) in cases {
        let error = PackageImplementationArtifact::try_from_bytes(
            artifact.shared_bytes().unwrap()[..length].to_vec(),
            InterfaceValidationLimits::default(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            InterfaceValidationError::Truncated {
                context: crate::InterfaceValidationContext::Header,
                field,
                ..
            } if field == expected_field
        ));
    }
}

#[test]
fn malformed_unrequested_payloads_do_not_block_other_body_lookups() {
    let fixture = artifact_fixture();

    let second_owner = InterfaceSymbolId::new(fixture.body.owner().raw().saturating_add(100));

    let second = InterfaceConstantCallableBody::new(second_owner, fixture.body.template().clone());

    let identity = super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let encoded = encode_artifact(
        &identity,
        &[fixture.body.clone(), second],
        &[],
        &[],
        &[],
        None,
        &[],
        &[],
    )
    .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"));

    let pristine = PackageImplementationArtifact::try_from_bytes(
        Arc::clone(&encoded),
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("test artifact must validate: {error:?}"));

    let entry = pristine
        .directory
        .iter()
        .find(|entry| entry.owner == second_owner)
        .unwrap_or_else(|| panic!("second body must have a directory entry"));

    let mut bytes = encoded.to_vec();

    let payload = bytes
        .get_mut(entry.payload.clone())
        .unwrap_or_else(|| panic!("second body payload must be in bounds"));

    payload[0] ^= 0xff;

    let checksum = compute_payload_hash(entry, payload);

    let directory_offset = u64::from_le_bytes(
        bytes[32..40]
            .try_into()
            .unwrap_or_else(|_| panic!("directory offset must be encoded")),
    );

    let directory_offset = usize::try_from(directory_offset)
        .unwrap_or_else(|_| panic!("directory offset must fit the test target"));

    let entry_index = pristine
        .directory
        .iter()
        .position(|candidate| candidate.owner == second_owner)
        .unwrap_or_else(|| panic!("second body entry must remain addressable"));

    let checksum_offset = directory_offset + entry_index * DIRECTORY_ENTRY_LENGTH + 84;

    bytes[checksum_offset..checksum_offset + 32].copy_from_slice(&checksum);

    let artifact_hash = compute_artifact_hash(&bytes)
        .unwrap_or_else(|| panic!("mutated artifact must remain hashable"));

    bytes[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32].copy_from_slice(&artifact_hash);

    let artifact =
        PackageImplementationArtifact::try_from_bytes(bytes, InterfaceValidationLimits::default())
            .unwrap_or_else(|error| panic!("directory validation must remain lazy: {error:?}"));

    let first = artifact.constant_callable_body(fixture.body.owner(), fixture.bundle.surface());

    assert_eq!(first.map(|body| body.is_some()), Ok(true));

    assert!(
        artifact
            .constant_callable_body(second_owner, fixture.bundle.surface(),)
            .is_err()
    );
}

#[test]
fn artifacts_reject_duplicate_constant_body_owners() {
    let fixture = artifact_fixture();

    let result = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [fixture.body.clone(), fixture.body.clone()],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    );

    assert_eq!(
        result,
        Err(PackageImplementationArtifactBuildError::DuplicateCallableBody(fixture.body.owner()))
    );
}

#[test]
fn artifacts_load_executable_templates_independently() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let template = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::ROOT,
        1,
        [1_u8, 2, 3],
    )
    .map(|template| {
        template.with_platform_service(Some(
            bray_runtime_interface::PlatformServiceRole::StandardOutputFlush,
        ))
    })
    .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [template.clone()],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("executable template artifact must validate: {error:?}"));

    assert_eq!(
        artifact.executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT),
        Ok(Some(template))
    );
}

#[test]
fn artifacts_reject_platform_services_on_nested_executable_templates() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let root = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::ROOT,
        2,
        [1_u8, 2, 3],
    )
    .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let nested = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::new(1),
        2,
        [4_u8, 5, 6],
    )
    .map(|template| {
        template.with_platform_service(Some(
            bray_runtime_interface::PlatformServiceRole::StandardOutputFlush,
        ))
    })
    .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let result = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [root, nested],
        [],
        [],
        InterfaceValidationLimits::default(),
    );

    assert_eq!(
        result,
        Err(PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner))
    );
}

#[test]
fn artifacts_reject_incomplete_executable_template_families() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let root = InterfaceExecutableTemplate::new(
        owner,
        bray_ir::MirExecutableTemplateId::ROOT,
        2,
        [1_u8, 2, 3],
    )
    .unwrap_or_else(|| panic!("non-empty executable payload must be valid"));

    let identity = super::construction::implementation_identity(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
    );

    let bytes = encode_artifact(
        &identity,
        &[],
        std::slice::from_ref(&root),
        &[],
        &[],
        None,
        &[],
        &[],
    )
    .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"));

    let result =
        PackageImplementationArtifact::try_from_bytes(bytes, InterfaceValidationLimits::default());

    assert_eq!(
        result,
        Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Value
        ))
    );
}

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
fn artifacts_load_native_boundaries_by_declaration_owner() {
    let fixture = artifact_fixture();
    let owner = generic_callable_owner(&fixture.bundle);

    let symbol = NonEmptySharedStr::try_new("native_operation")
        .unwrap_or_else(|| panic!("test symbol must be nonempty"));

    let boundary = InterfaceNativeBoundary::new(
        owner,
        ForeignCallableDirection::Import,
        crate::InterfaceNativeBoundaryKind::Callable,
        NativeSymbolContract::required_name(symbol),
    );

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [],
        [boundary.clone()],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native boundary artifact must validate: {error:?}"));

    assert_eq!(artifact.native_boundary(owner), Ok(Some(boundary)));
}

#[test]
fn artifacts_publish_complete_inspectable_identity_and_reject_configuration_mismatch() {
    let fixture = artifact_fixture();

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("identity artifact must validate: {error:?}"));

    let mut expected_runtime_requirements = fixture
        .bundle
        .semantics()
        .runtime_requirements()
        .iter()
        .map(crate::InterfaceRuntimeRequirement::requirements)
        .cloned()
        .collect::<Vec<_>>();

    expected_runtime_requirements.sort_unstable();
    expected_runtime_requirements.dedup();

    assert_eq!(
        artifact.identity().interface(),
        fixture.bundle.surface().identity()
    );

    assert_eq!(
        artifact.identity().dependencies(),
        fixture.bundle.surface().dependencies()
    );

    assert_eq!(
        artifact.identity().runtime_requirements(),
        expected_runtime_requirements
    );

    assert_eq!(
        artifact.identity().configuration(),
        fixture.bundle.implementation_configuration()
    );

    let selected_runtime = bray_runtime_interface::RuntimeIdentity::try_new("bray.runtime.test")
        .unwrap_or_else(|| panic!("test runtime identity must be valid"));

    let mismatched = PackageImplementationConfiguration::new(
        fixture
            .bundle
            .implementation_configuration()
            .target_properties()
            .clone(),
        Some(selected_runtime),
        fixture.bundle.implementation_configuration().runtime_abi(),
        fixture
            .bundle
            .implementation_configuration()
            .panic_abi()
            .clone(),
    );

    assert!(matches!(
        artifact.validate_configuration(&mismatched),
        Err(InterfaceValidationError::ImplementationConfigurationMismatch { .. })
    ));

    let selected_runtime_artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        mismatched.clone(),
        [],
        [],
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("selected runtime identity must round trip: {error:?}"));

    assert_eq!(
        selected_runtime_artifact.identity().configuration(),
        &mismatched
    );

    assert!(matches!(
        selected_runtime_artifact
            .validate_configuration(fixture.bundle.implementation_configuration()),
        Err(InterfaceValidationError::ImplementationConfigurationMismatch { .. })
    ));

    let alternate_target = bray_ir::MirTargetContract::new(
        bray_target::NativeTarget::X86_64WindowsMsvc.profile(),
        fixture.bundle.implementation_configuration().runtime_abi(),
    );

    let alternate_target = PackageImplementationConfiguration::for_mir_target(
        &alternate_target,
        None,
        fixture
            .bundle
            .implementation_configuration()
            .panic_abi()
            .clone(),
    );

    assert_ne!(
        alternate_target.target_properties().machine(),
        artifact
            .identity()
            .configuration()
            .target_properties()
            .machine()
    );

    assert_ne!(
        alternate_target.target_properties().properties(),
        artifact
            .identity()
            .configuration()
            .target_properties()
            .properties()
    );

    assert!(matches!(
        artifact.validate_configuration(&alternate_target),
        Err(InterfaceValidationError::ImplementationConfigurationMismatch { .. })
    ));
}

#[test]
fn pre_specialized_mir_uses_the_complete_specialization_identity() {
    let fixture = artifact_fixture();
    let key = specialization_key(&fixture, [1; 32]);

    let mir =
        InterfacePreSpecializedMir::new(key.clone(), CURRENT_MIR_SCHEMA_REVISION, [3_u8, 5, 8])
            .unwrap_or_else(|| panic!("nonempty pre-specialized MIR must be valid"));

    let artifact = PackageImplementationArtifact::try_new(
        &fixture.interface,
        fixture.bundle.surface(),
        fixture.bundle.semantics(),
        fixture.bundle.implementation_configuration().clone(),
        [],
        [],
        [],
        [mir.clone()],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("pre-specialized MIR artifact must validate: {error:?}"));

    let entry = artifact
        .directory
        .iter()
        .find(|entry| entry.kind == Some(ImplementationPayloadKind::PreSpecializedMir))
        .unwrap_or_else(|| panic!("pre-specialized MIR must be addressable"));

    assert_eq!(entry.discriminator, key.cache_identity());

    let different_substitution = specialization_key(&fixture, [2; 32]);

    assert_ne!(
        key.cache_identity(),
        different_substitution.cache_identity()
    );

    assert_ne!(entry.discriminator, different_substitution.cache_identity());

    let loaded = LoadedInterfaceSurface::new(
        ImportedInterfaceId::new(0),
        fixture.interface.header().content_hash(),
        fixture.bundle.surface(),
    );

    let skeleton = construct_imported_symbol_skeletons(SymbolId::new(0), [loaded])
        .unwrap_or_else(|error| panic!("test interface symbols must import: {error:?}"));

    let compiler_known = BTreeMap::new();

    let resolver =
        crate::ImportedInterfaceSymbolResolver::new(loaded, [loaded], &skeleton, &compiler_known);

    let store = SemanticValueStore::try_new()
        .unwrap_or_else(|error| panic!("test semantic store must construct: {error:?}"));

    let properties = fixture
        .bundle
        .semantics()
        .intern(&store, &resolver)
        .unwrap_or_else(|error| panic!("test imported semantics must load: {error:?}"));

    let owner_key = fixture
        .bundle
        .surface()
        .symbols()
        .symbol(generic_callable_owner(&fixture.bundle))
        .map(bray_symbols::ImportedSymbolIdentity::key)
        .unwrap_or_else(|| panic!("test callable identity must be present"));

    let owner = skeleton
        .symbol_by_external_key(owner_key)
        .unwrap_or_else(|| panic!("test callable must import"));

    let target = bray_ir::MirTargetContract::new(
        bray_target::NativeTarget::X86_64LinuxGnu.profile(),
        fixture.bundle.implementation_configuration().runtime_abi(),
    );

    assert!(matches!(
        artifact.pre_specialized_mir(
            &key,
            owner,
            bray_ir::MirUnitId::new(0),
            target,
            &properties,
            &resolver,
        ),
        Err(PreSpecializedMirDecodeError::Executable(
            crate::ExecutableTemplateDecodeError::Validation(
                InterfaceValidationError::Truncated { .. }
            ),
        ))
    ));
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

struct ArtifactFixture {
    interface: ValidatedPackageInterface,
    bundle: crate::PackageInterfaceExportBundle,
    body: InterfaceConstantCallableBody,
}

fn generic_callable_owner(bundle: &crate::PackageInterfaceExportBundle) -> InterfaceSymbolId {
    bundle
        .semantics()
        .generic_declarations()
        .iter()
        .find_map(|declaration| match declaration.owner() {
            crate::InterfaceSymbolReference::Local(owner)
                if bundle
                    .surface()
                    .symbols()
                    .symbol(*owner)
                    .is_some_and(|symbol| symbol.kind().is_callable()) =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("test interface must export a generic callable"))
}

fn specialization_key(
    fixture: &ArtifactFixture,
    argument: [u8; 32],
) -> PackageImplementationSpecializationKey {
    let package = PackageIdentity::try_new("example.specialization")
        .unwrap_or_else(|| panic!("specialization package identity must be valid"));

    PackageImplementationSpecializationKey::new(
        ImplementationExternalSymbolIdentity::new(&ExternalSymbolKey::package(package)),
        [ImplementationSpecializationArgument::new(
            ImplementationSpecializationArgumentKind::Type,
            argument,
        )],
        [],
        fixture.bundle.implementation_configuration().clone(),
        CURRENT_TEMPLATE_SCHEMA_REVISION,
        fixture.bundle.surface().dependencies().iter().cloned(),
    )
}

fn artifact_fixture() -> ArtifactFixture {
    let bundle = crate::test_support::package_interface_export_bundle();

    let encoded = encode_package_interface(&bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    let interface = ValidatedPackageInterface::try_new(
        encoded.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"));

    let body = crate::test_support::constant_callable_body(&bundle);

    ArtifactFixture {
        interface,
        bundle,
        body,
    }
}

#[test]
fn packed_storage_above_interface_ceiling_keeps_metadata_and_payload_bounds() {
    use std::io::{Seek, SeekFrom, Write};

    use crate::implementation::hash::{compute_metadata_hash, compute_payload_content_hash};
    use crate::{InterfaceLimit, InterfaceSectionCompatibility, InterfaceSectionEncoding};

    let fixture = artifact_fixture();

    let identity = super::construction::implementation_identity(
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

    let original_offset = usize::try_from(u64::from_le_bytes(bytes[32..40].try_into().unwrap()))
        .unwrap();

    let mut header = bytes[..super::HEADER_LENGTH].to_vec();
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
    file.write_all(&bytes[super::HEADER_LENGTH..original_offset]).unwrap();
    file.seek(SeekFrom::Start(offset as u64)).unwrap();
    file.write_all(&directory_bytes).unwrap();
    file.sync_all().unwrap();

    let limits = InterfaceValidationLimits::default();

    assert!(file_length as u64 > limits.maximum(InterfaceLimit::FileSize));

    let loaded = PackageImplementationArtifact::try_open(
        &path,
        limits.with_decoded_allocation(128 * 1024),
    )
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
        (limits.with_decoded_allocation(1), InterfaceLimit::DecodedAllocation),
        (limits.with_blob_length(payload.len() as u64 - 1), InterfaceLimit::BlobLength),
        (limits.with_implementation_entry_count(17), InterfaceLimit::ImplementationEntryCount),
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
    digest.update(&bytes[super::HEADER_LENGTH..original_offset]);

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
    assert_eq!(loaded.access_statistics().payload_bytes_hashed, file_length as u64 + artifact.access_statistics().payload_bytes_hashed);

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
fn packed_file_reads_only_metadata_and_demanded_bodies_and_shares_parallel_cache() {
    let fixture = artifact_fixture();

    let identity = super::construction::implementation_identity(
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
fn unread_corruption_is_rejected_on_selection_and_complete_verification() {
    let fixture = artifact_fixture();

    let identity = super::construction::implementation_identity(
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
        .set_len(super::HEADER_LENGTH as u64)
        .unwrap();

    assert!(
        matches!(selected.constant_callable_body(fixture.body.owner(), fixture.bundle.surface()),
        Err(InterfaceValidationError::Read { path: actual, kind: std::io::ErrorKind::UnexpectedEof }) if actual == path)
    );
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
                Err(super::native::PackageNativeArtifactError::Validation(
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
