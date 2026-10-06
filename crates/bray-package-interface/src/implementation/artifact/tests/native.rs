use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_codegen::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, ReproducibilityLevel,
    RuntimeObservationMode, SizePreference,
};
use bray_native_artifact::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeDefinition, NativeDefinitionSelection,
    NativeIndexError, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_symbols::NativeSymbolContract;
use bray_target::NativeTarget;

use crate::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity, InterfaceNativeBinding,
    InterfaceValidationError, InterfaceValidationLimits, PackageImplementationArtifactBuildError,
    PackageImplementationSpecializationKey, encode_package_interface,
};

use super::super::PackageImplementationArtifact;
use super::support::{artifact_fixture, native_digest};

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
        Err(
            super::super::native::PackageNativeArtifactError::Validation(
                InterfaceValidationError::NativeUnitDigestMismatch { .. }
            )
        )
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
        matches!(opaque.verify_all(), Err(super::super::native::PackageNativeArtifactError::InvalidBinding(owner))
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
        Err(super::super::native::PackageNativeArtifactError::Index(
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
