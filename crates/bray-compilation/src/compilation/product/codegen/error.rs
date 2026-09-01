use bray_codegen::{
    CodegenInstanceBuildError, CodegenPartitionError, CodegenReachabilityBuildError,
    CodegenTargetBuildError, CodegenUnitBuildError,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
    DiagnosticNativeProductFailureKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_emitter::EmissionBackendBuildError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::{ExecutableHostContractBuildError, RuntimeArtifactSelectionError};
use bray_symbols::ProductIdentity;

use crate::fact::{BatchCompletionError, FactQueryError};

/// Exact native link-input contract failure found during product planning.
#[derive(Debug, Hash)]
pub enum NativeLinkInputPlanningError {
    /// A standard-library artifact has no supported static link representation.
    UnsupportedStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: std::path::PathBuf,
        /// Exact rejected standard-library artifact category.
        kind: bray_standard_library::StandardLibraryArtifactKind,
    },
    /// A standard-library artifact could not form its retained link-input specification.
    InvalidStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: std::path::PathBuf,
        /// Selected linker input category.
        kind: bray_linker::LinkInputKind,
        /// Exact link-input contract failure.
        cause: bray_linker::LinkInputBuildError,
    },
    /// A source or platform native-link requirement could not form a linker input.
    InvalidRequirement {
        /// Exact requested native input name.
        name: String,
        /// Exact requested native link category.
        kind: bray_symbols::NativeLinkKind,
        /// Exact provider retained for the input.
        provenance: bray_linker::LinkInputProvenance,
    },
}

/// A failure to derive complete native product plans.
#[derive(Debug, Hash)]
pub enum NativeProductPlanningError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected backend could not form its native bitcode target contract.
    BitcodeTargetContract(bray_codegen::CodegenFailure),
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// The checked executable entry result is inconsistent with product semantics.
    InvalidEntryResult,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A library static cleanup closure requires unavailable main-thread execution.
    LibraryCleanupRequiresMainThread,
    /// A generated binary symbol name is invalid.
    InvalidSymbolName,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput(NativeLinkInputPlanningError),
    /// A lazy compilation plan could not be evaluated.
    Query(FactQueryError),
    /// The selected code generation target is invalid.
    InvalidCodegenTarget(CodegenTargetBuildError),
    /// Code generation reachability is inconsistent.
    InvalidReachability(CodegenReachabilityBuildError),
    /// One concrete code generation instance is invalid.
    InvalidCodegenInstance(CodegenInstanceBuildError),
    /// One code generation unit is invalid.
    InvalidCodegenUnit(CodegenUnitBuildError),
    /// Reachable definitions cannot be partitioned under the selected policy.
    InvalidCodegenPartition(CodegenPartitionError),
    /// The compiler-generated executable host MIR is invalid.
    InvalidHostMir(bray_ir::MirUnitBuildError),
    /// The compiler-generated executable host contract is invalid.
    InvalidExecutableHost(ExecutableHostContractBuildError),
    /// Runtime metadata cannot supply an exact physical component selection.
    InvalidRuntimeSelection(RuntimeArtifactSelectionError),
    /// The selected emitter backend description is invalid.
    InvalidEmissionBackend(EmissionBackendBuildError),
    /// The selected linker target is invalid.
    InvalidLinkTarget(LinkTargetBuildError),
    /// The configured standard library cannot supply a required native artifact.
    StandardLibrary(bray_standard_library::StandardLibraryLoadError),
    /// One code generation plan is unavailable.
    Codegen(super::super::super::CodegenPreparationError),
}

pub(super) fn native_batch_error<K>(
    error: BatchCompletionError<K, NativeProductPlanningError>,
) -> NativeProductPlanningError {
    match error {
        BatchCompletionError::Cancelled => FactQueryError::Cancelled.into(),
        BatchCompletionError::Evaluation { error, .. } => error,
        BatchCompletionError::Scheduler(error) => error.into(),
    }
}

