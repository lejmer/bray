use bray_diagnostics::{
    DiagnosticLoweringFailure, DiagnosticLoweringInputFailure, DiagnosticMirUnitBuildFailure,
};
use bray_ir::MirUnitBuildError;
use bray_lowering::{LoweringError, LoweringInputError};

pub(super) const fn lowering_input_failure(
    error: &LoweringInputError,
) -> DiagnosticLoweringInputFailure {
    match error {
        LoweringInputError::ForeignInput { .. } => DiagnosticLoweringInputFailure::ForeignInput,
        LoweringInputError::InputKindMismatch { .. } => {
            DiagnosticLoweringInputFailure::InputKindMismatch
        }
        LoweringInputError::MissingSemanticSelection(_) => {
            DiagnosticLoweringInputFailure::MissingSemanticSelection
        }
        LoweringInputError::MissingExpressionType(_) => {
            DiagnosticLoweringInputFailure::MissingExpressionType
        }
        LoweringInputError::InvalidPatternInput => {
            DiagnosticLoweringInputFailure::InvalidPatternInput
        }
        LoweringInputError::InvalidInputContents(_) => {
            DiagnosticLoweringInputFailure::InvalidInputContents
        }
        LoweringInputError::InvalidStorageOperation(_) => {
            DiagnosticLoweringInputFailure::InvalidStorageOperation
        }
        LoweringInputError::StorageOperationCountMismatch { .. } => {
            DiagnosticLoweringInputFailure::StorageOperationCountMismatch
        }
        LoweringInputError::InvalidStorageExit(_) => {
            DiagnosticLoweringInputFailure::InvalidStorageExit
        }
        LoweringInputError::LiteralTargetWidthMismatch { .. } => {
            DiagnosticLoweringInputFailure::LiteralTargetWidthMismatch
        }
        LoweringInputError::ExecutableHostRequiresSyntheticInput => {
            DiagnosticLoweringInputFailure::ExecutableHostRequiresSyntheticInput
        }
        LoweringInputError::CompileTimeUnitRequiresClassification => {
            DiagnosticLoweringInputFailure::CompileTimeUnitRequiresClassification
        }
    }
}

pub(super) const fn lowering_failure(error: &LoweringError) -> DiagnosticLoweringFailure {
    match error {
        LoweringError::UnsupportedRoot(_) => DiagnosticLoweringFailure::UnsupportedRoot,
        LoweringError::MissingBoundNode(_) => DiagnosticLoweringFailure::MissingBoundNode,
        LoweringError::RecoveredBoundNode(_) => DiagnosticLoweringFailure::RecoveredBoundNode,
        LoweringError::MissingExpressionType(_) => {
            DiagnosticLoweringFailure::MissingExpressionType
        }
        LoweringError::AwaitOutsideProtectedFrame(_) => {
            DiagnosticLoweringFailure::AwaitOutsideProtectedFrame
        }
        LoweringError::MissingSuspensionPoint(_) => {
            DiagnosticLoweringFailure::MissingSuspensionPoint
        }
        LoweringError::InvalidTaskOperation(_) => DiagnosticLoweringFailure::InvalidTaskOperation,
        LoweringError::MissingCallableResultType => {
            DiagnosticLoweringFailure::MissingCallableResultType
        }
        LoweringError::MissingLiteralValue(_) => DiagnosticLoweringFailure::MissingLiteralValue,
        LoweringError::MissingSemanticSelection(_) => {
            DiagnosticLoweringFailure::MissingSemanticSelection
        }
        LoweringError::UnsupportedExpression(_) => {
            DiagnosticLoweringFailure::UnsupportedExpression
        }
        LoweringError::UnsupportedPattern(_) => DiagnosticLoweringFailure::UnsupportedPattern,
        LoweringError::UnsupportedOperator(_) => DiagnosticLoweringFailure::UnsupportedOperator,
        LoweringError::MissingStorageAccess(_) => {
            DiagnosticLoweringFailure::MissingStorageAccess
        }
        LoweringError::MissingStorageAccessRecord(_) => {
            DiagnosticLoweringFailure::MissingStorageAccessRecord
        }
        LoweringError::MissingCleanupPlan(_) => DiagnosticLoweringFailure::MissingCleanupPlan,
        LoweringError::MissingStorageIdentity(_) => {
            DiagnosticLoweringFailure::MissingStorageIdentity
        }
        LoweringError::MissingStorageIdentityRecord(_) => {
            DiagnosticLoweringFailure::MissingStorageIdentityRecord
        }
        LoweringError::MissingIterationStorage(_) => {
            DiagnosticLoweringFailure::MissingIterationStorage
        }
        LoweringError::UnsupportedStorageAccess(_) => {
            DiagnosticLoweringFailure::UnsupportedStorageAccess
        }
        LoweringError::MissingOperationResult(_) => {
            DiagnosticLoweringFailure::MissingOperationResult
        }
        LoweringError::MissingRepresentation(_) => {
            DiagnosticLoweringFailure::MissingRepresentation
        }
        LoweringError::SemanticValueUnavailable => {
            DiagnosticLoweringFailure::SemanticValueUnavailable
        }
        LoweringError::InvalidFrameDescriptor => DiagnosticLoweringFailure::InvalidFrameDescriptor,
        LoweringError::Mir(error) => DiagnosticLoweringFailure::Mir(mir_unit_failure(error)),
    }
}

