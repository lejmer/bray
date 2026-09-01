use super::super::source::{format_english_artifact_digest, format_english_artifact_kind};
use super::checker::format_english_checker_failure;
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

pub(crate) fn format_english_emission_evaluation_failure(
    failure: bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionEvaluationFailure as Failure;

    let message = match failure {
        Failure::Cycle => "compiler evaluation encountered a dependency cycle",
        Failure::Infrastructure => "the compiler evaluation state became inconsistent",
        Failure::SemanticValueStoreCreate => {
            "the compiler exhausted its process-local semantic value store identities"
        }
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(failure).to_owned();
        }
        Failure::Binding(failure) => return format_english_binding_failure(failure).to_owned(),
        Failure::LoweringInput(failure) => return format_english_lowering_input_failure(failure),
        Failure::Lowering(failure) => return format_english_lowering_failure(failure),
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
        Failure::SemanticQuery(failure) => {
            return format_english_semantic_query_failure(failure).to_owned();
        }
        Failure::Checker(failure) => return format_english_checker_failure(failure),
    };

    message.to_owned()
}

fn format_english_semantic_query_failure(
    failure: bray_diagnostics::DiagnosticSemanticQueryFailure,
) -> String {
    use bray_diagnostics::DiagnosticSemanticQueryFailure as Failure;

    let detail = match failure {
        Failure::ContractViolation => "found inconsistent declaration information",
        Failure::CallableSignature => "found an inconsistent callable signature",
        Failure::GenericSubstitution => "found inconsistent generic arguments",
        Failure::BoundUnit => "found an inconsistent bound source unit",
        Failure::Implementation => "found inconsistent implementation evidence",
        Failure::CheckedConstantTerms => "found inconsistent checked constant terms",
        Failure::TypeSurface => "found inconsistent type-member information",
        Failure::PreparsedSyntax => "found inconsistent generated syntax",
    };

    super::format_internal_compiler_error(detail)
}