impl NativeProductPlanningError {
    /// Returns whether the selected target cannot realize a demanded native representation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::BitcodeTargetContract(_)
                | Self::MissingRuntime
                | Self::Codegen(super::super::super::CodegenPreparationError::UnsupportedType(_))
        )
    }

    /// Returns structured diagnostics produced while deriving native product plans.
    pub const fn diagnostics(&self) -> Option<&DiagnosticBag> {
        match self {
            Self::Codegen(super::super::super::CodegenPreparationError::Diagnostics(
                diagnostics,
            )) => Some(diagnostics),
            _ => None,
        }
    }

    /// Returns whether plan evaluation observed cancellation rather than a terminal failure.
    pub const fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Query(FactQueryError::Cancelled)
                | Self::Codegen(super::super::super::CodegenPreparationError::Query(
                    FactQueryError::Cancelled
                ))
        )
    }

    /// Converts one terminal native-product planning failure with exact product and target context.
    pub fn diagnostic(&self, product: &ProductIdentity, target: &str) -> Option<DiagnosticBag> {
        let failure = native_product_failure_kind(self)?;

        let outer = DiagnosticBag::single(native_product_preparation_diagnostic(
            failure, product, target,
        ));

        match self {
            Self::StandardLibrary(error) => {
                let (cause, artifact_path) =
                    super::super::super::imported::standard_library_failure_diagnostic(
                        error.clone(),
                    );

                let cause = if let Some(artifact_path) = artifact_path.as_deref() {
                    super::super::super::imported::with_standard_library_product_context(
                        cause,
                        product,
                        artifact_path,
                    )
                } else {
                    cause
                };

                Some(DiagnosticBag::single(cause).merged(&outer))
            }
            _ => Some(outer),
        }
    }
}

pub(in crate::compilation) fn native_product_preparation_diagnostic(
    failure: DiagnosticNativeProductFailureKind,
    product: &ProductIdentity,
    target: &str,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::NativeProductPreparationFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target))
    .with_arg(DiagnosticArg::native_product_failure_kind(failure.clone()));

    match failure {
        DiagnosticNativeProductFailureKind::EvaluationLoweringInput(failure) => {
            super::super::super::diagnostics::with_compiler_defect_source(
                diagnostic,
                failure.source(),
            )
        }
        DiagnosticNativeProductFailureKind::EvaluationLowering(failure) => {
            super::super::super::diagnostics::with_compiler_defect_source(
                diagnostic,
                failure.source(),
            )
        }
        DiagnosticNativeProductFailureKind::CodegenMirUnavailable => diagnostic.with_note(
            DiagnosticNote::new(DiagnosticNoteKind::NativeProductPreparationRecovery),
        ),
        _ => diagnostic,
    }
}

