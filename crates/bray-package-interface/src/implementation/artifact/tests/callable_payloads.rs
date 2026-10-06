use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{
    ExternalSymbolKey, ForeignCallableDirection, ImportedInterfaceId, InterfaceSymbolId,
    NativeSymbolContract, PackageIdentity, SemanticValueStore, SymbolId,
};

use crate::implementation::hash::{compute_artifact_hash, compute_payload_hash};
use crate::{
    CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION,
    ImplementationExternalSymbolIdentity, ImplementationSpecializationArgument,
    ImplementationSpecializationArgumentKind, InterfaceConstantCallableBody,
    InterfaceExecutableTemplate, InterfaceNativeBoundary, InterfacePreSpecializedMir,
    InterfaceValidationError, InterfaceValidationLimits, LoadedInterfaceSurface,
    PackageImplementationSpecializationKey, PreSpecializedMirDecodeError,
    construct_imported_symbol_skeletons,
};

use super::super::encoding::encode_artifact;
use super::super::{
    ARTIFACT_HASH_OFFSET, DIRECTORY_ENTRY_LENGTH, ImplementationPayloadKind,
    PackageImplementationArtifact,
};
use super::support::{ArtifactFixture, artifact_fixture, generic_callable_owner};

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
fn malformed_unrequested_payloads_do_not_block_other_body_lookups() {
    let fixture = artifact_fixture();

    let second_owner = InterfaceSymbolId::new(fixture.body.owner().raw().saturating_add(100));

    let second = InterfaceConstantCallableBody::new(second_owner, fixture.body.template().clone());

    let identity = super::super::construction::implementation_identity(
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

    let consumer_owner = bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(7)).into();
    let before = artifact.access_statistics();

    assert_eq!(
        artifact.executable_template_key(
            owner,
            consumer_owner,
            bray_ir::MirExecutableTemplateId::ROOT
        ),
        Some(
            bray_ir::MirImportedExecutableKey::new(
                consumer_owner,
                bray_ir::MirExecutableTemplateId::ROOT
            )
            .with_platform_service(Some(
                bray_runtime_interface::PlatformServiceRole::StandardOutputFlush
            ))
        )
    );

    assert_eq!(
        artifact.executable_template_key(
            owner,
            consumer_owner,
            bray_ir::MirExecutableTemplateId::new(1)
        ),
        None
    );

    assert_eq!(artifact.access_statistics(), before);

    assert_eq!(
        artifact.executable_template(owner, bray_ir::MirExecutableTemplateId::ROOT),
        Ok(Some(template))
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
