use crate::{
    InterfaceExecutableTemplate, InterfaceValidationError, InterfaceValidationLimits,
    PackageImplementationArtifactBuildError, PackageImplementationConfiguration,
};

use super::super::PackageImplementationArtifact;
use super::super::encoding::encode_artifact;
use super::support::{artifact_fixture, generic_callable_owner};

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

    let identity = super::super::construction::implementation_identity(
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