fn native_product_failure_kind(
    error: &NativeProductPlanningError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        NativeProductPlanningError::CodegenUnavailable => Kind::CodegenBackendNotSelected,
        NativeProductPlanningError::BitcodeTargetContract(_) => Kind::CodegenBackendUnavailable,
        NativeProductPlanningError::MissingProductRoot => Kind::MissingProductRoot,
        NativeProductPlanningError::InvalidEntryResult => Kind::InvalidEntryResult,
        NativeProductPlanningError::MissingRuntime => Kind::MissingRuntime,
        NativeProductPlanningError::LibraryCleanupRequiresMainThread => {
            Kind::LibraryCleanupRequiresMainThread
        }
        NativeProductPlanningError::InvalidSymbolName => Kind::InvalidSymbolName,
        NativeProductPlanningError::InvalidNativeLinkInput(error) => {
            Kind::InvalidNativeLinkInput(diagnostic_native_link_input_failure(error))
        }
        NativeProductPlanningError::Query(error) => fact_query_failure_kind(error)?,
        NativeProductPlanningError::InvalidCodegenTarget(error) => match error {
            CodegenTargetBuildError::UnsupportedProfile => Kind::CodegenTargetUnsupportedProfile,
            CodegenTargetBuildError::EmptyTriple => Kind::CodegenTargetEmptyTriple,
            CodegenTargetBuildError::EmptyCpu => Kind::CodegenTargetEmptyCpu,
            CodegenTargetBuildError::EmptyFeature => Kind::CodegenTargetEmptyFeature,
        },
        NativeProductPlanningError::InvalidReachability(error) => match error {
            CodegenReachabilityBuildError::EmptyRoots => Kind::ReachabilityEmptyRoots,
            CodegenReachabilityBuildError::DuplicateInstance => Kind::ReachabilityDuplicateInstance,
            CodegenReachabilityBuildError::InstanceWasNotDemanded => {
                Kind::ReachabilityUndemandedInstance
            }
            CodegenReachabilityBuildError::Incomplete => Kind::ReachabilityIncomplete,
        },
        NativeProductPlanningError::InvalidCodegenInstance(error) => match error {
            CodegenInstanceBuildError::TemplateMismatch => Kind::InstanceTemplateMismatch,
            CodegenInstanceBuildError::TargetMismatch => Kind::InstanceTargetMismatch,
            CodegenInstanceBuildError::DependencyTargetMismatch => {
                Kind::InstanceDependencyTargetMismatch
            }
        },
        NativeProductPlanningError::InvalidCodegenUnit(error) => codegen_unit_failure_kind(*error),
        NativeProductPlanningError::InvalidCodegenPartition(error) => match error {
            CodegenPartitionError::MissingCompatibility(_) => Kind::PartitionMissingCompatibility,
            CodegenPartitionError::InvalidUnit(_) => Kind::PartitionInvalidUnit,
        },
        NativeProductPlanningError::InvalidHostMir(_) => Kind::GeneratedHostMirInvalid,
        NativeProductPlanningError::InvalidExecutableHost(error) => match error {
            ExecutableHostContractBuildError::DuplicateRole(_) => Kind::ExecutableHostDuplicateRole,
            ExecutableHostContractBuildError::MissingRuntime => Kind::ExecutableHostMissingRuntime,
            ExecutableHostContractBuildError::RuntimeOwnedHostBinding(_) => {
                Kind::ExecutableHostRuntimeOwnedBinding
            }
            ExecutableHostContractBuildError::IncompatibleRuntime(_) => {
                Kind::ExecutableHostIncompatibleRuntime
            }
            ExecutableHostContractBuildError::MissingMainThreadLaneCapability => {
                Kind::ExecutableHostMissingMainThreadLane
            }
            ExecutableHostContractBuildError::MissingProtectedFrameAbi => {
                Kind::ExecutableHostMissingProtectedFrameAbi
            }
            ExecutableHostContractBuildError::MissingRole(_) => Kind::ExecutableHostMissingRole,
        },
        NativeProductPlanningError::InvalidRuntimeSelection(error) => match error {
            RuntimeArtifactSelectionError::IncompatibleRuntime(_) => {
                Kind::RuntimeSelectionIncompatible
            }
            RuntimeArtifactSelectionError::MissingRoleOwner(_) => {
                Kind::RuntimeSelectionMissingRoleOwner
            }
            RuntimeArtifactSelectionError::MissingCapabilityOwner(_) => {
                Kind::RuntimeSelectionMissingCapabilityOwner
            }
            RuntimeArtifactSelectionError::UnreadableArchive { .. } => {
                Kind::RuntimeSelectionUnreadableArchive
            }
            RuntimeArtifactSelectionError::InvalidArchive { .. } => {
                Kind::RuntimeSelectionInvalidArchive
            }
            RuntimeArtifactSelectionError::ArchiveDigestMismatch { .. } => {
                Kind::RuntimeSelectionArchiveDigestMismatch
            }
        },
        NativeProductPlanningError::InvalidEmissionBackend(
            EmissionBackendBuildError::DuplicateCodegenUnit,
        ) => Kind::EmissionBackendDuplicateUnit,
        NativeProductPlanningError::InvalidLinkTarget(LinkTargetBuildError::EmptyTriple) => {
            Kind::LinkTargetEmptyTriple
        }
        NativeProductPlanningError::StandardLibrary(_) => Kind::StandardLibraryUnavailable,
        NativeProductPlanningError::Codegen(error) => codegen_preparation_failure_kind(error)?,
    })
}

