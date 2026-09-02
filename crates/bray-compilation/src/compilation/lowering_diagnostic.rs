use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{
    DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
    DiagnosticLoweringInputFailureKind, DiagnosticMirUnitBuildFailure,
    DiagnosticMirUnitBuildFailureContext, DiagnosticMirUnitBuildFailureKind,
    DiagnosticMirUnitLocalIdentity, DiagnosticSourceConstructKind,
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
    use DiagnosticMirUnitBuildFailureContext as Context;
    use DiagnosticMirUnitBuildFailureKind as Kind;

    match error {
        MirUnitBuildError::SourceOriginMismatch => mir_failure(Kind::SourceOriginMismatch),
        MirUnitBuildError::IdentityCapacityExceeded => mir_failure(Kind::IdentityCapacityExceeded),
        MirUnitBuildError::ForeignBlock { expected, actual } => DiagnosticMirUnitBuildFailure::new(
            Kind::ForeignBlock,
            Context::UnitMismatch {
                expected: expected.raw(),
                actual: actual.raw(),
            },
        ),
        MirUnitBuildError::ForeignOperation(identity) => {
            mir_operation_failure(Kind::ForeignOperation, *identity)
        }
        MirUnitBuildError::ForeignStorage(identity) => {
            mir_storage_failure(Kind::ForeignStorage, *identity)
        }
        MirUnitBuildError::ForeignValue(identity) => {
            mir_value_failure(Kind::ForeignValue, *identity)
        }
        MirUnitBuildError::MissingBlock(identity) => {
            mir_block_failure(Kind::MissingBlock, *identity)
        }
        MirUnitBuildError::MissingOperation(identity) => {
            mir_operation_failure(Kind::MissingOperation, *identity)
        }
        MirUnitBuildError::MissingOperationResult(identity) => {
            mir_operation_failure(Kind::MissingOperationResult, *identity)
        }
        MirUnitBuildError::UnexpectedOperationResult(identity) => {
            mir_operation_failure(Kind::UnexpectedOperationResult, *identity)
        }
        MirUnitBuildError::OperationResultTypeMismatch(identity) => {
            mir_operation_failure(Kind::OperationResultTypeMismatch, *identity)
        }
        MirUnitBuildError::InvalidAggregateOperation(identity) => {
            mir_operation_failure(Kind::InvalidAggregateOperation, *identity)
        }
        MirUnitBuildError::InvalidMemoryOperation(identity) => {
            mir_operation_failure(Kind::InvalidMemoryOperation, *identity)
        }
        MirUnitBuildError::InvalidAnonymousCallable(identity) => {
            mir_operation_failure(Kind::InvalidAnonymousCallable, *identity)
        }
        MirUnitBuildError::InvalidConstructionInput(identity) => {
            mir_operation_failure(Kind::InvalidConstructionInput, *identity)
        }
        MirUnitBuildError::InvalidCall(identity) => {
            mir_operation_failure(Kind::InvalidCall, *identity)
        }
        MirUnitBuildError::InvalidHostOperation(identity) => {
            mir_operation_failure(Kind::InvalidHostOperation, *identity)
        }
        MirUnitBuildError::InvalidHostSequence => mir_failure(Kind::InvalidHostSequence),
        MirUnitBuildError::MissingStorage(identity) => {
            mir_storage_failure(Kind::MissingStorage, *identity)
        }
        MirUnitBuildError::MissingValue(identity) => {
            mir_value_failure(Kind::MissingValue, *identity)
        }
        MirUnitBuildError::DuplicateTerminator(identity) => {
            mir_block_failure(Kind::DuplicateTerminator, *identity)
        }
        MirUnitBuildError::MissingTerminator(identity) => {
            mir_block_failure(Kind::MissingTerminator, *identity)
        }
        MirUnitBuildError::InvalidInlineAssemblyTerminator(identity) => {
            mir_block_failure(Kind::InvalidInlineAssemblyTerminator, *identity)
        }
        MirUnitBuildError::InvalidSuspensionPayload(identity) => {
            mir_block_failure(Kind::InvalidSuspensionPayload, *identity)
        }
        MirUnitBuildError::InvalidCallPanicCheck(identity) => {
            mir_block_failure(Kind::InvalidCallPanicCheck, *identity)
        }
        MirUnitBuildError::EdgeArgumentCountMismatch(identity) => {
            mir_block_failure(Kind::EdgeArgumentCountMismatch, *identity)
        }
        MirUnitBuildError::EdgeArgumentTypeMismatch(identity) => {
            mir_block_failure(Kind::EdgeArgumentTypeMismatch, *identity)
        }
        MirUnitBuildError::DuplicateSwitchCase(identity) => {
            mir_block_failure(Kind::DuplicateSwitchCase, *identity)
        }
        MirUnitBuildError::CleanupTargetMismatch { phase, target } => {
            DiagnosticMirUnitBuildFailure::new(
                Kind::CleanupTargetMismatch,
                Context::CleanupTarget {
                    phase: cleanup_phase(*phase),
                    target: mir_local_identity(target.unit(), target.slot()),
                },
            )
        }
        MirUnitBuildError::CleanupPhaseOrderViolation(identity) => {
            mir_block_failure(Kind::CleanupPhaseOrderViolation, *identity)
        }
        MirUnitBuildError::RuntimeRoleMismatch { expected, actual } => {
            DiagnosticMirUnitBuildFailure::new(
                Kind::RuntimeRoleMismatch,
                Context::RuntimeRoleMismatch {
                    expected: expected.as_str(),
                    actual: actual.as_str(),
                },
            )
        }
        MirUnitBuildError::RuntimeAbiVersionMismatch => {
            mir_failure(Kind::RuntimeAbiVersionMismatch)
        }
        MirUnitBuildError::InvalidOperationBlock(identity) => {
            mir_operation_failure(Kind::InvalidOperationBlock, *identity)
        }
        MirUnitBuildError::StorageKindMismatch(identity) => {
            mir_storage_failure(Kind::StorageKindMismatch, *identity)
        }
        MirUnitBuildError::StorageTypeMismatch(identity) => {
            mir_storage_failure(Kind::StorageTypeMismatch, *identity)
        }
        MirUnitBuildError::ValueDoesNotDominateUse(identity) => {
            mir_value_failure(Kind::ValueDoesNotDominateUse, *identity)
        }
        MirUnitBuildError::ProtectedFrameMismatch => mir_failure(Kind::ProtectedFrameMismatch),
        MirUnitBuildError::MissingFrameDescriptor => mir_failure(Kind::MissingFrameDescriptor),
        MirUnitBuildError::DuplicateFrameDescriptor => mir_failure(Kind::DuplicateFrameDescriptor),
        MirUnitBuildError::UnexpectedFrameDescriptor => {
            mir_failure(Kind::UnexpectedFrameDescriptor)
        }
        MirUnitBuildError::InvalidFrameStateEntry(identity) => {
            mir_block_failure(Kind::InvalidFrameStateEntry, *identity)
        }
        MirUnitBuildError::MissingFrameState => mir_failure(Kind::MissingFrameState),
    }
}

