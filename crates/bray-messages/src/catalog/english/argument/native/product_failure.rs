pub(super) const fn native_product_failure_is_internal(
    kind: &bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> bool {
    use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

    match kind {
        Kind::EvaluationCycle(_)
        | Kind::EvaluationInfrastructure
        | Kind::EvaluationRuntime(_)
        | Kind::EvaluationSemanticValueStoreCreate
        | Kind::EvaluationSemanticValue(_)
        | Kind::EvaluationBinding(_)
        | Kind::EvaluationLoweringInput(_)
        | Kind::EvaluationLowering(_)
        | Kind::EvaluationConstantCallableBodyUnavailable
        | Kind::EvaluationConstantCallableRootUnavailable
        | Kind::EvaluationAtomicRepresentationTypeUnavailable
        | Kind::EvaluationAtomicRepresentationArgumentsUnavailable
        | Kind::EvaluationAtomicInitializerArgumentUnavailable
        | Kind::EvaluationAtomicInitializerResultUnavailable
        | Kind::EvaluationUninitInitializerResultUnavailable
        | Kind::EvaluationImportedExecutableTemplateMismatch
        | Kind::EvaluationSemanticQuery(_)
        | Kind::EvaluationProduct(_)
        | Kind::EvaluationForeign(_)
        | Kind::EvaluationChecker(_)
        | Kind::SemanticContextFailure(_)
        | Kind::CheckingInfrastructureFailure
        | Kind::ReachabilityEmptyRoots
        | Kind::ReachabilityDuplicateInstance
        | Kind::ReachabilityUndemandedInstance
        | Kind::ReachabilityIncomplete
        | Kind::InstanceTemplateMismatch
        | Kind::InstanceTargetMismatch
        | Kind::InstanceDependencyTargetMismatch
        | Kind::UnitEmpty
        | Kind::UnitDuplicateInstance
        | Kind::UnitMissingCompatibility
        | Kind::UnitTargetMismatch
        | Kind::UnitWorkBoundExceeded
        | Kind::UnitRecipeMismatch
        | Kind::PartitionMissingCompatibility
        | Kind::PartitionInvalidUnit
        | Kind::GeneratedHostMirInvalid
        | Kind::ExecutableHostDuplicateRole
        | Kind::ExecutableHostMissingRuntime
        | Kind::ExecutableHostRuntimeOwnedBinding
        | Kind::ExecutableHostIncompatibleRuntime
        | Kind::ExecutableHostMissingMainThreadLane
        | Kind::ExecutableHostMissingProtectedFrameAbi
        | Kind::ExecutableHostMissingRole
        | Kind::EmissionBackendDuplicateUnit
        | Kind::CodegenInvalidRequest
        | Kind::CodegenInvalidInstance
        | Kind::CodegenInvalidUnit
        | Kind::CodegenUnitMismatch
        | Kind::CodegenInvalidHostMir
        | Kind::CodegenInvalidLifecycleMir
        | Kind::CodegenInvalidMappings
        | Kind::CodegenMissingRuntimeRole
        | Kind::CodegenOpenConstantTerm
        | Kind::CodegenInvalidArrayLength
        | Kind::CodegenRecursiveValueType
        | Kind::CodegenUnresolvedType
        | Kind::CodegenUnsizedTypeByValue
        | Kind::CodegenInvalidAbiMapping
        | Kind::CodegenMissingHelperInstance
        | Kind::CodegenInvalidSymbolName => true,
        Kind::CodegenBackendNotSelected
        | Kind::MissingProductRoot
        | Kind::InvalidEntryResult
        | Kind::MissingRuntime
        | Kind::LibraryCleanupRequiresMainThread
        | Kind::InvalidSymbolName
        | Kind::InvalidNativeLinkInput(_)
        | Kind::EvaluationCancelled
        | Kind::CodegenTargetUnsupportedProfile
        | Kind::CodegenTargetEmptyTriple
        | Kind::CodegenTargetEmptyCpu
        | Kind::CodegenTargetEmptyFeature
        | Kind::RuntimeSelectionIncompatible
        | Kind::RuntimeSelectionMissingRoleOwner
        | Kind::RuntimeSelectionMissingCapabilityOwner
        | Kind::RuntimeSelectionUnreadableArchive
        | Kind::RuntimeSelectionInvalidArchive
        | Kind::RuntimeSelectionArchiveDigestMismatch
        | Kind::StandardLibraryUnavailable
        | Kind::LinkTargetEmptyTriple
        | Kind::CodegenBackendUnavailable
        | Kind::CodegenMirUnavailable
        | Kind::CodegenMissingEntrypoint
        | Kind::CodegenUnsupportedType
        | Kind::CodegenLayoutOverflow => false,
    }
}
