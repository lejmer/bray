use super::super::source::{format_english_artifact_digest, format_english_artifact_kind};
use bray_diagnostics::DiagnosticArtifactDigest;

pub(crate) fn format_artifact_failure(
    message: &str,
    artifact: bray_diagnostics::DiagnosticEmissionArtifact,
) -> String {
    format!(
        "{message}: {} artifact #{}",
        format_english_artifact_kind(artifact.kind()),
        artifact.ordinal(),
    )
}

pub(crate) fn format_unit_failure(message: &str, unit: &DiagnosticArtifactDigest) -> String {
    format!(
        "native-code work item {} {message}",
        format_english_artifact_digest(unit),
    )
}

pub(crate) const fn format_english_emission_evaluation_failure(
    failure: bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticEmissionEvaluationFailure as Failure;

    match failure {
        Failure::Cycle => "compiler evaluation encountered a dependency cycle",
        Failure::Infrastructure => "the compiler could not complete the evaluation",
        Failure::SemanticContext => {
            "the selected program element has inconsistent checking context"
        }
        Failure::CheckerInfrastructure => "semantic checking could not complete a dependency",
    }
}

pub(crate) const fn format_english_native_product_failure(
    kind: bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

    match kind {
        Kind::CodegenBackendNotSelected => "no native-code generator is selected",
        Kind::MissingProductRoot => "the product has no executable code root",
        Kind::InvalidEntryResult => {
            "the executable entry result does not match the product contract"
        }
        Kind::MissingRuntime => "the asynchronous product has no selected runtime",
        Kind::InvalidSymbolName => "a generated binary symbol name is not representable",
        Kind::InvalidNativeLinkInput => "a configured native link input is invalid",
        Kind::EvaluationCycle => "compiler evaluation encountered a dependency cycle",
        Kind::EvaluationInfrastructure => "the compiler could not complete product construction",
        Kind::SemanticContextFailure => "a program element has inconsistent checking context",
        Kind::CheckingInfrastructureFailure => "semantic checking could not complete",
        Kind::CodegenTargetUnsupportedProfile => {
            "the selected target profile cannot generate native code"
        }
        Kind::CodegenTargetEmptyTriple => {
            "the selected native-code target has an empty target triple"
        }
        Kind::CodegenTargetEmptyCpu => "the selected native-code target has an empty CPU name",
        Kind::CodegenTargetEmptyFeature => {
            "the selected native-code target has an empty feature name"
        }
        Kind::ReachabilityEmptyRoots => "native-code selection has no executable roots",
        Kind::ReachabilityDuplicateInstance => {
            "native-code selection contains a duplicate compiled item"
        }
        Kind::ReachabilityUndemandedInstance => {
            "native-code generation returned a compiled item that was not requested"
        }
        Kind::ReachabilityIncomplete => "native-code selection is incomplete",
        Kind::InstanceTemplateMismatch => {
            "a compiled item does not match its checked program definition"
        }
        Kind::InstanceTargetMismatch => "a compiled item does not match the selected target",
        Kind::InstanceDependencyTargetMismatch => {
            "a compiled item dependency targets another platform"
        }
        Kind::UnitEmpty => "a native-code work item contains no compiled program items",
        Kind::UnitDuplicateInstance => {
            "a native-code work item contains a duplicate compiled program item"
        }
        Kind::UnitMissingCompatibility => {
            "a native-code work item lacks required grouping compatibility"
        }
        Kind::UnitTargetMismatch => {
            "a native-code work item combines program items for different targets"
        }
        Kind::UnitWorkBoundExceeded => "a native-code work item exceeds its work limit",
        Kind::UnitRecipeMismatch => {
            "a reconstructed native-code work item differs from its original request"
        }
        Kind::PartitionMissingCompatibility => {
            "a compiled program item lacks native-code grouping compatibility"
        }
        Kind::PartitionInvalidUnit => "native-code grouping produced an invalid work item",
        Kind::GeneratedHostMirInvalid => "the compiler-generated executable host is invalid",
        Kind::ExecutableHostDuplicateRole => {
            "the executable host binds one runtime role more than once"
        }
        Kind::ExecutableHostMissingRuntime => {
            "the executable host requires a runtime that was not selected"
        }
        Kind::ExecutableHostRuntimeOwnedBinding => {
            "the executable host attempts to own a runtime-owned binding"
        }
        Kind::ExecutableHostIncompatibleRuntime => {
            "the selected runtime is incompatible with the executable host"
        }
        Kind::ExecutableHostMissingMainThreadLane => {
            "the executable host requires a main-thread execution lane"
        }
        Kind::ExecutableHostMissingProtectedFrameAbi => {
            "the executable host lacks the required protected-frame ABI"
        }
        Kind::ExecutableHostMissingRole => "the executable host lacks a required runtime role",
        Kind::RuntimeSelectionIncompatible => {
            "the selected runtime is incompatible with the product"
        }
        Kind::RuntimeSelectionMissingRoleOwner => {
            "the selected runtime has no owner for a required ABI role"
        }
        Kind::RuntimeSelectionMissingCapabilityOwner => {
            "the selected runtime has no owner for a required capability"
        }
        Kind::RuntimeSelectionUnreadableArchive => "a selected runtime archive cannot be read",
        Kind::RuntimeSelectionArchiveDigestMismatch => {
            "a selected runtime archive does not match its declared digest"
        }
        Kind::EmissionBackendDuplicateUnit => {
            "the native-code generator contains a duplicate work item"
        }
        Kind::LinkTargetEmptyTriple => "the native link target has an empty target triple",
        Kind::CodegenBackendUnavailable => "no native-code generator is available",
        Kind::CodegenInvalidRequest => "the code generation request is internally inconsistent",
        Kind::CodegenMirUnavailable => "the required checked program representation is unavailable",
        Kind::CodegenMissingEntrypoint => "the product has no selected entrypoint",
        Kind::CodegenInvalidInstance => "a compiled program item is invalid",
        Kind::CodegenInvalidUnit => "a native-code work item is invalid",
        Kind::CodegenUnitMismatch => "a native-code work item differs from its original request",
        Kind::CodegenInvalidHostMir => "generated executable startup code is invalid",
        Kind::CodegenInvalidLifecycleMir => "generated lifecycle code is invalid",
        Kind::CodegenInvalidMappings => {
            "required native-code metadata is incomplete or inconsistent"
        }
        Kind::CodegenMissingRuntimeRole => "a required runtime ABI role has no selected binding",
        Kind::CodegenOpenConstantTerm => "a compiled constant still contains unresolved parameters",
        Kind::CodegenInvalidArrayLength => "a checked array length has no integer value",
        Kind::CodegenRecursiveValueType => "a value type contains itself without indirection",
        Kind::CodegenUnresolvedType => "a required type is still unresolved",
        Kind::CodegenUnsizedTypeByValue => "an unsized type is used by value",
        Kind::CodegenInvalidAbiMapping => "a callable ABI mapping is invalid",
        Kind::CodegenUnsupportedType => {
            "the native-code generator cannot represent a required type"
        }
        Kind::CodegenMissingHelperInstance => "a required generated helper is missing",
        Kind::CodegenLayoutOverflow => "a required type layout exceeds the selected target",
        Kind::CodegenInvalidSymbolName => "a generated binary symbol name is not representable",
    }
}
