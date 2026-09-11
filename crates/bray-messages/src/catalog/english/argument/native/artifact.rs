use super::super::source::{format_english_artifact_digest, format_english_artifact_kind};
use super::checker::format_english_checker_failure;
use super::foreign_query::format_english_foreign_query_failure;
use super::linking::format_english_native_link_input_failure;
use super::lowering::{format_english_lowering_failure, format_english_lowering_input_failure};
use super::product_failure::native_product_failure_is_internal;
use super::product_query::format_english_product_query_failure;
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
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> String {
    let message = format_english_emission_evaluation_failure_detail(failure);

    if matches!(
        failure,
        bray_diagnostics::DiagnosticEmissionEvaluationFailure::Cancelled
    ) {
        message
    } else {
        super::format_internal_compiler_error(message)
    }
}

fn format_english_emission_evaluation_failure_detail(
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionEvaluationFailure as Failure;

    let message = match failure {
        Failure::Cancelled => "evaluation was cancelled",
        Failure::Cycle(_) => "evaluation encountered a dependency cycle",
        Failure::Runtime(failure) => return format_english_fact_runtime_failure(failure),
        Failure::SemanticValueStoreCreate => {
            "process-local semantic-value store identity capacity was exhausted during evaluation"
        }
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(*failure);
        }
        Failure::Binding(failure) => return format_english_binding_failure(failure),
        Failure::LoweringInput(failure) => return format_english_lowering_input_failure(*failure),
        Failure::Lowering(failure) => return format_english_lowering_failure(*failure),
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
        Failure::SemanticContext(_) => {
            "the selected program element has inconsistent checking context"
        }
        Failure::SemanticQuery(failure) => {
            return format_english_semantic_query_failure(failure);
        }
        Failure::Product(failure) => return format_english_product_query_failure(failure),
        Failure::Foreign(failure) => return format_english_foreign_query_failure(failure),
        Failure::Checker(failure) => return format_english_checker_failure(*failure),
    };

    message.to_owned()
}

fn format_english_semantic_query_failure(
    _failure: &bray_diagnostics::DiagnosticSemanticQueryFailure,
) -> String {
    super::format_internal_compiler_error("semantic analysis violated an internal contract")
}

