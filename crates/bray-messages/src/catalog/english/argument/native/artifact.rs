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
        Failure::Infrastructure => "the compiler evaluation state became inconsistent",
        Failure::LoweringInput(failure) => format_english_lowering_input_failure(failure),
        Failure::Lowering(failure) => format_english_lowering_failure(failure),
        Failure::ConstantCallableBodyUnavailable => {
            "the selected constant callable has no available body"
        }
        Failure::ConstantCallableRootUnavailable => {
            "the selected constant callable body has no result expression"
        }
        Failure::AtomicRepresentationTypeUnavailable => {
            "the selected atomic value has no available representation type"
        }
        Failure::AtomicRepresentationArgumentsUnavailable => {
            "the selected atomic value has no available representation arguments"
        }
        Failure::AtomicInitializerArgumentUnavailable => {
            "the atomic initializer argument has no compile-time value"
        }
        Failure::AtomicInitializerResultUnavailable => {
            "the atomic initializer result cannot be represented as a compile-time value"
        }
        Failure::UninitInitializerResultUnavailable => {
            "the uninitialized-storage initializer result cannot be represented as a compile-time value"
        }
        Failure::ImportedExecutableTemplateMismatch => {
            "an imported native operation does not match its compiled definition"
        }
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
        Kind::EvaluationLoweringInput(failure) => format_english_lowering_input_failure(failure),
        Kind::EvaluationLowering(failure) => format_english_lowering_failure(failure),
        Kind::EvaluationConstantCallableBodyUnavailable => {
            "the selected constant callable has no available body"
        }
        Kind::EvaluationConstantCallableRootUnavailable => {
            "the selected constant callable body has no result expression"
        }
        Kind::EvaluationAtomicRepresentationTypeUnavailable => {
            "the selected atomic value has no available representation type"
        }
        Kind::EvaluationAtomicRepresentationArgumentsUnavailable => {
            "the selected atomic value has no available representation arguments"
        }
        Kind::EvaluationAtomicInitializerArgumentUnavailable => {
            "the atomic initializer argument has no compile-time value"
        }
        Kind::EvaluationAtomicInitializerResultUnavailable => {
            "the atomic initializer result cannot be represented as a compile-time value"
        }
        Kind::EvaluationUninitInitializerResultUnavailable => {
            "the uninitialized-storage initializer result cannot be represented as a compile-time value"
        }
        Kind::EvaluationImportedExecutableTemplateMismatch => {
            "an imported native operation does not match its compiled definition"
        }
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
        Kind::LibraryCleanupRequiresMainThread => {
            "a library static cleanup operation requires main-thread execution"
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
        Kind::RuntimeSelectionInvalidArchive => "a selected runtime archive is invalid",
        Kind::RuntimeSelectionArchiveDigestMismatch => {
            "a selected runtime archive does not match its declared digest"
        }
        Kind::EmissionBackendDuplicateUnit => {
            "the native-code generator contains a duplicate work item"
        }
        Kind::LinkTargetEmptyTriple => "the native link target has an empty target triple",
        Kind::CodegenBackendUnavailable => "no native-code generator is available",
        Kind::CodegenInvalidRequest => "the code generation request is internally inconsistent",
        Kind::CodegenMirUnavailable => "the program is not ready for native code generation",
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

const fn format_english_lowering_input_failure(
    failure: bray_diagnostics::DiagnosticLoweringInputFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticLoweringInputFailure as Failure;

    match failure {
        Failure::ForeignInput => "checked program information belongs to another program unit",
        Failure::InputKindMismatch => {
            "checked program information describes another program-unit category"
        }
        Failure::MissingSemanticSelection => {
            "a checked expression has no selected semantic operation"
        }
        Failure::MissingExpressionType => "a checked expression has no final type",
        Failure::InvalidPatternInput => "checked pattern information is inconsistent",
        Failure::InvalidInputContents => "checked program information contains unknown identities",
        Failure::InvalidStorageOperation => {
            "a checked storage operation does not match its storage plan"
        }
        Failure::StorageOperationCountMismatch => {
            "checked storage operations do not exactly cover the storage plan"
        }
        Failure::InvalidStorageExit => {
            "a checked scope exit refers to unknown storage information"
        }
        Failure::LiteralTargetWidthMismatch => {
            "a checked integer literal uses another target width"
        }
        Failure::ExecutableHostRequiresSyntheticInput => {
            "an executable host was supplied through a source program unit"
        }
        Failure::CompileTimeUnitRequiresClassification => {
            "a compile-time program unit reached executable translation"
        }
    }
}

const fn format_english_lowering_failure(
    failure: bray_diagnostics::DiagnosticLoweringFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticLoweringFailure as Failure;

    match failure {
        Failure::UnsupportedRoot => "the program-unit root cannot be translated",
        Failure::MissingBoundNode => "a required checked program node is missing",
        Failure::RecoveredBoundNode => "an error-recovery node reached executable translation",
        Failure::MissingExpressionType => "a checked expression has no final type",
        Failure::AwaitOutsideProtectedFrame => {
            "an await operation has no protected execution frame"
        }
        Failure::MissingSuspensionPoint => "an await operation has no suspension plan",
        Failure::InvalidTaskOperation => "a task operation has an incompatible call shape",
        Failure::MissingCallableResultType => "a callable has no checked result type",
        Failure::MissingLiteralValue => "a checked literal has no canonical value",
        Failure::MissingSemanticSelection => {
            "a checked expression has no selected semantic operation"
        }
        Failure::UnsupportedExpression => "a checked expression cannot be translated",
        Failure::UnsupportedPattern => "a checked pattern cannot be translated",
        Failure::UnsupportedOperator => "a checked operator has no executable operation",
        Failure::MissingStorageAccess => "an expression has no checked storage access",
        Failure::MissingStorageAccessRecord => {
            "a checked storage access is absent from the storage plan"
        }
        Failure::MissingCleanupPlan => "a checked scope exit has no cleanup plan",
        Failure::MissingStorageIdentity => "a checked access reaches no storage identity",
        Failure::MissingStorageIdentityRecord => {
            "a checked storage identity is absent from the storage plan"
        }
        Failure::MissingIterationStorage => "an iteration has no checked cursor or element storage",
        Failure::UnsupportedStorageAccess => "a checked storage path cannot be translated",
        Failure::MissingOperationResult => "a value-producing operation has no result",
        Failure::MissingRepresentation => {
            "a required compiler-provided representation is unavailable"
        }
        Failure::SemanticValueUnavailable => "a checked semantic value is unavailable",
        Failure::InvalidFrameDescriptor => "an execution frame description is inconsistent",
        Failure::Mir(failure) => format_english_mir_unit_failure(failure),
    }
}

const fn format_english_mir_unit_failure(
    failure: bray_diagnostics::DiagnosticMirUnitBuildFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticMirUnitBuildFailure as Failure;

    match failure {
        Failure::SourceOriginMismatch => "generated source information belongs to another unit",
        Failure::IdentityCapacityExceeded => "generated unit identity capacity was exceeded",
        Failure::ForeignBlock => "a generated block belongs to another unit",
        Failure::ForeignOperation => "a generated operation belongs to another unit",
        Failure::ForeignStorage => "generated storage belongs to another unit",
        Failure::ForeignValue => "a generated value belongs to another unit",
        Failure::MissingBlock => "a referenced generated block is missing",
        Failure::MissingOperation => "a referenced generated operation is missing",
        Failure::MissingOperationResult => "a generated operation has no required result",
        Failure::UnexpectedOperationResult => "a generated operation has an unexpected result",
        Failure::OperationResultTypeMismatch => "a generated operation result has the wrong type",
        Failure::InvalidAggregateOperation => "a generated aggregate operation is inconsistent",
        Failure::InvalidMemoryOperation => "a generated memory operation is inconsistent",
        Failure::InvalidAnonymousCallable => "a generated anonymous callable is inconsistent",
        Failure::InvalidConstructionInput => "a generated construction input is inconsistent",
        Failure::InvalidCall => "a generated call is inconsistent",
        Failure::InvalidHostOperation => "a generated host operation is inconsistent",
        Failure::InvalidHostSequence => "generated host operations have an invalid sequence",
        Failure::MissingStorage => "a referenced generated storage allocation is missing",
        Failure::MissingValue => "a referenced generated value is missing",
        Failure::DuplicateTerminator => "a generated block has more than one terminator",
        Failure::MissingTerminator => "a generated block has no terminator",
        Failure::InvalidInlineAssemblyTerminator => {
            "a generated inline-assembly branch is inconsistent"
        }
        Failure::InvalidSuspensionPayload => "a generated suspension payload is inconsistent",
        Failure::InvalidCallPanicCheck => "a generated call panic check is inconsistent",
        Failure::EdgeArgumentCountMismatch => "a generated edge supplies the wrong argument count",
        Failure::EdgeArgumentTypeMismatch => "a generated edge supplies an argument of the wrong type",
        Failure::DuplicateSwitchCase => "a generated switch contains a duplicate case",
        Failure::CleanupTargetMismatch => "a generated cleanup edge targets the wrong cleanup phase",
        Failure::CleanupPhaseOrderViolation => "generated cleanup phases have an invalid order",
        Failure::RuntimeRoleMismatch => "a generated runtime operation uses the wrong runtime role",
        Failure::RuntimeAbiVersionMismatch => {
            "a generated runtime reference uses another runtime interface version"
        }
        Failure::InvalidOperationBlock => "a generated operation is not valid in its block",
        Failure::StorageKindMismatch => "a generated operation uses storage of the wrong kind",
        Failure::StorageTypeMismatch => "a generated storage operation uses incompatible types",
        Failure::ValueDoesNotDominateUse => "a generated value is used outside its valid region",
        Failure::ProtectedFrameMismatch => "a generated operation uses another protected frame",
        Failure::MissingFrameDescriptor => "a protected program unit has no frame description",
        Failure::DuplicateFrameDescriptor => {
            "a protected program unit has more than one frame description"
        }
        Failure::UnexpectedFrameDescriptor => {
            "an unprotected program unit has a frame description"
        }
        Failure::InvalidFrameStateEntry => "a protected frame state has an invalid entry block",
        Failure::MissingFrameState => "a referenced protected frame state is missing",
    }
}

pub(crate) fn format_english_runtime_artifact_problem(
    problem: &bray_diagnostics::DiagnosticRuntimeArtifactProblem,
) -> String {
    use bray_diagnostics::DiagnosticRuntimeArtifactProblem as Problem;

    match problem {
        Problem::MetadataSizeLimitExceeded => "metadata exceeds the size limit".to_owned(),
        Problem::MalformedMetadata => "malformed metadata".to_owned(),
        Problem::UnsupportedFormat => "unsupported metadata format".to_owned(),
        Problem::InvalidRuntimeIdentity => "invalid runtime identity".to_owned(),
        Problem::InvalidArtifactIdentity => "invalid artifact identity".to_owned(),
        Problem::InvalidTarget => "invalid target identity".to_owned(),
        Problem::InvalidPanicAbi => "invalid panic ABI identity".to_owned(),
        Problem::UnknownCapability => "unknown runtime capability".to_owned(),
        Problem::UnknownRole => "unknown runtime role".to_owned(),
        Problem::UnknownPlatformService => "unknown native operation".to_owned(),
        Problem::InvalidRoleSymbol => "invalid runtime role symbol".to_owned(),
        Problem::UnknownRoleImplementation => "unknown runtime role implementation".to_owned(),
        Problem::InvalidNativeLinkName => "invalid native link name".to_owned(),
        Problem::UnknownNativeLinkKind => "unknown native link kind".to_owned(),
        Problem::UnknownComponentPurpose => "unknown runtime component purpose".to_owned(),
        Problem::InvalidComponentIdentity => "invalid runtime component identity".to_owned(),
        Problem::InvalidArchiveDigest => "invalid archive digest".to_owned(),
        Problem::DuplicateContractRole(role) => format!("duplicate runtime contract role `{role}`"),
        Problem::CompilerOwnedRole(role) => {
            format!("compiler-owned role `{role}` is published by the runtime")
        }
        Problem::MissingCooperativeExecution => {
            "runtime contract omits cooperative execution".to_owned()
        }
        Problem::InvalidArchiveFileName => "invalid runtime component archive file name".to_owned(),
        Problem::UnreferencedSupportComponent(component) => {
            format!("support component `{component}` is unreferenced")
        }
        Problem::DuplicateComponent(component) => {
            format!("duplicate runtime component `{component}`")
        }
        Problem::InvalidComponentDependency {
            component,
            dependency,
        } => format!("component `{component}` has invalid dependency `{dependency}`"),
        Problem::ComponentDependencyCycle(component) => {
            format!("component dependency cycle includes `{component}`")
        }
        Problem::UnknownComponentRole(role) => {
            format!("component claims unknown runtime role `{role}`")
        }
        Problem::UnknownComponentCapability(capability) => {
            format!("component claims unknown runtime capability `{capability}`")
        }
        Problem::TestRoleInProductComponent(component) => {
            format!("product component `{component}` claims the test-entry role")
        }
        Problem::MissingRoleOwner { purpose, role } => format!(
            "{} runtime surface has no owner for role `{role}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicateRoleOwner { purpose, role } => format!(
            "{} runtime surface has multiple owners for role `{role}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::MissingCapabilityOwner {
            purpose,
            capability,
        } => format!(
            "{} runtime surface has no owner for capability `{capability}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicateCapabilityOwner {
            purpose,
            capability,
        } => format!(
            "{} runtime surface has multiple owners for capability `{capability}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicatePlatformServiceOwner { purpose } => format!(
            "the same native operation is provided more than once for {} programs",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::MissingComponent => "missing runtime component".to_owned(),
        Problem::UnexpectedComponent => "unexpected runtime component".to_owned(),
        Problem::ArchiveFileNameMismatch => "archive file name does not match metadata".to_owned(),
    }
}

const fn format_english_runtime_artifact_purpose(
    purpose: bray_diagnostics::DiagnosticRuntimeArtifactPurpose,
) -> &'static str {
    match purpose {
        bray_diagnostics::DiagnosticRuntimeArtifactPurpose::Product => "product",
        bray_diagnostics::DiagnosticRuntimeArtifactPurpose::TestRunner => "test-runner",
    }
}