const fn mir_failure(kind: DiagnosticMirUnitBuildFailureKind) -> DiagnosticMirUnitBuildFailure {
    DiagnosticMirUnitBuildFailure::new(kind, DiagnosticMirUnitBuildFailureContext::None)
}

const fn mir_block_failure(
    kind: DiagnosticMirUnitBuildFailureKind,
    identity: bray_ir::MirBlockId,
) -> DiagnosticMirUnitBuildFailure {
    DiagnosticMirUnitBuildFailure::new(
        kind,
        DiagnosticMirUnitBuildFailureContext::Block(mir_local_identity(
            identity.unit(),
            identity.slot(),
        )),
    )
}

const fn mir_operation_failure(
    kind: DiagnosticMirUnitBuildFailureKind,
    identity: bray_ir::MirOperationId,
) -> DiagnosticMirUnitBuildFailure {
    DiagnosticMirUnitBuildFailure::new(
        kind,
        DiagnosticMirUnitBuildFailureContext::Operation(mir_local_identity(
            identity.unit(),
            identity.slot(),
        )),
    )
}

const fn mir_storage_failure(
    kind: DiagnosticMirUnitBuildFailureKind,
    identity: bray_ir::MirStorageId,
) -> DiagnosticMirUnitBuildFailure {
    DiagnosticMirUnitBuildFailure::new(
        kind,
        DiagnosticMirUnitBuildFailureContext::Storage(mir_local_identity(
            identity.unit(),
            identity.slot(),
        )),
    )
}

const fn mir_value_failure(
    kind: DiagnosticMirUnitBuildFailureKind,
    identity: bray_ir::MirValueId,
) -> DiagnosticMirUnitBuildFailure {
    DiagnosticMirUnitBuildFailure::new(
        kind,
        DiagnosticMirUnitBuildFailureContext::Value(mir_local_identity(
            identity.unit(),
            identity.slot(),
        )),
    )
}

const fn mir_local_identity(unit: bray_ir::MirUnitId, slot: u32) -> DiagnosticMirUnitLocalIdentity {
    DiagnosticMirUnitLocalIdentity::new(unit.raw(), slot)
}

const fn cleanup_phase(phase: bray_ir::MirCleanupPhase) -> &'static str {
    match phase {
        bray_ir::MirCleanupPhase::TaskCancellation => "task_cancellation",
        bray_ir::MirCleanupPhase::LifecycleResolution => "lifecycle_resolution",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringInputFailure,
        DiagnosticLoweringInputFailureKind, DiagnosticMirUnitBuildFailure,
        DiagnosticMirUnitBuildFailureContext, DiagnosticMirUnitBuildFailureKind,
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
                DiagnosticLoweringFailureKind::Mir(DiagnosticMirUnitBuildFailure::new(
                    DiagnosticMirUnitBuildFailureKind::InvalidHostSequence,
                    DiagnosticMirUnitBuildFailureContext::None,
                ),),
                source,
            )
        );
    }

    #[test]
    fn mir_failures_retain_unit_local_identity_payloads() {
        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let operation = bray_ir::MirOperationId::from_slot(bray_ir::MirUnitId::new(7), 11);

        let failure = super::lowering_failure(&LocatedLoweringFailure::new(
            LoweringError::Mir(MirUnitBuildError::ForeignOperation(operation)),
            source,
        ));

        assert_eq!(
            failure.kind(),
            DiagnosticLoweringFailureKind::Mir(DiagnosticMirUnitBuildFailure::new(
                DiagnosticMirUnitBuildFailureKind::ForeignOperation,
                DiagnosticMirUnitBuildFailureContext::Operation(
                    bray_diagnostics::DiagnosticMirUnitLocalIdentity::new(7, 11),
                ),
            ))
        );
    }
}