pub(crate) fn format_english_native_product_failure(
    kind: &bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> String {
    let message = format_english_native_product_failure_detail(kind);

    if native_product_failure_is_internal(kind) {
        super::format_internal_compiler_error(message)
    } else {
        message
    }
}

fn format_english_native_product_failure_detail(
    kind: &bray_diagnostics::DiagnosticNativeProductFailureKind,
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
        Kind::InvalidNativeLinkInput(failure) => {
            return format_english_native_link_input_failure(failure);
        }
        Kind::EvaluationCancelled => "product construction was cancelled",
        Kind::EvaluationCycle(_) => "evaluation encountered a dependency cycle",
        Kind::EvaluationRuntime(failure) => return format_english_fact_runtime_failure(failure),
        Kind::EvaluationSemanticValueStoreCreate => {
            "process-local semantic-value store identity capacity was exhausted during product construction"
        }
        Kind::EvaluationSemanticValue(failure) => {
            return format_english_semantic_value_failure(*failure);
        }
        Kind::EvaluationBinding(failure) => {
            return format_english_binding_failure(failure);
        }
        Kind::EvaluationLoweringInput(failure) => {
            return format_english_lowering_input_failure(*failure);
        }
        Kind::EvaluationLowering(failure) => return format_english_lowering_failure(*failure),
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
        Kind::SemanticContextFailure(_) => "a program element has inconsistent checking context",
        Kind::EvaluationSemanticQuery(failure) => {
            return format_english_semantic_query_failure(failure);
        }
        Kind::EvaluationProduct(failure) => return format_english_product_query_failure(failure),
        Kind::EvaluationForeign(failure) => return format_english_foreign_query_failure(failure),
        Kind::CheckingInfrastructureFailure => "semantic checking could not complete",
        Kind::EvaluationChecker(failure) => {
            return format_english_checker_failure(*failure);
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
        Kind::PartitionMissingCompatibility(_) => {
            "a compiled program item lacks native-code grouping compatibility"
        }
        Kind::PartitionInvalidUnit(_) => "native-code grouping produced an invalid work item",
        Kind::GeneratedHostMirInvalid(_) => "the generated executable host is invalid",
        Kind::ExecutableHostDuplicateRole(_) => {
            "the executable host binds one runtime role more than once"
        }
        Kind::ExecutableHostMissingRuntime => {
            "the executable host requires a runtime that was not selected"
        }
        Kind::ExecutableHostRuntimeOwnedBinding(_) => {
            "the executable host attempts to own a runtime-owned binding"
        }
        Kind::ExecutableHostIncompatibleRuntime(_) => {
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
        Kind::ExecutableHostInvalidReturnedValueCleanup => {
            "the executable host has an invalid returned-value cleanup contract"
        }
        Kind::ExecutableHostMissingRole(_) => "the executable host lacks a required runtime role",
        Kind::RuntimeSelectionIncompatible(_) => {
            "the selected runtime is incompatible with the product"
        }
        Kind::RuntimeSelectionMissingRoleOwner(_) => {
            "the selected runtime has no owner for a required ABI role"
        }
        Kind::RuntimeSelectionMissingCapabilityOwner(_) => {
            "the selected runtime has no owner for a required capability"
        }
        Kind::RuntimeSelectionUnreadableArchive(_) => "a selected runtime archive cannot be read",
        Kind::RuntimeSelectionInvalidArchive(_) => "a selected runtime archive is invalid",
        Kind::RuntimeSelectionArchiveDigestMismatch(_) => {
            "a selected runtime archive does not match its declared digest"
        }
        Kind::StandardLibraryUnavailable => {
            "the configured standard library cannot supply a required native artifact"
        }
        Kind::EmissionBackendDuplicateUnit => "native-code work contains a duplicate work item",
        Kind::LinkTargetEmptyTriple => "the native link target has an empty target triple",
        Kind::CodegenBackendUnsupportedTarget => {
            "native code generation is unavailable for the selected target"
        }
        Kind::CodegenBackendUnsupportedTargetDetail(_) => {
            "native code generation is unavailable for the selected target"
        }
        Kind::CodegenBackendUnsupportedArtifact(_) => {
            "native code generation cannot produce a requested artifact"
        }
        Kind::CodegenBackendInvalidConfiguration
        | Kind::CodegenBackendInvalidConfigurationDetail(_) => {
            "native-code configuration is internally inconsistent"
        }
        Kind::CodegenBackendResourceExhausted => {
            "native code generation exceeded an available resource budget"
        }
        Kind::CodegenBackendResourceLimit(_) => {
            "native code generation exceeded an available resource budget"
        }
        Kind::CodegenBackendLibraryFailure(_) => {
            "backend-library processing failed for valid native-code input"
        }
        Kind::CodegenBackendToolFailure(_) => {
            "native code generation received an unsuccessful support-program result"
        }
        Kind::CodegenBackendGeneratedModuleInvariant
        | Kind::CodegenBackendGeneratedModuleInvariantDetail(_) => {
            "generated native-code input violated an internal module contract"
        }
        Kind::CodegenBackendInvalidRuntimeMetadata(_) => {
            "generated runtime metadata violated an internal publication contract"
        }
        Kind::CodegenBackendInvalidOutcome(_) => {
            "a completed native-code result violated an internal publication contract"
        }
        Kind::CodegenBackendRejectedModule(_) => {
            "internally generated input did not satisfy backend validation"
        }
        Kind::CodegenBackendArtifactConstruction(_) => {
            "a requested native artifact could not be constructed"
        }
        Kind::CodegenBackendUnavailable => "no native-code generator is available",
        Kind::CodegenInvalidRequest(_) => "the code generation request is internally inconsistent",
        Kind::CodegenMirUnavailable(_) => "the program is not ready for native code generation",
        Kind::CodegenMissingCallableImplementation { callable, .. } => {
            return format!(
                "callable {} has no executable implementation or native import",
                super::super::interface::format_english_interface_symbol_identity(callable)
            );
        }
        Kind::CodegenMissingEntrypoint => "the product has no selected entrypoint",
        Kind::CodegenInvalidInstance(_) => "a compiled program item is invalid",
        Kind::CodegenInvalidUnit(_) => "a native-code work item is invalid",
        Kind::CodegenUnitMismatch(_) => "a native-code work item differs from its original request",
        Kind::CodegenInvalidHostMir(_) => "generated executable startup code is invalid",
        Kind::CodegenInvalidLifecycleMir(_) => "generated lifecycle code is invalid",
        Kind::CodegenInvalidCompilerProvidedMir(_) => {
            "the compiler could not compile a compiler-provided callable in this product"
        }
        Kind::CodegenInvalidMappings(_) => {
            "required native-code metadata is incomplete or inconsistent"
        }
        Kind::CodegenMissingRuntimeRole(_) => "a required runtime ABI role has no selected binding",
        Kind::CodegenOpenConstantTerm(_) => {
            "a compiled constant still contains unresolved parameters"
        }
        Kind::CodegenInvalidArrayLength(_) => "a checked array length has no integer value",
        Kind::CodegenRecursiveValueType(_) => "a value type contains itself without indirection",
        Kind::CodegenUnresolvedType(_) => "a required type is still unresolved",
        Kind::CodegenUnsizedTypeByValue(_) => "an unsized type is used by value",
        Kind::CodegenInvalidAbiMapping(_) => "a callable ABI mapping is invalid",
        Kind::CodegenUnsupportedType(_) => {
            "the native-code generator cannot represent a required type"
        }
        Kind::CodegenMissingHelperInstance(_) => "a required generated helper is missing",
        Kind::CodegenLayoutOverflow(_) => "a required type layout exceeds the selected target",
        Kind::CodegenInvalidSymbolName => "a generated binary symbol name is not representable",
    };

    message.to_owned()
}

fn format_english_fact_runtime_failure(
    _failure: &bray_diagnostics::DiagnosticFactRuntimeFailure,
) -> String {
    super::format_internal_compiler_error("evaluation state violated an internal contract")
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

fn format_english_binding_failure(failure: &bray_diagnostics::DiagnosticBindingFailure) -> String {
    if let Some(failure) = failure.semantic_value_failure() {
        return format_english_semantic_value_failure(failure);
    }

    super::format_internal_compiler_error(
        "a source declaration or body violated an internal analysis contract",
    )
}

pub(super) fn format_english_semantic_value_failure(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> String {
    super::format_internal_compiler_error(format_english_semantic_value_failure_detail(failure))
}

pub(crate) const fn format_english_semantic_value_failure_detail(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    match failure {
        Failure::ForeignId { .. } => {
            "values from different compilations were combined while analyzing a source declaration or body"
        }
        Failure::UnknownId { .. } => {
            "a value required to analyze a source declaration or body was unavailable"
        }
        Failure::CapacityExhausted { .. } => {
            "semantic-value capacity was exhausted while retaining data required by this product"
        }
        Failure::GenericOwnerMismatch { .. } => {
            "generic arguments belong to a different declaration"
        }
        Failure::OpenSubstitution => {
            "generic substitution remained unresolved where concrete arguments were required"
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticCheckerNode, DiagnosticCheckerSymbol, DiagnosticFactRuntimeFailure,
        DiagnosticFailureField, DiagnosticFailureValue, DiagnosticProductQueryFailure,
        DiagnosticSemanticQueryFailure,
    };
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};

    use super::{
        format_english_binding_failure, format_english_checker_failure,
        format_english_emission_evaluation_failure, format_english_fact_runtime_failure,
        format_english_native_product_failure, format_english_product_query_failure,
        format_english_semantic_query_failure,
    };
    use crate::catalog::english::INTERNAL_COMPILER_ERROR;

    #[test]
    fn product_query_failures_hide_internal_context() {
        let message = format_english_product_query_failure(&DiagnosticProductQueryFailure::new(
            "product_query_missing",
            [DiagnosticFailureField::new(
                "product_context_identity",
                DiagnosticFailureValue::Identity([7; 32]),
            )],
        ));

        assert!(message.starts_with(INTERNAL_COMPILER_ERROR));
        assert!(!message.contains("product_query_missing"));
        assert!(!message.contains("07"));
        assert!(!message.contains(';'));
    }

    #[test]
    fn runtime_failures_render_one_shared_heading_without_internal_context() {
        let message = format_english_fact_runtime_failure(&DiagnosticFactRuntimeFailure::new(
            "publication_mismatch",
            [
                DiagnosticFailureField::new(
                    "requested_fact",
                    DiagnosticFailureValue::Text("SyntaxTree".to_owned()),
                ),
                DiagnosticFailureField::new("actual_task", DiagnosticFailureValue::Count(23)),
            ],
        ));

        assert!(message.starts_with(INTERNAL_COMPILER_ERROR));
        assert_eq!(message.matches(INTERNAL_COMPILER_ERROR).count(), 1);
        assert!(!message.contains("publication_mismatch"));
        assert!(!message.contains("SyntaxTree"));
        assert!(!message.contains("23"));
        assert!(!message.contains(';'));
    }

    #[test]
    fn compiler_defects_render_exactly_one_shared_internal_heading() {
        let nested = DiagnosticProductQueryFailure::new("product_query_missing", []);

        let failures = [
            bray_diagnostics::DiagnosticEmissionEvaluationFailure::SemanticValueStoreCreate,
            bray_diagnostics::DiagnosticEmissionEvaluationFailure::Product(nested.clone()),
        ];

        for failure in &failures {
            let message = format_english_emission_evaluation_failure(failure);

            assert!(message.starts_with(INTERNAL_COMPILER_ERROR));
            assert_eq!(message.matches(INTERNAL_COMPILER_ERROR).count(), 1);
            assert!(!message.contains(';'));
        }

        let failures = [
            bray_diagnostics::DiagnosticNativeProductFailureKind::EvaluationProduct(nested),
            bray_diagnostics::DiagnosticNativeProductFailureKind::SemanticContextFailure(
                bray_diagnostics::DiagnosticEvaluationFailureDetail::new(
                    "semantic_context_failure",
                    [],
                ),
            ),
            bray_diagnostics::DiagnosticNativeProductFailureKind::CheckingInfrastructureFailure,
            bray_diagnostics::DiagnosticNativeProductFailureKind::GeneratedHostMirInvalid(
                bray_diagnostics::DiagnosticNativeProductFailureDetail::new(
                    "generated_host_mir_invalid",
                    [],
                ),
            ),
            bray_diagnostics::DiagnosticNativeProductFailureKind::InstanceTemplateMismatch,
            bray_diagnostics::DiagnosticNativeProductFailureKind::CodegenBackendGeneratedModuleInvariant,
        ];

        for failure in &failures {
            let message = format_english_native_product_failure(failure);

            assert!(message.starts_with(INTERNAL_COMPILER_ERROR));
            assert_eq!(message.matches(INTERNAL_COMPILER_ERROR).count(), 1);
            assert!(!message.contains(';'));
        }

        for failure in [
            bray_diagnostics::DiagnosticNativeProductFailureKind::EvaluationCancelled,
            bray_diagnostics::DiagnosticNativeProductFailureKind::CodegenTargetUnsupportedProfile,
            bray_diagnostics::DiagnosticNativeProductFailureKind::StandardLibraryUnavailable,
            bray_diagnostics::DiagnosticNativeProductFailureKind::CodegenBackendUnsupportedTarget,
        ] {
            let message = format_english_native_product_failure(&failure);

            assert!(!message.starts_with(INTERNAL_COMPILER_ERROR));
            assert!(!message.contains(';'));
        }
    }

    #[test]
    fn semantic_query_failures_share_actor_free_user_facing_prose() {
        let first = format_english_semantic_query_failure(&DiagnosticSemanticQueryFailure::new(
            "semantic_query_bound_unit",
            "bound_unit_missing_root",
            [],
        ));

        let second = format_english_semantic_query_failure(&DiagnosticSemanticQueryFailure::new(
            "semantic_query_implementation",
            "implementation_match_semantic_value",
            [],
        ));

        assert_eq!(first, second);
        assert!(first.starts_with(INTERNAL_COMPILER_ERROR));
        assert_eq!(first.matches(INTERNAL_COMPILER_ERROR).count(), 1);
        assert!(!first.contains("compiler was"));
        assert!(!first.contains("bound source unit"));
        assert!(!first.contains("implementation evidence"));
        assert!(!first.contains(';'));
    }

    #[test]
    fn binding_failures_share_actor_free_user_facing_prose() {
        use bray_diagnostics::DiagnosticBindingFailure as Failure;

        let syntax = format_english_binding_failure(&Failure::new("binding_missing_syntax", []));

        let owner = format_english_binding_failure(&Failure::new("binding_missing_owner", []));

        assert_eq!(syntax, owner);
        assert!(syntax.starts_with(INTERNAL_COMPILER_ERROR));
        assert!(owner.starts_with(INTERNAL_COMPILER_ERROR));
        assert!(!syntax.contains("binding_missing"));
        assert!(!syntax.contains(';'));
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
            assert!(message.starts_with(INTERNAL_COMPILER_ERROR));
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
