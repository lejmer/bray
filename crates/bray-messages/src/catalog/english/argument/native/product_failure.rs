pub(super) const fn native_product_failure_is_internal(
    kind: &bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> bool {
    use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

    match kind {
        Kind::EvaluationCycle(_)
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
        | Kind::PartitionMissingCompatibility(_)
        | Kind::PartitionInvalidUnit(_)
        | Kind::GeneratedHostMirInvalid(_)
        | Kind::ExecutableHostDuplicateRole(_)
        | Kind::ExecutableHostMissingRuntime
        | Kind::ExecutableHostRuntimeOwnedBinding(_)
        | Kind::ExecutableHostIncompatibleRuntime(_)
        | Kind::ExecutableHostMissingMainThreadLane
        | Kind::ExecutableHostMissingProtectedFrameAbi
        | Kind::ExecutableHostMissingRole(_)
        | Kind::EmissionBackendDuplicateUnit
        | Kind::CodegenBackendInvalidConfiguration
        | Kind::CodegenBackendInvalidConfigurationDetail(_)
        | Kind::CodegenBackendResourceExhausted
        | Kind::CodegenBackendResourceLimit(_)
        | Kind::CodegenBackendLibraryFailure(_)
        | Kind::CodegenBackendToolFailure(_)
        | Kind::CodegenBackendGeneratedModuleInvariant
        | Kind::CodegenBackendGeneratedModuleInvariantDetail(_)
        | Kind::CodegenBackendInvalidRuntimeMetadata(_)
        | Kind::CodegenBackendInvalidOutcome(_)
        | Kind::CodegenBackendRejectedModule(_)
        | Kind::CodegenBackendArtifactConstruction(_)
        | Kind::CodegenInvalidRequest(_)
        | Kind::CodegenInvalidInstance(_)
        | Kind::CodegenInvalidUnit(_)
        | Kind::CodegenUnitMismatch(_)
        | Kind::CodegenInvalidHostMir(_)
        | Kind::CodegenInvalidLifecycleMir(_)
        | Kind::CodegenInvalidCompilerProvidedMir(_)
        | Kind::CodegenMissingCallableImplementation { .. }
        | Kind::CodegenInvalidMappings(_)
        | Kind::CodegenMissingRuntimeRole(_)
        | Kind::CodegenOpenConstantTerm(_)
        | Kind::CodegenInvalidArrayLength(_)
        | Kind::CodegenRecursiveValueType(_)
        | Kind::CodegenUnresolvedType(_)
        | Kind::CodegenUnsizedTypeByValue(_)
        | Kind::CodegenInvalidAbiMapping(_)
        | Kind::CodegenMissingHelperInstance(_)
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
        | Kind::RuntimeSelectionIncompatible(_)
        | Kind::RuntimeSelectionMissingRoleOwner(_)
        | Kind::RuntimeSelectionMissingCapabilityOwner(_)
        | Kind::RuntimeSelectionUnreadableArchive(_)
        | Kind::RuntimeSelectionInvalidArchive(_)
        | Kind::RuntimeSelectionArchiveDigestMismatch(_)
        | Kind::StandardLibraryUnavailable
        | Kind::LinkTargetEmptyTriple
        | Kind::CodegenBackendUnsupportedTarget
        | Kind::CodegenBackendUnsupportedTargetDetail(_)
        | Kind::CodegenBackendUnsupportedArtifact(_)
        | Kind::CodegenBackendUnavailable
        | Kind::CodegenMirUnavailable(_)
        | Kind::CodegenMissingEntrypoint
        | Kind::CodegenUnsupportedType(_)
        | Kind::CodegenLayoutOverflow(_) => false,
    }
}