const fn mir_unit_failure(error: &MirUnitBuildError) -> DiagnosticMirUnitBuildFailure {
    match error {
        MirUnitBuildError::SourceOriginMismatch => {
            DiagnosticMirUnitBuildFailure::SourceOriginMismatch
        }
        MirUnitBuildError::IdentityCapacityExceeded => {
            DiagnosticMirUnitBuildFailure::IdentityCapacityExceeded
        }
        MirUnitBuildError::ForeignBlock { .. } => DiagnosticMirUnitBuildFailure::ForeignBlock,
        MirUnitBuildError::ForeignOperation(_) => DiagnosticMirUnitBuildFailure::ForeignOperation,
        MirUnitBuildError::ForeignStorage(_) => DiagnosticMirUnitBuildFailure::ForeignStorage,
        MirUnitBuildError::ForeignValue(_) => DiagnosticMirUnitBuildFailure::ForeignValue,
        MirUnitBuildError::MissingBlock(_) => DiagnosticMirUnitBuildFailure::MissingBlock,
        MirUnitBuildError::MissingOperation(_) => DiagnosticMirUnitBuildFailure::MissingOperation,
        MirUnitBuildError::MissingOperationResult(_) => {
            DiagnosticMirUnitBuildFailure::MissingOperationResult
        }
        MirUnitBuildError::UnexpectedOperationResult(_) => {
            DiagnosticMirUnitBuildFailure::UnexpectedOperationResult
        }
        MirUnitBuildError::OperationResultTypeMismatch(_) => {
            DiagnosticMirUnitBuildFailure::OperationResultTypeMismatch
        }
        MirUnitBuildError::InvalidAggregateOperation(_) => {
            DiagnosticMirUnitBuildFailure::InvalidAggregateOperation
        }
        MirUnitBuildError::InvalidMemoryOperation(_) => {
            DiagnosticMirUnitBuildFailure::InvalidMemoryOperation
        }
        MirUnitBuildError::InvalidAnonymousCallable(_) => {
            DiagnosticMirUnitBuildFailure::InvalidAnonymousCallable
        }
        MirUnitBuildError::InvalidConstructionInput(_) => {
            DiagnosticMirUnitBuildFailure::InvalidConstructionInput
        }
        MirUnitBuildError::InvalidCall(_) => DiagnosticMirUnitBuildFailure::InvalidCall,
        MirUnitBuildError::InvalidHostOperation(_) => {
            DiagnosticMirUnitBuildFailure::InvalidHostOperation
        }
        MirUnitBuildError::InvalidHostSequence => {
            DiagnosticMirUnitBuildFailure::InvalidHostSequence
        }
        MirUnitBuildError::MissingStorage(_) => DiagnosticMirUnitBuildFailure::MissingStorage,
        MirUnitBuildError::MissingValue(_) => DiagnosticMirUnitBuildFailure::MissingValue,
        MirUnitBuildError::DuplicateTerminator(_) => {
            DiagnosticMirUnitBuildFailure::DuplicateTerminator
        }
        MirUnitBuildError::MissingTerminator(_) => {
            DiagnosticMirUnitBuildFailure::MissingTerminator
        }
        MirUnitBuildError::InvalidInlineAssemblyTerminator(_) => {
            DiagnosticMirUnitBuildFailure::InvalidInlineAssemblyTerminator
        }
        MirUnitBuildError::InvalidSuspensionPayload(_) => {
            DiagnosticMirUnitBuildFailure::InvalidSuspensionPayload
        }
        MirUnitBuildError::InvalidCallPanicCheck(_) => {
            DiagnosticMirUnitBuildFailure::InvalidCallPanicCheck
        }
        MirUnitBuildError::EdgeArgumentCountMismatch(_) => {
            DiagnosticMirUnitBuildFailure::EdgeArgumentCountMismatch
        }
        MirUnitBuildError::EdgeArgumentTypeMismatch(_) => {
            DiagnosticMirUnitBuildFailure::EdgeArgumentTypeMismatch
        }
        MirUnitBuildError::DuplicateSwitchCase(_) => {
            DiagnosticMirUnitBuildFailure::DuplicateSwitchCase
        }
        MirUnitBuildError::CleanupTargetMismatch { .. } => {
            DiagnosticMirUnitBuildFailure::CleanupTargetMismatch
        }
        MirUnitBuildError::CleanupPhaseOrderViolation(_) => {
            DiagnosticMirUnitBuildFailure::CleanupPhaseOrderViolation
        }
        MirUnitBuildError::RuntimeRoleMismatch { .. } => {
            DiagnosticMirUnitBuildFailure::RuntimeRoleMismatch
        }
        MirUnitBuildError::RuntimeAbiVersionMismatch => {
            DiagnosticMirUnitBuildFailure::RuntimeAbiVersionMismatch
        }
        MirUnitBuildError::InvalidOperationBlock(_) => {
            DiagnosticMirUnitBuildFailure::InvalidOperationBlock
        }
        MirUnitBuildError::StorageKindMismatch(_) => {
            DiagnosticMirUnitBuildFailure::StorageKindMismatch
        }
        MirUnitBuildError::StorageTypeMismatch(_) => {
            DiagnosticMirUnitBuildFailure::StorageTypeMismatch
        }
        MirUnitBuildError::ValueDoesNotDominateUse(_) => {
            DiagnosticMirUnitBuildFailure::ValueDoesNotDominateUse
        }
        MirUnitBuildError::ProtectedFrameMismatch => {
            DiagnosticMirUnitBuildFailure::ProtectedFrameMismatch
        }
        MirUnitBuildError::MissingFrameDescriptor => {
            DiagnosticMirUnitBuildFailure::MissingFrameDescriptor
        }
        MirUnitBuildError::DuplicateFrameDescriptor => {
            DiagnosticMirUnitBuildFailure::DuplicateFrameDescriptor
        }
        MirUnitBuildError::UnexpectedFrameDescriptor => {
            DiagnosticMirUnitBuildFailure::UnexpectedFrameDescriptor
        }
        MirUnitBuildError::InvalidFrameStateEntry(_) => {
            DiagnosticMirUnitBuildFailure::InvalidFrameStateEntry
        }
        MirUnitBuildError::MissingFrameState => DiagnosticMirUnitBuildFailure::MissingFrameState,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticLoweringFailure, DiagnosticLoweringInputFailure, DiagnosticMirUnitBuildFailure,
    };
    use bray_ir::MirUnitBuildError;
    use bray_lowering::{LoweringError, LoweringInputError};

    #[test]
    fn lowering_contract_failures_retain_their_exact_categories() {
        assert_eq!(
            super::lowering_input_failure(&LoweringInputError::InvalidPatternInput),
            DiagnosticLoweringInputFailure::InvalidPatternInput
        );

        assert_eq!(
            super::lowering_failure(&LoweringError::Mir(
                MirUnitBuildError::InvalidHostSequence
            )),
            DiagnosticLoweringFailure::Mir(
                DiagnosticMirUnitBuildFailure::InvalidHostSequence
            )
        );
    }
}