fn diagnostic_native_link_input_failure(
    error: &NativeLinkInputPlanningError,
) -> bray_diagnostics::DiagnosticNativeLinkInputFailure {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as DiagnosticFailure;

    match error {
        NativeLinkInputPlanningError::UnsupportedStandardLibraryArtifact { path, kind } => {
            DiagnosticFailure::UnsupportedStandardLibraryArtifact {
                path: path.to_string_lossy().into_owned(),
                artifact_kind: format!("{kind:?}"),
            }
        }
        NativeLinkInputPlanningError::InvalidStandardLibraryArtifact { path, kind, cause } => {
            DiagnosticFailure::InvalidStandardLibraryArtifact {
                path: path.to_string_lossy().into_owned(),
                input_kind: format!("{kind:?}"),
                cause: format!("{cause:?}"),
            }
        }
        NativeLinkInputPlanningError::InvalidRequirement {
            name,
            kind,
            provenance,
        } => DiagnosticFailure::InvalidRequirement {
            name: name.clone(),
            link_kind: kind.as_str().to_owned(),
            provenance: format!("{provenance:?}"),
        },
    }
}

const fn codegen_unit_failure_kind(
    error: CodegenUnitBuildError,
) -> DiagnosticNativeProductFailureKind {
    use DiagnosticNativeProductFailureKind as Kind;

    match error {
        CodegenUnitBuildError::Empty => Kind::UnitEmpty,
        CodegenUnitBuildError::DuplicateInstance => Kind::UnitDuplicateInstance,
        CodegenUnitBuildError::MissingCompatibility => Kind::UnitMissingCompatibility,
        CodegenUnitBuildError::TargetMismatch => Kind::UnitTargetMismatch,
        CodegenUnitBuildError::WorkBoundExceeded => Kind::UnitWorkBoundExceeded,
        CodegenUnitBuildError::RecipeMismatch => Kind::UnitRecipeMismatch,
    }
}

pub(in crate::compilation) fn codegen_preparation_failure_kind(
    error: &super::super::super::CodegenPreparationError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use super::super::super::CodegenPreparationError;
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        CodegenPreparationError::CodegenUnavailable => Kind::CodegenBackendUnavailable,
        CodegenPreparationError::InvalidRequest(_) => Kind::CodegenInvalidRequest,
        CodegenPreparationError::MirUnavailable(_) => Kind::CodegenMirUnavailable,
        CodegenPreparationError::MissingEntrypoint => Kind::CodegenMissingEntrypoint,
        CodegenPreparationError::InvalidInstance(_) => Kind::CodegenInvalidInstance,
        CodegenPreparationError::InvalidUnit(_) => Kind::CodegenInvalidUnit,
        CodegenPreparationError::UnitMismatch(_) => Kind::CodegenUnitMismatch,
        CodegenPreparationError::InvalidHostMir(_) => Kind::CodegenInvalidHostMir,
        CodegenPreparationError::InvalidGeneratedLifecycleMir(_) => {
            Kind::CodegenInvalidLifecycleMir
        }
        CodegenPreparationError::InvalidMappings(_) => Kind::CodegenInvalidMappings,
        CodegenPreparationError::MissingRuntimeRole(_) => Kind::CodegenMissingRuntimeRole,
        CodegenPreparationError::InvalidRuntimeRoleSourceBinding { .. } => {
            Kind::CodegenInvalidAbiMapping
        }
        CodegenPreparationError::OpenConstantTerm(_) => Kind::CodegenOpenConstantTerm,
        CodegenPreparationError::InvalidArrayLength(_) => Kind::CodegenInvalidArrayLength,
        CodegenPreparationError::RecursiveValueType(_) => Kind::CodegenRecursiveValueType,
        CodegenPreparationError::UnresolvedType(_) => Kind::CodegenUnresolvedType,
        CodegenPreparationError::UnsizedTypeByValue(_) => Kind::CodegenUnsizedTypeByValue,
        CodegenPreparationError::InvalidAbiMapping => Kind::CodegenInvalidAbiMapping,
        CodegenPreparationError::UnsupportedType(_) => Kind::CodegenUnsupportedType,
        CodegenPreparationError::MissingHelperInstance(_) => Kind::CodegenMissingHelperInstance,
        CodegenPreparationError::Diagnostics(_) => return None,
        CodegenPreparationError::LayoutOverflow(_) => Kind::CodegenLayoutOverflow,
        CodegenPreparationError::InvalidSymbolName => Kind::CodegenInvalidSymbolName,
        CodegenPreparationError::Query(error) => fact_query_failure_kind(error)?,
    })
}

