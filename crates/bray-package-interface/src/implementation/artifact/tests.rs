use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_bound_tree::CheckedTemplateKind;
use bray_codegen::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, ReproducibilityLevel,
    RuntimeObservationMode, SizePreference,
};
use bray_native_artifact::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeRoot,
    NativeIndexError, NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_target::NativeTarget;
use bray_symbols::{
    ExternalSymbolKey, ForeignCallableDirection, ImportedInterfaceId, InterfaceSymbolId,
    NativeSymbolContract, PackageIdentity, SemanticValueStore, SymbolId, SymbolKind,
};

use super::{
    ARTIFACT_HASH_OFFSET, DIRECTORY_ENTRY_LENGTH, ImplementationPayloadKind,
    PackageImplementationArtifact,
};
use crate::implementation::artifact_encoding::encode_artifact;
use crate::implementation::hash::{compute_artifact_hash, compute_payload_hash};
use crate::{
    CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION,
    ImplementationExternalSymbolIdentity, ImplementationSpecializationArgument,
    ImplementationSpecializationArgumentKind, InterfaceCheckedTemplate,
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceLanguageRevision,
    InterfaceNativeBinding, InterfaceNativeBoundary, InterfacePreSpecializedMir, InterfaceValidationError,
    InterfaceValidationLimits, InterfaceValidationPolicy, LoadedInterfaceSurface,
    PackageImplementationArtifactBuildError, PackageImplementationConfiguration,
    PackageImplementationSpecializationKey, PreSpecializedMirDecodeError,
    ValidatedPackageInterface, construct_imported_symbol_skeletons, encode_package_interface,
};

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

    let owner_symbol = fixture.bundle.surface().symbols().symbol(fixture.body.owner())
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
        OptimizationLevel::Full, SizePreference::Size, DebugInformationMode::LineTables,
        ReproducibilityLevel::ByteForByte,
        RuntimeObservationMode::PerformanceInterval {
            inner_iterations: std::num::NonZeroU64::new(3).expect("test interval must be nonzero"),
        },
    );

    let binding = InterfaceNativeBinding::new(
        fixture.body.owner(), key, producer_options, first_digest.bytes(), symbol.clone(),
    );

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
        [],
    );

    let second_unit = NativeUnit::new(
        second_digest,
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Opaque,
        [],
        [],
    );

    let index = NativeArtifactIndex::try_new(target, native_digest(b"producer"),
        [second_unit, first_unit], [])
        .unwrap_or_else(|error| panic!("test native index must validate: {error:?}"));

    let index_bytes = index.encode()
        .unwrap_or_else(|error| panic!("test native index must encode: {error:?}"));

    let payloads = [
        (second_digest.bytes(), Arc::<[u8]>::from(second.as_slice())),
        (first_digest.bytes(), Arc::<[u8]>::from(first.as_slice())),
    ];

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &index_bytes, &payloads, &[binding.clone()],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native package must encode: {error:?}"));

    let imported = PackageImplementationArtifact::try_from_bytes(
        artifact.bytes().to_vec(), InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native package must import: {error:?}"));

    let imported_index = imported.native_artifact()
        .unwrap_or_else(|error| panic!("native units must authenticate: {error:?}"))
        .unwrap_or_else(|| panic!("native index must be present"));

    assert_eq!(imported_index, index);
    assert_eq!(imported.native_bindings().unwrap_or_else(|error| panic!("bindings must decode: {error:?}")), vec![binding.clone()]);
    assert_eq!(imported.native_binding(binding.owner(), binding.key(), producer_options), Ok(Some(binding.clone())));
    assert_eq!(imported.native_binding(binding.owner(), binding.key(), CodegenOptions::default()), Ok(None));
    assert_eq!(imported.native_unit_bytes(first_digest.bytes()), Ok(Some(Arc::from(first.as_slice()))));

    let other_policy = InterfaceNativeBinding::new(
        binding.owner(), binding.key().clone(), CodegenOptions::default(), first_digest.bytes(),
        NonEmptySharedStr::try_new(binding.symbol()).expect("test symbol must be nonempty"),
    );

    let other_artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &index_bytes, &payloads, &[other_policy],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("other producer policy must encode: {error:?}"));

    assert_ne!(artifact.content_hash(), other_artifact.content_hash());

    let wrong = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &index_bytes,
        &[(first_digest.bytes(), Arc::from(b"wrong payload".as_slice())),
          (second_digest.bytes(), Arc::from(second.as_slice()))],
        &[], InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("outer artifact may encode an inconsistent unit: {error:?}"));

    assert!(matches!(wrong.native_artifact(), Err(super::native::PackageNativeArtifactError::Index(NativeIndexError::PayloadDigestMismatch { .. }))));

    let opaque_binding = InterfaceNativeBinding::new(
        fixture.body.owner(), binding.key().clone(), producer_options, second_digest.bytes(),
        NonEmptySharedStr::try_new("bray_test_first")
            .unwrap_or_else(|| panic!("test native name must be valid")),
    );

    let opaque = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &index_bytes, &payloads, &[opaque_binding],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("outer artifact may encode a binding to an opaque unit: {error:?}"));

    assert!(matches!(opaque.native_artifact(), Err(super::native::PackageNativeArtifactError::InvalidBinding(owner))
        if owner == fixture.body.owner()));

    let other_target = if target == NativeTarget::X86_64WindowsMsvc {
        NativeTarget::X86_64LinuxGnu
    } else {
        NativeTarget::X86_64WindowsMsvc
    };

    let stale_index = NativeArtifactIndex::try_new(
        other_target, index.producer(), index.units().iter().cloned(), [],
    )
    .unwrap_or_else(|error| panic!("stale target index must encode: {error:?}"));

    let stale_bytes = stale_index.encode()
        .unwrap_or_else(|error| panic!("stale target bytes must encode: {error:?}"));

    let stale = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &stale_bytes, &payloads, &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("outer artifact may encode a stale target: {error:?}"));

    assert!(matches!(stale.native_artifact(), Err(super::native::PackageNativeArtifactError::Index(NativeIndexError::WrongTarget { .. }))));

    let other_owner = fixture.bundle.surface().symbols().symbols().iter().find(|symbol| symbol.id() != fixture.body.owner())
        .unwrap_or_else(|| panic!("other exported callable must be present"));

    let wrong_key = PackageImplementationSpecializationKey::new(
        ImplementationExternalSymbolIdentity::new(other_owner.key()),
        [], [], fixture.bundle.implementation_configuration().clone(),
        CURRENT_TEMPLATE_SCHEMA_REVISION,
        fixture.bundle.surface().dependencies().iter().cloned(),
    );

    let wrong_binding = InterfaceNativeBinding::new(
        fixture.body.owner(), wrong_key, producer_options, first_digest.bytes(),
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

    let named = |value| NativeSymbolContract::required_name(
        NonEmptySharedStr::try_new(value)
            .unwrap_or_else(|| panic!("test native symbol must be nonempty")),
    );

    let function_unit = NativeUnit::new(function_id, NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([NativeDefinition::new(named("callback"), NativeDefinitionSelection::Ordinary)]),
            references: Arc::from([]), roots: Arc::from([]),
        }, [], []);

    let data_unit = NativeUnit::new(data_id, NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([NativeDefinition::new(named("callback_table"), NativeDefinitionSelection::Ordinary)]),
            references: Arc::from([named("callback")]), roots: Arc::from([]),
        }, [], []);

    let initializer_unit = NativeUnit::new(initializer_id, NativeUnitKind::Bitcode,
        NativeUnitSummary::Exact {
            definitions: Arc::from([]), references: Arc::from([named("callback_table")]),
            roots: Arc::from([NativeRoot::Initialization]),
        }, [], []);

    let group = NativeCoRetentionGroup::try_new([data_id, initializer_id])
        .unwrap_or_else(|| panic!("two units must form a co-retention group"));

    let index = NativeArtifactIndex::try_new(target, native_digest(b"producer"),
        [function_unit, data_unit, initializer_unit], [group.clone()])
        .unwrap_or_else(|error| panic!("native graph must validate: {error:?}"));

    let index_bytes = index.encode()
        .unwrap_or_else(|error| panic!("native graph must encode: {error:?}"));

    let payloads = [
        (function_id.bytes(), Arc::<[u8]>::from(function.as_slice())),
        (data_id.bytes(), Arc::<[u8]>::from(data.as_slice())),
        (initializer_id.bytes(), Arc::<[u8]>::from(initializer.as_slice())),
    ];

    let artifact = PackageImplementationArtifact::try_from_export_bundle_with_native(
        &encoded, &fixture.bundle, &index_bytes, &payloads, &[],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("native graph must publish: {error:?}"));

    let imported = artifact.native_artifact()
        .unwrap_or_else(|error| panic!("native graph must authenticate: {error:?}"))
        .unwrap_or_else(|| panic!("native graph must be present"));

    assert_eq!(imported, index);
    assert_eq!(imported.co_retention_groups(), [group]);

    assert!(matches!(imported.units().iter().find(|unit| unit.digest() == initializer_id)
        .map(NativeUnit::summary), Some(NativeUnitSummary::Exact { roots, .. })
        if roots.as_ref() == [NativeRoot::Initialization]));
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
            artifact.bytes()[..length].to_vec(),
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

    let encoded = encode_artifact(&identity, &[fixture.body.clone(), second], &[], &[], &[], None, &[], &[])
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

    let bytes = encode_artifact(&identity, &[], std::slice::from_ref(&root), &[], &[], None, &[], &[])
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

    let resolver = crate::ImportedInterfaceSymbolResolver::try_new(
        loaded,
        [loaded],
        &skeleton,
        &compiler_known,
    )
    .unwrap_or_else(|error| panic!("test resolver must construct: {error:?}"));

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

    let owner = bundle
        .surface()
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == SymbolKind::Function)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("test interface must export a function"));

    let template = bundle
        .semantics()
        .checked_templates()
        .first()
        .unwrap_or_else(|| panic!("test interface must publish a checked template"));

    let template = InterfaceCheckedTemplate::new(
        CheckedTemplateKind::ConstantCallableBody,
        template.inputs().iter().cloned(),
        template.nodes().iter().cloned(),
        template.temporaries().iter().copied(),
        template.result(),
        template.behavior().clone(),
    );

    ArtifactFixture {
        interface,
        bundle,
        body: InterfaceConstantCallableBody::new(owner, template),
    }
}
