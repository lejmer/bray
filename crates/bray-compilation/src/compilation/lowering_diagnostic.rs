use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{
    DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringIdentity,
    DiagnosticLoweringInputFailure, DiagnosticLoweringInputFailureKind, DiagnosticLoweringRoot,
    DiagnosticMirUnitBuildFailure,
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
        LoweringInputError::ForeignInput {
            input,
            expected,
            actual,
        } => Kind::ForeignInput {
            input: lowering_input_kind(*input),
            expected_unit: expected.raw(),
            actual_unit: actual.raw(),
        },
        LoweringInputError::InputKindMismatch {
            input,
            expected,
            actual,
        } => Kind::InputKindMismatch {
            input: lowering_input_kind(*input),
            expected_kind: expected.as_str(),
            actual_kind: actual.as_str(),
        },
        LoweringInputError::MissingSemanticSelection(expression) => {
            Kind::MissingSemanticSelection(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringInputError::MissingExpressionType(expression) => {
            Kind::MissingExpressionType(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringInputError::InvalidPatternInput => Kind::InvalidPatternInput,
        LoweringInputError::InvalidInputContents(input) => {
            Kind::InvalidInputContents(lowering_input_kind(*input))
        }
        LoweringInputError::SemanticValue(error) => {
            Kind::SemanticValue(crate::fact::diagnostic_semantic_value_failure(*error))
        }
        LoweringInputError::InvalidStorageOperation(expression) => Kind::InvalidStorageOperation(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringInputError::StorageOperationCountMismatch { expected, actual } => {
            Kind::StorageOperationCountMismatch {
                expected: u64::try_from(*expected).unwrap_or(u64::MAX),
                actual: u64::try_from(*actual).unwrap_or(u64::MAX),
            }
        }
        LoweringInputError::InvalidStorageExit(block) => {
            Kind::InvalidStorageExit(bound_identity(block.unit(), block.ordinal()))
        }
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
        LoweringError::UnsupportedRoot(root) => Kind::UnsupportedRoot(lowering_root(*root)),
        LoweringError::MissingBoundNode(node) => Kind::MissingSourceNode {
            kind: source_construct(*node),
            identity: any_bound_identity(*node),
        },
        LoweringError::RecoveredBoundNode(node) => Kind::RecoveredSourceNode {
            kind: source_construct(*node),
            identity: any_bound_identity(*node),
        },
        LoweringError::MissingExpressionType(expression) => {
            Kind::MissingExpressionType(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::AwaitOutsideProtectedFrame(expression) => Kind::AwaitOutsideProtectedFrame(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringError::MissingSuspensionPoint(expression) => Kind::MissingSuspensionPoint(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringError::InvalidTaskOperation(expression) => Kind::InvalidTaskOperation(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringError::MissingCallableResultType => Kind::MissingCallableResultType,
        LoweringError::MissingLiteralValue(expression) => {
            Kind::MissingLiteralValue(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingSemanticSelection(expression) => Kind::MissingSemanticSelection(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringError::UnsupportedExpression(expression) => {
            Kind::UnsupportedExpression(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::UnsupportedPattern(pattern) => {
            Kind::UnsupportedPattern(bound_identity(pattern.unit(), pattern.ordinal()))
        }
        LoweringError::UnsupportedOperator {
            expression,
            operator,
        } => Kind::UnsupportedOperator {
            expression: bound_identity(expression.unit(), expression.ordinal()),
            operator: operator.as_str(),
        },
        LoweringError::MissingStorageAccess(expression) => {
            Kind::MissingStorageAccess(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingStorageAccessRecord(access) => Kind::MissingStorageAccessRecord(
            bound_identity(access.unit(), access.ordinal()),
        ),
        LoweringError::MissingCleanupPlan(block) => {
            Kind::MissingCleanupPlan(bound_identity(block.unit(), block.ordinal()))
        }
        LoweringError::MissingStorageIdentity(access) => Kind::MissingStorageIdentity(
            bound_identity(access.unit(), access.ordinal()),
        ),
        LoweringError::MissingStorageIdentityRecord(identity) => {
            Kind::MissingStorageIdentityRecord(bound_identity(identity.unit(), identity.ordinal()))
        }
        LoweringError::MissingIterationStorage(expression) => Kind::MissingIterationStorage(
            bound_identity(expression.unit(), expression.ordinal()),
        ),
        LoweringError::UnsupportedStorageAccess(access) => Kind::UnsupportedStorageAccess(
            bound_identity(access.unit(), access.ordinal()),
        ),
        LoweringError::MissingOperationResult(expression) => {
            Kind::MissingOperationResult(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingRepresentation(role) => Kind::MissingRepresentation(role.as_str()),
        LoweringError::SemanticValueUnavailable => Kind::SemanticValueUnavailable,
        LoweringError::SemanticValue(error) => {
            Kind::SemanticValue(crate::fact::diagnostic_semantic_value_failure(*error))
        }
        LoweringError::InvalidFrameDescriptor => Kind::InvalidFrameDescriptor,
        LoweringError::Mir(error) => Kind::Mir(mir_unit_failure(error)),
    };

    DiagnosticLoweringFailure::new(kind, failure.source())
}

const fn bound_identity(
    unit: bray_bound_tree::BoundUnitId,
    ordinal: u32,
) -> DiagnosticLoweringIdentity {
    DiagnosticLoweringIdentity::new(unit.raw(), ordinal)
}

const fn any_bound_identity(node: AnyBoundNodeId) -> DiagnosticLoweringIdentity {
    bound_identity(node.unit(), node.ordinal())
}

const fn lowering_root(root: bray_bound_tree::BoundUnitRoot) -> DiagnosticLoweringRoot {
    use bray_bound_tree::BoundUnitRoot as Root;

    match root {
        Root::CallableBody { execution, body } => DiagnosticLoweringRoot::CallableBody {
            execution: callable_execution(execution),
            body: bound_identity(body.unit(), body.ordinal()),
        },
        Root::AnonymousCallable {
            callable,
            execution,
            body,
        } => DiagnosticLoweringRoot::AnonymousCallable {
            callable_region: callable.region().raw(),
            callable_ordinal: callable.ordinal(),
            execution: callable_execution(execution),
            body: bound_identity(body.unit(), body.ordinal()),
        },
        Root::Expression(expression) => DiagnosticLoweringRoot::Expression(bound_identity(
            expression.unit(),
            expression.ordinal(),
        )),
        Root::ExpressionSequence(block) => DiagnosticLoweringRoot::ExpressionSequence(
            bound_identity(block.unit(), block.ordinal()),
        ),
    }
}

const fn callable_execution(execution: bray_symbols::CallableExecution) -> &'static str {
    match execution {
        bray_symbols::CallableExecution::Synchronous => "synchronous",
        bray_symbols::CallableExecution::Asynchronous => "asynchronous",
    }
}

const fn lowering_input_kind(kind: bray_lowering::LoweringInputKind) -> &'static str {
    use bray_lowering::LoweringInputKind as Kind;

    match kind {
        Kind::ControlFlow => "control_flow",
        Kind::ExpressionTypes => "expression_types",
        Kind::Patterns => "patterns",
        Kind::SemanticSelections => "semantic_selections",
        Kind::LiteralValues => "literal_values",
        Kind::ConstantReferences => "constant_references",
        Kind::StoragePlan => "storage_plan",
        Kind::Liveness => "liveness",
        Kind::Refinements => "refinements",
        Kind::StorageFlow => "storage_flow",
        Kind::DependencyContracts => "dependency_contracts",
        Kind::Async => "async",
        Kind::BodyBehavior => "body_behavior",
    }
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
    fn lowering_contract_failures_retain_non_mir_payloads() {
        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let input = super::lowering_input_failure(&LocatedLoweringFailure::new(
            LoweringInputError::ForeignInput {
                input: bray_lowering::LoweringInputKind::StoragePlan,
                expected: bray_bound_tree::BoundUnitId::new(7),
                actual: bray_bound_tree::BoundUnitId::new(11),
            },
            source,
        ));

        assert_eq!(
            input.kind(),
            DiagnosticLoweringInputFailureKind::ForeignInput {
                input: "storage_plan",
                expected_unit: 7,
                actual_unit: 11,
            }
        );

        let lowering = super::lowering_failure(&LocatedLoweringFailure::new(
            LoweringError::MissingRepresentation(bray_compiler_known::RepresentationRole::Task),
            source,
        ));

        assert_eq!(
            lowering.kind(),
            DiagnosticLoweringFailureKind::MissingRepresentation("Task")
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