fn fact_query_failure_kind(error: &FactQueryError) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    match error {
        FactQueryError::Cancelled => None,
        FactQueryError::Cycle(_) => Some(Kind::EvaluationCycle),
        FactQueryError::InfrastructureFailure | FactQueryError::Runtime(_) => {
            Some(Kind::EvaluationInfrastructure)
        }
        FactQueryError::SemanticValueStoreCreate(_) => {
            Some(Kind::EvaluationSemanticValueStoreCreate)
        }
        FactQueryError::SemanticValueStore(error) => Some(Kind::EvaluationSemanticValue(
            crate::fact::diagnostic_semantic_value_failure(*error),
        )),
        FactQueryError::BindingDependencyUnavailable => Some(Kind::EvaluationBinding(
            bray_diagnostics::DiagnosticBindingFailure::DependencyUnavailable,
        )),
        FactQueryError::Binding(error) => Some(Kind::EvaluationBinding(
            crate::fact::diagnostic_binding_failure(error),
        )),
        FactQueryError::LoweringInput(error) => Some(Kind::EvaluationLoweringInput(
            super::super::super::lowering_diagnostic::lowering_input_failure(error),
        )),
        FactQueryError::Lowering(error) => Some(Kind::EvaluationLowering(
            super::super::super::lowering_diagnostic::lowering_failure(error),
        )),
        FactQueryError::ConstantCallableBodyUnavailable => {
            Some(Kind::EvaluationConstantCallableBodyUnavailable)
        }
        FactQueryError::ConstantCallableRootUnavailable => {
            Some(Kind::EvaluationConstantCallableRootUnavailable)
        }
        FactQueryError::AtomicInitializerArgumentUnavailable => {
            Some(Kind::EvaluationAtomicInitializerArgumentUnavailable)
        }
        FactQueryError::AtomicInitializerResultUnavailable => {
            Some(Kind::EvaluationAtomicInitializerResultUnavailable)
        }
        FactQueryError::UninitInitializerResultUnavailable => {
            Some(Kind::EvaluationUninitInitializerResultUnavailable)
        }
        FactQueryError::ImportedExecutableTemplateMismatch => {
            Some(Kind::EvaluationImportedExecutableTemplateMismatch)
        }
        FactQueryError::SemanticUnitContext(_) => Some(Kind::SemanticContextFailure),
        FactQueryError::SemanticQuery(_) => Some(Kind::SemanticContextFailure),
        FactQueryError::Product(error) => Some(Kind::EvaluationProduct(
            super::super::super::product_emission::diagnostics::product_query::diagnostic_product_query_failure(error),
        )),
        FactQueryError::Foreign(error) => Some(Kind::EvaluationForeign(
            super::super::super::product_emission::diagnostics::foreign_query::diagnostic_foreign_query_failure(error),
        )),
        FactQueryError::CheckerInfrastructure(error) => Some(match error {
            bray_checker::CheckerInfrastructureError::AtomicRepresentationTypeUnavailable => {
                Kind::EvaluationAtomicRepresentationTypeUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicRepresentationArgumentsUnavailable => {
                Kind::EvaluationAtomicRepresentationArgumentsUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerArgumentUnavailable => {
                Kind::EvaluationAtomicInitializerArgumentUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerResultUnavailable => {
                Kind::EvaluationAtomicInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::UninitInitializerResultUnavailable => {
                Kind::EvaluationUninitInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::ImportedExecutableTemplateMismatch => {
                Kind::EvaluationImportedExecutableTemplateMismatch
            }
            error => Kind::EvaluationChecker(crate::fact::diagnostic_checker_failure(*error)),
        }),
    }
}

impl From<FactQueryError> for NativeProductPlanningError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<super::super::super::CodegenPreparationError> for NativeProductPlanningError {
    fn from(error: super::super::super::CodegenPreparationError) -> Self {
        Self::Codegen(error)
    }
}

#[cfg(test)]
mod tests {
    use bray_checker::CheckerInfrastructureError;
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticLabelKind,
        DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
        DiagnosticLoweringInputFailureKind, DiagnosticNativeProductFailureKind, DiagnosticNoteKind,
        DiagnosticSemanticValueFailure,
    };
    use bray_lowering::{LoweringError, LoweringInputError};
    use bray_messages::DiagnosticRenderer;
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::{
        PackageIdentity, ProductIdentity, SemanticValueKind, SemanticValueStore,
        SemanticValueStoreError,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{
        NativeProductPlanningError, fact_query_failure_kind, native_product_preparation_diagnostic,
    };
    use crate::LocatedLoweringFailure;
    use crate::fact::FactQueryError;

    #[test]
    fn native_product_evaluation_failures_preserve_specific_reasons() {
        use DiagnosticNativeProductFailureKind as Kind;

        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let first_store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("first semantic store must build: {error:?}"));

        let second_store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("second semantic store must build: {error:?}"));

        let foreign = SemanticValueStoreError::ForeignId {
            expected: first_store.id(),
            actual: second_store.id(),
        };

        let capacity = SemanticValueStoreError::CapacityExhausted {
            kind: SemanticValueKind::ConstantValue,
        };

        let cases = [
            (
                FactQueryError::ConstantCallableBodyUnavailable,
                Kind::EvaluationConstantCallableBodyUnavailable,
            ),
            (
                FactQueryError::ConstantCallableRootUnavailable,
                Kind::EvaluationConstantCallableRootUnavailable,
            ),
            (
                FactQueryError::CheckerInfrastructure(
                    CheckerInfrastructureError::AtomicRepresentationTypeUnavailable,
                ),
                Kind::EvaluationAtomicRepresentationTypeUnavailable,
            ),
            (
                FactQueryError::CheckerInfrastructure(
                    CheckerInfrastructureError::AtomicRepresentationArgumentsUnavailable,
                ),
                Kind::EvaluationAtomicRepresentationArgumentsUnavailable,
            ),
            (
                FactQueryError::AtomicInitializerArgumentUnavailable,
                Kind::EvaluationAtomicInitializerArgumentUnavailable,
            ),
            (
                FactQueryError::AtomicInitializerResultUnavailable,
                Kind::EvaluationAtomicInitializerResultUnavailable,
            ),
            (
                FactQueryError::UninitInitializerResultUnavailable,
                Kind::EvaluationUninitInitializerResultUnavailable,
            ),
            (
                FactQueryError::ImportedExecutableTemplateMismatch,
                Kind::EvaluationImportedExecutableTemplateMismatch,
            ),
            (
                FactQueryError::LoweringInput(LocatedLoweringFailure::new(
                    LoweringInputError::InvalidPatternInput,
                    source,
                )),
                Kind::EvaluationLoweringInput(DiagnosticLoweringInputFailure::new(
                    DiagnosticLoweringInputFailureKind::InvalidPatternInput,
                    source,
                )),
            ),
            (
                FactQueryError::LoweringInput(LocatedLoweringFailure::new(
                    LoweringInputError::SemanticValue(foreign),
                    source,
                )),
                Kind::EvaluationLoweringInput(DiagnosticLoweringInputFailure::new(
                    DiagnosticLoweringInputFailureKind::SemanticValue(
                        DiagnosticSemanticValueFailure::ForeignId {
                            expected_store: first_store.id().raw(),
                            actual_store: second_store.id().raw(),
                        },
                    ),
                    source,
                )),
            ),
            (
                FactQueryError::Lowering(LocatedLoweringFailure::new(
                    LoweringError::InvalidFrameDescriptor,
                    source,
                )),
                Kind::EvaluationLowering(DiagnosticLoweringFailure::new(
                    DiagnosticLoweringFailureKind::InvalidFrameDescriptor,
                    source,
                )),
            ),
            (
                FactQueryError::Lowering(LocatedLoweringFailure::new(
                    LoweringError::SemanticValue(capacity),
                    source,
                )),
                Kind::EvaluationLowering(DiagnosticLoweringFailure::new(
                    DiagnosticLoweringFailureKind::SemanticValue(
                        DiagnosticSemanticValueFailure::CapacityExhausted {
                            kind: "constant_value",
                        },
                    ),
                    source,
                )),
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(fact_query_failure_kind(&error), Some(expected));
        }
    }

    #[test]
    fn native_product_failures_preserve_exact_product_target_and_reason() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostics = NativeProductPlanningError::MissingRuntime
            .diagnostic(&product, "x86_64-pc-windows-msvc")
            .unwrap_or_else(|| panic!("terminal native product failure must diagnose"));

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::NativeProductPreparationFailed,
        );

        let diagnostic = diagnostics
            .iter()
            .next()
            .unwrap_or_else(|| panic!("test diagnostic must exist"));

        assert!(diagnostic.args().iter().any(|arg| {
            arg.name() == DiagnosticArgName::NativeProductFailureKind
                && matches!(
                    arg.value(),
                    DiagnosticArgValue::NativeProductFailureKind(
                        DiagnosticNativeProductFailureKind::MissingRuntime
                    )
                )
        }));

        assert!(diagnostic.notes().is_empty());
    }

    #[test]
    fn standard_library_failures_preserve_product_target_and_exact_cause() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let target = bray_target::TargetIdentity::try_new("x86_64-pc-windows-msvc")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let diagnostics = NativeProductPlanningError::StandardLibrary(
            bray_standard_library::StandardLibraryLoadError::OptimizationUnavailable {
                target: target.clone(),
            },
        )
        .diagnostic(&product, target.as_str())
        .unwrap_or_else(|| panic!("standard-library planning failure must diagnose"));

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::StandardLibraryOptimizationUnavailable
        }));

        let outer = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::NativeProductPreparationFailed)
            .unwrap_or_else(|| panic!("product-context diagnostic must exist"));

        assert!(outer.args().iter().any(|arg| {
            matches!(
                arg.value(),
                DiagnosticArgValue::NativeProductFailureKind(
                    DiagnosticNativeProductFailureKind::StandardLibraryUnavailable
                )
            )
        }));

        assert_eq!(
            DiagnosticRenderer::english().render(outer).message(),
            "cannot prepare native product 'example/application' for target 'x86_64-pc-windows-msvc': the configured standard library cannot supply a required native artifact"
        );

        let artifact_path = std::path::PathBuf::from("targets/test/libstd.a");

        let infrastructure = NativeProductPlanningError::StandardLibrary(
            bray_standard_library::StandardLibraryLoadError::Infrastructure {
                path: artifact_path.clone(),
            },
        )
        .diagnostic(&product, target.as_str())
        .unwrap_or_else(|| panic!("standard-library infrastructure failure must diagnose"));

        let cause = infrastructure
            .iter()
            .find(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::StandardLibraryInfrastructureFailure
            })
            .unwrap_or_else(|| panic!("standard-library infrastructure cause must exist"));

        assert_eq!(cause.args(), &[DiagnosticArg::artifact_path(artifact_path)]);
        bray_testing::assert_goal_state_diagnostic(cause);
    }

    #[test]
    fn unavailable_native_program_provides_recovery_guidance() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostic = native_product_preparation_diagnostic(
            DiagnosticNativeProductFailureKind::CodegenMirUnavailable,
            &product,
            "x86_64-pc-windows-msvc",
        );

        assert_eq!(diagnostic.notes().len(), 1);

        assert_eq!(
            diagnostic.notes()[0].kind(),
            DiagnosticNoteKind::NativeProductPreparationRecovery
        );
    }

    #[test]
    fn compiler_owned_code_production_failures_are_explicit_and_source_anchored() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let source = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let diagnostic = native_product_preparation_diagnostic(
            DiagnosticNativeProductFailureKind::EvaluationLowering(DiagnosticLoweringFailure::new(
                DiagnosticLoweringFailureKind::MissingSuspensionPoint,
                source,
            )),
            &product,
            "x86_64-pc-windows-msvc",
        );

        assert_eq!(diagnostic.primary_span(), Some(source));

        assert!(diagnostic.labels().iter().any(|label| {
            label.kind() == DiagnosticLabelKind::CompilerDefectSource && label.span() == source
        }));

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| note.kind() == DiagnosticNoteKind::ReportCompilerDefect)
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);
        let message = rendered.message();

        assert_eq!(
            message,
            "cannot prepare native product 'example/application' for target 'x86_64-pc-windows-msvc': an internal compiler error prevented Bray from generating a valid resume path for the highlighted `await` expression"
        );

        for forbidden in ["MIR", "lowering", "node", "frame descriptor", "terminator"] {
            assert!(!message.contains(forbidden));
        }
    }

    #[test]
    fn code_production_node_failures_name_the_highlighted_syntax_category() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let source = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let diagnostic = native_product_preparation_diagnostic(
            DiagnosticNativeProductFailureKind::EvaluationLowering(DiagnosticLoweringFailure::new(
                DiagnosticLoweringFailureKind::MissingSourceNode(
                    bray_diagnostics::DiagnosticSourceConstructKind::Pattern,
                ),
                source,
            )),
            &product,
            "x86_64-pc-windows-msvc",
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "cannot prepare native product 'example/application' for target 'x86_64-pc-windows-msvc': an internal compiler error prevented Bray from generating executable code for the highlighted pattern"
        );

        assert!(!rendered.message().contains("node"));
    }

    #[test]
    fn code_production_input_failures_retain_explanatory_values() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let source = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let diagnostic = native_product_preparation_diagnostic(
            DiagnosticNativeProductFailureKind::EvaluationLoweringInput(
                DiagnosticLoweringInputFailure::new(
                    DiagnosticLoweringInputFailureKind::StorageOperationCountMismatch {
                        expected: 3,
                        actual: 2,
                    },
                    source,
                ),
            ),
            &product,
            "x86_64-pc-windows-msvc",
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "cannot prepare native product 'example/application' for target 'x86_64-pc-windows-msvc': an internal compiler error prevented Bray from reconciling the highlighted declaration's 2 value accesses with the 3 required by ownership analysis"
        );

        assert_eq!(diagnostic.primary_span(), Some(source));

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| note.kind() == DiagnosticNoteKind::ReportCompilerDefect)
        );
    }
}
