use std::collections::BTreeSet;
use std::io::ErrorKind;

use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use super::{
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticArtifactDigest,
    DiagnosticArtifactDigestAlgorithm, DiagnosticIoErrorKind, DiagnosticLoweringFailure,
    DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
    DiagnosticLoweringInputFailureKind, DiagnosticNameKind, DiagnosticNativeLinkInputFailure,
    DiagnosticNativeProductFailureKind,
};

#[test]
fn diagnostic_args_pair_stable_names_with_typed_values() {
    let arg = DiagnosticArg::new(
        DiagnosticArgName::Character,
        DiagnosticArgValue::Character('\u{0}'),
    );

    assert_eq!(arg.name(), DiagnosticArgName::Character);
    assert_eq!(arg.name().as_str(), "character");
    assert_eq!(arg.value(), &DiagnosticArgValue::Character('\u{0}'));
}

#[test]
fn syntax_args_keep_syntax_kind_values_typed() {
    let arg = DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword);

    assert_eq!(arg.name(), DiagnosticArgName::ExpectedSyntaxKind);
    assert_eq!(arg.name().as_str(), "expected_syntax_kind");

    assert_eq!(
        arg.value(),
        &DiagnosticArgValue::SyntaxKind(SyntaxKind::FuncKeyword)
    );
}

#[test]
fn artifact_mismatch_args_keep_expected_and_actual_values_typed() {
    let digest =
        DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, [0_u8; 32]);

    let expected_digest = DiagnosticArg::expected_artifact_digest(digest.clone());
    let actual_length = DiagnosticArg::actual_byte_count(4);

    assert_eq!(
        expected_digest.name(),
        DiagnosticArgName::ExpectedArtifactDigest
    );

    assert_eq!(
        expected_digest.value(),
        &DiagnosticArgValue::ArtifactDigest(digest)
    );

    assert_eq!(actual_length.name(), DiagnosticArgName::ActualByteCount);
    assert_eq!(actual_length.value(), &DiagnosticArgValue::ByteCount(4));
}

#[test]
fn declaration_args_keep_names_and_module_surface_values_typed() {
    let name = DiagnosticArg::declaration_name("Point");
    let named_member = DiagnosticArg::trait_member_name("get");
    let lifecycle_member = DiagnosticArg::trait_member_kind(SyntaxKind::EnterKeyword);
    let visibility = DiagnosticArg::expected_visibility(super::DiagnosticVisibility::Public);
    let trust = DiagnosticArg::actual_module_trust(super::DiagnosticModuleTrust::Ordinary);

    assert_eq!(name.name(), DiagnosticArgName::DeclarationName);
    assert_eq!(name.name().as_str(), "declaration_name");

    assert_eq!(
        name.value(),
        &DiagnosticArgValue::DeclarationName(String::from("Point"))
    );

    assert_eq!(named_member.name(), DiagnosticArgName::TraitMemberName);
    assert_eq!(named_member.name().as_str(), "trait_member_name");

    assert_eq!(
        named_member.value(),
        &DiagnosticArgValue::DeclarationName(String::from("get"))
    );

    assert_eq!(
        lifecycle_member.value(),
        &DiagnosticArgValue::SyntaxKind(SyntaxKind::EnterKeyword)
    );

    assert_eq!(
        visibility.value(),
        &DiagnosticArgValue::Visibility(super::DiagnosticVisibility::Public)
    );

    assert_eq!(
        trust.value(),
        &DiagnosticArgValue::ModuleTrust(super::DiagnosticModuleTrust::Ordinary)
    );

    assert_eq!(super::DiagnosticVisibility::Internal.as_str(), "internal");
    assert_eq!(super::DiagnosticModuleTrust::Trusted.as_str(), "trusted");
}

#[test]
fn binding_args_keep_reference_names_and_expected_categories_typed() {
    let name = DiagnosticArg::referenced_name("Point");
    let kind = DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type);

    assert_eq!(name.name(), DiagnosticArgName::ReferencedName);
    assert_eq!(name.name().as_str(), "referenced_name");

    assert_eq!(
        name.value(),
        &DiagnosticArgValue::ReferencedName(String::from("Point"))
    );

    assert_eq!(kind.name(), DiagnosticArgName::ExpectedNameKind);
    assert_eq!(kind.name().as_str(), "expected_name_kind");

    assert_eq!(
        kind.value(),
        &DiagnosticArgValue::NameKind(DiagnosticNameKind::Type)
    );

    assert_eq!(
        DiagnosticNameKind::CallableOverload.as_str(),
        "callable_overload"
    );
}

#[test]
fn diagnostic_io_error_kinds_keep_stable_categories() {
    assert_eq!(
        DiagnosticIoErrorKind::from(ErrorKind::NotFound),
        DiagnosticIoErrorKind::NotFound
    );

    assert_eq!(
        DiagnosticIoErrorKind::from(ErrorKind::ConnectionReset),
        DiagnosticIoErrorKind::Other
    );

    assert_eq!(DiagnosticIoErrorKind::NotFound.as_str(), "not_found");
}

