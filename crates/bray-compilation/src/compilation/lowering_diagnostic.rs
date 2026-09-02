use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{
    DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
    DiagnosticLoweringInputFailureKind, DiagnosticMirUnitBuildFailure,
    DiagnosticSourceConstructKind,
};
use bray_ir::MirUnitBuildError;
use bray_lowering::{LoweringError, LoweringInputError};

use crate::LocatedLoweringFailure;

pub(super) fn lowering_input_failure(
    failure: &LocatedLoweringFailure<LoweringInputError>,
) -> DiagnosticLoweringInputFailure {
    use DiagnosticLoweringInputFailureKind as Kind;

    let kind = match failure.cause() {
        LoweringInputError::ForeignInput { .. } => Kind::ForeignInput,
        LoweringInputError::InputKindMismatch { .. } => Kind::InputKindMismatch,
        LoweringInputError::MissingSemanticSelection(_) => Kind::MissingSemanticSelection,
        LoweringInputError::MissingExpressionType(_) => Kind::MissingExpressionType,
        LoweringInputError::InvalidPatternInput => Kind::InvalidPatternInput,
        LoweringInputError::InvalidInputContents(_) => Kind::InvalidInputContents,
        LoweringInputError::SemanticValue(error) => {
            Kind::SemanticValue(crate::fact::diagnostic_semantic_value_failure(*error))
        }
        LoweringInputError::InvalidStorageOperation(_) => Kind::InvalidStorageOperation,
        LoweringInputError::StorageOperationCountMismatch { expected, actual } => {
            Kind::StorageOperationCountMismatch {
                expected: u64::try_from(*expected).unwrap_or(u64::MAX),
                actual: u64::try_from(*actual).unwrap_or(u64::MAX),
            }
        }
        LoweringInputError::InvalidStorageExit(_) => Kind::InvalidStorageExit,
        LoweringInputError::LiteralTargetWidthMismatch { expected, actual } => {
            Kind::LiteralTargetWidthMismatch {
                expected: expected.get(),
                actual: actual.get(),
            }
        }
        LoweringInputError::ExecutableHostRequiresSyntheticInput => {
            Kind::ExecutableHostRequiresSyntheticInput
        }
        LoweringInputError::CompileTimeUnitRequiresClassification => {
            Kind::CompileTimeUnitRequiresClassification
        }
    };

    DiagnosticLoweringInputFailure::new(kind, failure.source())
}

pub(super) fn lowering_failure(
    failure: &LocatedLoweringFailure<LoweringError>,
) -> DiagnosticLoweringFailure {
    use DiagnosticLoweringFailureKind as Kind;

    let kind = match failure.cause() {
        LoweringError::UnsupportedRoot(_) => Kind::UnsupportedRoot,
        LoweringError::MissingBoundNode(node) => Kind::MissingSourceNode(source_construct(*node)),
        LoweringError::RecoveredBoundNode(node) => {
            Kind::RecoveredSourceNode(source_construct(*node))
        }
        LoweringError::MissingExpressionType(_) => Kind::MissingExpressionType,
        LoweringError::AwaitOutsideProtectedFrame(_) => Kind::AwaitOutsideProtectedFrame,
        LoweringError::MissingSuspensionPoint(_) => Kind::MissingSuspensionPoint,
        LoweringError::InvalidTaskOperation(_) => Kind::InvalidTaskOperation,
        LoweringError::MissingCallableResultType => Kind::MissingCallableResultType,
        LoweringError::MissingLiteralValue(_) => Kind::MissingLiteralValue,
        LoweringError::MissingSemanticSelection(_) => Kind::MissingSemanticSelection,
        LoweringError::UnsupportedExpression(_) => Kind::UnsupportedExpression,
        LoweringError::UnsupportedPattern(_) => Kind::UnsupportedPattern,
        LoweringError::UnsupportedOperator { .. } => Kind::UnsupportedOperator,
        LoweringError::MissingStorageAccess(_) => Kind::MissingStorageAccess,
        LoweringError::MissingStorageAccessRecord(_) => Kind::MissingStorageAccessRecord,
        LoweringError::MissingCleanupPlan(_) => Kind::MissingCleanupPlan,
        LoweringError::MissingStorageIdentity(_) => Kind::MissingStorageIdentity,
        LoweringError::MissingStorageIdentityRecord(_) => Kind::MissingStorageIdentityRecord,
        LoweringError::MissingIterationStorage(_) => Kind::MissingIterationStorage,
        LoweringError::UnsupportedStorageAccess(_) => Kind::UnsupportedStorageAccess,
        LoweringError::MissingOperationResult(_) => Kind::MissingOperationResult,
        LoweringError::MissingRepresentation(_) => Kind::MissingRepresentation,
        LoweringError::SemanticValueUnavailable => Kind::SemanticValueUnavailable,
        LoweringError::SemanticValue(error) => {
            Kind::SemanticValue(crate::fact::diagnostic_semantic_value_failure(*error))
        }
        LoweringError::InvalidFrameDescriptor => Kind::InvalidFrameDescriptor,
        LoweringError::Mir(error) => Kind::Mir(mir_unit_failure(error)),
    };

    DiagnosticLoweringFailure::new(kind, failure.source())
}

const fn source_construct(node: AnyBoundNodeId) -> DiagnosticSourceConstructKind {
    match node {
        AnyBoundNodeId::Expression(_) => DiagnosticSourceConstructKind::Expression,
        AnyBoundNodeId::Pattern(_) => DiagnosticSourceConstructKind::Pattern,
        AnyBoundNodeId::Block(_) => DiagnosticSourceConstructKind::Block,
        AnyBoundNodeId::CallableBody(_) => DiagnosticSourceConstructKind::CallableBody,
    }
}

pub(crate) const fn mir_unit_failure(error: &MirUnitBuildError) -> DiagnosticMirUnitBuildFailure {
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
        MirUnitBuildError::MissingTerminator(_) => DiagnosticMirUnitBuildFailure::MissingTerminator,
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
        DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
        DiagnosticLoweringInputFailureKind, DiagnosticMirUnitBuildFailure,
    };
    use bray_ir::MirUnitBuildError;
    use bray_lowering::{LoweringError, LoweringInputError};
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use crate::LocatedLoweringFailure;

    #[test]
    fn lowering_contract_failures_retain_their_exact_categories() {
        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        assert_eq!(
            super::lowering_input_failure(&LocatedLoweringFailure::new(
                LoweringInputError::InvalidPatternInput,
                source,
            )),
            DiagnosticLoweringInputFailure::new(
                DiagnosticLoweringInputFailureKind::InvalidPatternInput,
                source,
            )
        );

        assert_eq!(
            super::lowering_failure(&LocatedLoweringFailure::new(
                LoweringError::Mir(MirUnitBuildError::InvalidHostSequence),
                source,
            )),
            DiagnosticLoweringFailure::new(
                DiagnosticLoweringFailureKind::Mir(
                    DiagnosticMirUnitBuildFailure::InvalidHostSequence,
                ),
                source,
            )
        );
    }
}