pub(crate) fn format_english_native_product_failure(
    kind: bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> String {
    use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

    let message = match kind {
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
        Kind::EvaluationSemanticValueStoreCreate => {
            "the compiler exhausted its process-local semantic value store identities"
        }
        Kind::EvaluationSemanticValue(failure) => {
            return format_english_semantic_value_failure(failure).to_owned();
        }
        Kind::EvaluationBinding(failure) => {
            return format_english_binding_failure(failure).to_owned();
        }
        Kind::EvaluationLoweringInput(failure) => {
            return format_english_lowering_input_failure(failure);
        }
        Kind::EvaluationLowering(failure) => return format_english_lowering_failure(failure),
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
        Kind::EvaluationChecker(failure) => {
            return format_english_checker_failure(failure);
        }
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
            return super::format_internal_compiler_error(
                "the selected declaration could not be compiled consistently",
            );
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
    };

    message.to_owned()
}

fn format_english_lowering_input_failure(
    failure: bray_diagnostics::DiagnosticLoweringInputFailure,
) -> String {
    use bray_diagnostics::DiagnosticLoweringInputFailureKind as Failure;

    let prevented_operation = match failure.kind() {
        Failure::StorageOperationCountMismatch { expected, actual } => format!(
            "reconciling the highlighted declaration's {actual} value accesses with the {expected} required by ownership analysis"
        ),
        Failure::LiteralTargetWidthMismatch { expected, actual } => format!(
            "generating target-correct code for the highlighted integer because it was interpreted as {actual} bits instead of the target's {expected} bits"
        ),
        Failure::ForeignInput => {
            "generating executable code for the highlighted declaration because required analysis belongs to another declaration".to_owned()
        }
        Failure::InputKindMismatch => {
            "generating executable code for the highlighted declaration because required analysis describes a different kind of declaration".to_owned()
        }
        Failure::MissingSemanticSelection => {
            "generating executable code for the highlighted expression because its selected callable or built-in behavior is unavailable".to_owned()
        }
        Failure::MissingExpressionType => {
            "generating executable code for the highlighted expression because the type established for it is unavailable".to_owned()
        }
        Failure::InvalidPatternInput => {
            "generating executable code for the highlighted pattern".to_owned()
        }
        Failure::InvalidInputContents => {
            "generating executable code for the highlighted declaration because required analysis refers to another declaration".to_owned()
        }
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(failure);
        }
        Failure::InvalidStorageOperation => {
            "generating ownership-safe code for the highlighted expression because its read, borrow, move, or write behavior is unavailable".to_owned()
        }
        Failure::InvalidStorageExit => {
            "generating cleanup code for the highlighted scope because the values it must release are unavailable".to_owned()
        }
        Failure::ExecutableHostRequiresSyntheticInput => {
            "generating program startup code because it was incorrectly associated with Bray source".to_owned()
        }
        Failure::CompileTimeUnitRequiresClassification => {
            "classifying the highlighted declaration before generating executable code".to_owned()
        }
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
}

fn format_english_lowering_failure(failure: bray_diagnostics::DiagnosticLoweringFailure) -> String {
    use bray_diagnostics::DiagnosticLoweringFailureKind as Failure;

    let prevented_operation = match failure.kind() {
        Failure::UnsupportedRoot => {
            "generating executable code for the highlighted declaration because it has no executable body"
        }
        Failure::MissingSourceNode(kind) => {
            return super::format_internal_compiler_error(format!(
                "could not generate executable code for the highlighted {}",
                format_source_construct(kind),
            ));
        }
        Failure::RecoveredSourceNode(kind) => {
            return super::format_internal_compiler_error(format!(
                "could not generate executable code for the highlighted {} after an earlier error",
                format_source_construct(kind),
            ));
        }
        Failure::MissingExpressionType => {
            "generating executable code for the highlighted expression because the type established for it is unavailable"
        }
        Failure::AwaitOutsideProtectedFrame => {
            "generating resumable code for the highlighted `await` expression"
        }
        Failure::MissingSuspensionPoint => {
            "generating a valid resume path for the highlighted `await` expression"
        }
        Failure::InvalidTaskOperation => {
            "generating a type-correct call for the highlighted task operation"
        }
        Failure::MissingCallableResultType => {
            "generating executable code for the highlighted callable because its result type is unavailable"
        }
        Failure::MissingLiteralValue => {
            "generating executable code for the highlighted literal because its value is unavailable"
        }
        Failure::MissingSemanticSelection => {
            "generating executable code for the highlighted expression because its selected callable or built-in behavior is unavailable"
        }
        Failure::UnsupportedExpression => {
            "generating executable code for the highlighted expression"
        }
        Failure::UnsupportedPattern => "generating executable code for the highlighted pattern",
        Failure::UnsupportedOperator => "generating executable code for the highlighted operator",
        Failure::MissingStorageAccess => {
            "generating ownership-safe code for the highlighted expression because its value-access behavior is unavailable"
        }
        Failure::MissingStorageAccessRecord => {
            "generating ownership-safe code for the highlighted expression because its read, borrow, move, or write behavior is unavailable"
        }
        Failure::MissingCleanupPlan => {
            "generating cleanup code for the highlighted scope because the values it must release are unavailable"
        }
        Failure::MissingStorageIdentity => {
            "generating ownership-safe code for the highlighted expression because the value it accesses is unavailable"
        }
        Failure::MissingStorageIdentityRecord => {
            "generating ownership-safe code for the highlighted declaration because one of its associated values is unavailable"
        }
        Failure::MissingIterationStorage => {
            "generating executable code for the highlighted iteration because its cursor or current value is unavailable"
        }
        Failure::UnsupportedStorageAccess => {
            "generating ownership-safe code for the highlighted value access"
        }
        Failure::MissingOperationResult => {
            "generating the value required by the highlighted expression"
        }
        Failure::MissingRepresentation => {
            "generating target-correct code for the highlighted declaration because its target representation is unavailable"
        }
        Failure::SemanticValueUnavailable => {
            "generating executable code for the highlighted declaration because a required type or constant value is unavailable"
        }
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(failure);
        }
        Failure::InvalidFrameDescriptor => {
            "generating resumable code for the highlighted callable because its state-preservation requirements conflict"
        }
        Failure::Mir(failure) => return format_english_mir_unit_failure(failure),
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
}

const fn format_source_construct(
    kind: bray_diagnostics::DiagnosticSourceConstructKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticSourceConstructKind as Kind;

    match kind {
        Kind::Expression => "expression",
        Kind::Pattern => "pattern",
        Kind::Block => "block",
        Kind::CallableBody => "callable body",
    }
}

fn format_english_mir_unit_failure(
    failure: bray_diagnostics::DiagnosticMirUnitBuildFailure,
) -> String {
    use bray_diagnostics::DiagnosticMirUnitBuildFailure as Failure;

    let prevented_operation = match failure {
        Failure::SourceOriginMismatch => {
            "generating executable code associated with the highlighted source declaration"
        }
        Failure::IdentityCapacityExceeded => {
            "generating executable code for the highlighted declaration because an internal capacity was exceeded"
        }
        Failure::ForeignBlock
        | Failure::ForeignOperation
        | Failure::ForeignStorage
        | Failure::ForeignValue => {
            "generating executable code for the highlighted declaration because required analysis belongs to another declaration"
        }
        Failure::MissingBlock | Failure::MissingOperation => {
            "generating all instructions required by the highlighted declaration"
        }
        Failure::MissingOperationResult => {
            "generating a value required by the highlighted declaration"
        }
        Failure::UnexpectedOperationResult => {
            "generating valid executable code for the highlighted declaration because it unexpectedly produces a value"
        }
        Failure::OperationResultTypeMismatch => {
            "generating type-correct executable code for the highlighted declaration"
        }
        Failure::InvalidAggregateOperation => {
            "generating a valid aggregate value for the highlighted declaration"
        }
        Failure::InvalidMemoryOperation => {
            "generating a type-correct read, write, move, or borrow for the highlighted declaration"
        }
        Failure::InvalidAnonymousCallable => {
            "generating executable code for the highlighted anonymous callable"
        }
        Failure::InvalidConstructionInput => {
            "generating a valid construction of the highlighted value"
        }
        Failure::InvalidCall => {
            "generating a type-correct executable call for the highlighted call expression"
        }
        Failure::InvalidHostOperation => {
            "generating program startup or shutdown code for the selected product"
        }
        Failure::InvalidHostSequence => "generating correctly ordered program shutdown code",
        Failure::MissingStorage => "reserving memory required by the highlighted declaration",
        Failure::MissingValue => "generating a value required by the highlighted declaration",
        Failure::DuplicateTerminator => {
            "generating a single outcome for each path through the highlighted declaration"
        }
        Failure::MissingTerminator => {
            "generating an outcome for every path through the highlighted declaration"
        }
        Failure::InvalidInlineAssemblyTerminator => {
            "generating type-correct branches for the highlighted inline assembly"
        }
        Failure::InvalidSuspensionPayload => {
            "preserving the required value while suspending the highlighted asynchronous callable"
        }
        Failure::InvalidCallPanicCheck => {
            "generating valid panic handling for the highlighted call"
        }
        Failure::EdgeArgumentCountMismatch => {
            "passing the required number of values between paths through the highlighted declaration"
        }
        Failure::EdgeArgumentTypeMismatch => {
            "passing type-correct values between paths through the highlighted declaration"
        }
        Failure::DuplicateSwitchCase => "generating distinct cases for the highlighted branch",
        Failure::CleanupTargetMismatch => "generating cleanup code for the highlighted scope",
        Failure::CleanupPhaseOrderViolation => {
            "generating correctly ordered cleanup code for the highlighted scope"
        }
        Failure::RuntimeRoleMismatch => {
            "selecting the required runtime service for the highlighted declaration"
        }
        Failure::RuntimeAbiVersionMismatch => {
            "selecting runtime services from one compatible runtime interface version"
        }
        Failure::InvalidOperationBlock => "placing each instruction on a path where it can run",
        Failure::StorageKindMismatch => {
            "generating ownership-safe code for the highlighted declaration"
        }
        Failure::StorageTypeMismatch => {
            "generating a type-correct value access for the highlighted declaration"
        }
        Failure::ValueDoesNotDominateUse => {
            "generating code that produces each value before the highlighted declaration uses it"
        }
        Failure::ProtectedFrameMismatch => {
            "generating resumable code for the highlighted asynchronous callable"
        }
        Failure::MissingFrameDescriptor => {
            "preserving the state needed to resume the highlighted asynchronous callable"
        }
        Failure::DuplicateFrameDescriptor => {
            "generating one consistent state layout for the highlighted asynchronous callable"
        }
        Failure::UnexpectedFrameDescriptor => {
            "generating valid executable code for the highlighted non-suspending callable"
        }
        Failure::InvalidFrameStateEntry => {
            "generating valid resume points for the highlighted asynchronous callable"
        }
        Failure::MissingFrameState => {
            "generating every required resume point for the highlighted asynchronous callable"
        }
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
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

fn format_english_binding_failure(failure: bray_diagnostics::DiagnosticBindingFailure) -> String {
    use bray_diagnostics::DiagnosticBindingFailure as Failure;

    let detail = match failure {
        Failure::DependencyUnavailable => "a name required by this product could not be resolved",
        Failure::InvalidUnitKey => {
            "the source declaration or body to compile could not be identified"
        }
        Failure::MissingSyntax => "source syntax required by this product was unavailable",
        Failure::MissingOwner => "the declaration that owns a source body could not be identified",
        Failure::MissingModule => "the module containing a declaration could not be identified",
        Failure::InvalidSurfaceName => "found a declaration without a valid local lookup name",
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(failure);
        }
        Failure::Construction => "a source declaration or body could not be analyzed",
        Failure::Binding => {
            "a source declaration or body could not be recovered after an earlier error"
        }
        Failure::Assembly => "a source declaration or body could not be validated",
    };

    super::format_internal_compiler_error(detail)
}

pub(crate) fn format_english_semantic_value_failure(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> String {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    let detail = match failure {
        Failure::ForeignId { .. } => {
            "mixed values from different compilations while understanding a source declaration or body"
        }
        Failure::UnknownId { .. } => {
            "lost a value required to understand a source declaration or body"
        }
        Failure::CapacityExhausted { .. } => {
            return "an internal compiler limit prevented Bray from retaining another value required by this product".to_owned();
        }
        Failure::GenericOwnerMismatch { .. } => {
            "associated generic arguments with the wrong declaration"
        }
        Failure::OpenSubstitution => {
            "required unresolved generic arguments where concrete arguments were needed"
        }
    };

    super::format_internal_compiler_error(detail)
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticCheckerNode, DiagnosticCheckerSymbol};
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};

    use super::{format_english_binding_failure, format_english_checker_failure};
    use crate::catalog::english::argument::native::INTERNAL_COMPILER_ERROR_PREFIX;

    #[test]
    fn binding_failures_render_distinct_user_facing_causes() {
        use bray_diagnostics::DiagnosticBindingFailure as Failure;

        let syntax = format_english_binding_failure(Failure::MissingSyntax);
        let owner = format_english_binding_failure(Failure::MissingOwner);

        assert_ne!(syntax, owner);
        assert!(syntax.contains("source syntax"));
        assert!(owner.contains("declaration"));
        assert!(syntax.starts_with(INTERNAL_COMPILER_ERROR_PREFIX));
        assert!(owner.starts_with(INTERNAL_COMPILER_ERROR_PREFIX));
    }

    #[test]
    fn checker_failures_render_distinct_source_level_operations() {
        use bray_diagnostics::DiagnosticCheckerFailure as Failure;

        let pattern = format_english_checker_failure(Failure::PatternInput(
            bray_diagnostics::DiagnosticPatternInputFailure::ConflictingDeclaredPattern(
                bray_diagnostics::DiagnosticCheckerNode::new("pattern", 1, 2),
            ),
        ));

        let storage = format_english_checker_failure(Failure::InvalidStoragePlan);

        assert_ne!(pattern, storage);
        assert!(pattern.contains("pattern"));
        assert!(storage.contains("local values"));
        assert!(!pattern.contains("checked"));
        assert!(!storage.contains("storage plan"));
    }

    #[test]
    fn checker_failures_keep_internal_identities_out_of_user_messages() {
        use bray_diagnostics::DiagnosticCheckerFailure as Failure;

        let missing = format_english_checker_failure(Failure::MissingSource {
            source_id: SourceId::new(7),
        });

        let version = format_english_checker_failure(Failure::SourceVersionMismatch {
            source_id: SourceId::new(8),
            expected: SourceVersion::new(13),
            actual: SourceVersion::new(21),
        });

        let range = format_english_checker_failure(Failure::InvalidSourceRange {
            span: SourceSpan::new(
                SourceId::new(9),
                TextRange::new(TextSize::new(34), TextSize::new(55)),
            ),
        });

        let query = format_english_checker_failure(Failure::SemanticQueryUnavailable {
            symbol: DiagnosticCheckerSymbol::new("module", 11),
            query: "members",
        });

        let expression = format_english_checker_failure(Failure::InvalidExpressionTypeInput {
            expression: DiagnosticCheckerNode::new("expression", 12, 17),
        });

        let node = format_english_checker_failure(Failure::InvalidBoundNode {
            node: DiagnosticCheckerNode::new("pattern", 14, 19),
        });

        for message in [&missing, &version, &range, &query, &expression, &node] {
            assert!(message.starts_with(INTERNAL_COMPILER_ERROR_PREFIX));
            assert!(!message.contains('#'));
        }

        assert!(missing.contains("source text"));
        assert!(version.contains("wrong source-text revision"));
        assert!(range.contains("source range"));
        assert!(query.contains("member declarations"));
        assert!(query.contains("highlighted module declaration"));
        assert!(expression.contains("highlighted expression"));
        assert!(node.contains("highlighted pattern"));
    }
}