#[test]
fn native_product_failure_keys_are_unique_and_domain_named() {
    use DiagnosticNativeProductFailureKind as Kind;

    let source = SourceSpan::new(
        SourceId::new(0),
        TextRange::new(TextSize::new(0), TextSize::new(1)),
    );

    let failures = [
        Kind::CodegenBackendNotSelected,
        Kind::MissingProductRoot,
        Kind::InvalidEntryResult,
        Kind::MissingRuntime,
        Kind::InvalidSymbolName,
        Kind::InvalidNativeLinkInput(DiagnosticNativeLinkInputFailure::InvalidRequirement {
            name: "native".to_owned(),
            link_kind: "dynamic".to_owned(),
            provenance_kind: "package",
            provenance_identity: Some("test".to_owned()),
        }),
        Kind::EvaluationCycle(crate::DiagnosticEvaluationFailureDetail::new("cycle", [])),
        Kind::EvaluationInfrastructure,
        Kind::EvaluationLoweringInput(DiagnosticLoweringInputFailure::new(
            DiagnosticLoweringInputFailureKind::InvalidStorageExit,
            source,
        )),
        Kind::EvaluationLowering(DiagnosticLoweringFailure::new(
            DiagnosticLoweringFailureKind::MissingCleanupPlan,
            source,
        )),
        Kind::SemanticContextFailure(crate::DiagnosticEvaluationFailureDetail::new(
            "semantic_context_failure",
            [],
        )),
        Kind::CheckingInfrastructureFailure,
        Kind::CodegenTargetUnsupportedProfile,
        Kind::CodegenTargetEmptyTriple,
        Kind::CodegenTargetEmptyCpu,
        Kind::CodegenTargetEmptyFeature,
        Kind::ReachabilityEmptyRoots,
        Kind::ReachabilityDuplicateInstance,
        Kind::ReachabilityUndemandedInstance,
        Kind::ReachabilityIncomplete,
        Kind::InstanceTemplateMismatch,
        Kind::InstanceTargetMismatch,
        Kind::InstanceDependencyTargetMismatch,
        Kind::UnitEmpty,
        Kind::UnitDuplicateInstance,
        Kind::UnitMissingCompatibility,
        Kind::UnitTargetMismatch,
        Kind::UnitWorkBoundExceeded,
        Kind::UnitRecipeMismatch,
        Kind::PartitionMissingCompatibility,
        Kind::PartitionInvalidUnit,
        Kind::GeneratedHostMirInvalid,
        Kind::ExecutableHostDuplicateRole,
        Kind::ExecutableHostMissingRuntime,
        Kind::ExecutableHostRuntimeOwnedBinding,
        Kind::ExecutableHostIncompatibleRuntime,
        Kind::ExecutableHostMissingMainThreadLane,
        Kind::ExecutableHostMissingProtectedFrameAbi,
        Kind::ExecutableHostMissingRole,
        Kind::RuntimeSelectionIncompatible,
        Kind::RuntimeSelectionMissingRoleOwner,
        Kind::RuntimeSelectionMissingCapabilityOwner,
        Kind::RuntimeSelectionUnreadableArchive,
        Kind::RuntimeSelectionArchiveDigestMismatch,
        Kind::StandardLibraryUnavailable,
        Kind::EmissionBackendDuplicateUnit,
        Kind::LinkTargetEmptyTriple,
        Kind::CodegenBackendUnavailable,
        Kind::CodegenInvalidRequest,
        Kind::CodegenMirUnavailable,
        Kind::CodegenMissingEntrypoint,
        Kind::CodegenInvalidInstance,
        Kind::CodegenInvalidUnit,
        Kind::CodegenUnitMismatch,
        Kind::CodegenInvalidHostMir,
        Kind::CodegenInvalidLifecycleMir,
        Kind::CodegenInvalidMappings,
        Kind::CodegenMissingRuntimeRole,
        Kind::CodegenOpenConstantTerm,
        Kind::CodegenInvalidArrayLength,
        Kind::CodegenRecursiveValueType,
        Kind::CodegenUnresolvedType,
        Kind::CodegenUnsizedTypeByValue,
        Kind::CodegenInvalidAbiMapping,
        Kind::CodegenUnsupportedType,
        Kind::CodegenMissingHelperInstance,
        Kind::CodegenLayoutOverflow,
        Kind::CodegenInvalidSymbolName,
    ];

    let keys = failures
        .each_ref()
        .map(DiagnosticNativeProductFailureKind::as_str);

    let unique = keys.into_iter().collect::<BTreeSet<_>>();

    assert_eq!(unique.len(), failures.len());

    for key in keys {
        assert!(!key.contains("fact"), "{key}");
        assert!(!key.contains("query"), "{key}");
        assert!(!key.contains("witness"), "{key}");
    }
}
