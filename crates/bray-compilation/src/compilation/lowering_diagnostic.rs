use bray_bound_tree::AnyBoundNodeId;
use bray_diagnostics::{
    DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringIdentity,
    DiagnosticLoweringRoot, DiagnosticSourceConstructKind,
};
use bray_lowering::LoweringError;

use crate::LocatedLoweringFailure;

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
        LoweringError::MissingSuspensionPoint(expression) => {
            Kind::MissingSuspensionPoint(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::InvalidTaskOperation(expression) => {
            Kind::InvalidTaskOperation(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingInputCleanup(expression) => {
            Kind::MissingInputCleanup(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingCallableResultType => Kind::MissingCallableResultType,
        LoweringError::InvalidCleanupScopeDepth {
            scope_depth,
            active_scope_count,
            exit,
        } => Kind::InvalidCleanupScopeDepth {
            scope_depth: *scope_depth,
            active_scope_count: *active_scope_count,
            exit: any_bound_identity(*exit),
        },
        LoweringError::MissingScopeExitPlan { scope, exit } => Kind::MissingScopeExitPlan {
            scope: bound_identity(scope.unit(), scope.ordinal()),
            exit: any_bound_identity(*exit),
        },
        LoweringError::MissingLiteralValue(expression) => {
            Kind::MissingLiteralValue(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingSemanticSelection(expression) => {
            Kind::MissingSemanticSelection(bound_identity(expression.unit(), expression.ordinal()))
        }
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
        LoweringError::MissingStorageAccessRecord(access) => {
            Kind::MissingStorageAccessRecord(bound_identity(access.unit(), access.ordinal()))
        }
        LoweringError::MissingStorageIdentity(access) => {
            Kind::MissingStorageIdentity(bound_identity(access.unit(), access.ordinal()))
        }
        LoweringError::MissingStorageIdentityRecord(identity) => {
            Kind::MissingStorageIdentityRecord(bound_identity(identity.unit(), identity.ordinal()))
        }
        LoweringError::MissingIterationStorage(expression) => {
            Kind::MissingIterationStorage(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::UnsupportedStorageAccess(access) => {
            Kind::UnsupportedStorageAccess(bound_identity(access.unit(), access.ordinal()))
        }
        LoweringError::MissingOperationResult(expression) => {
            Kind::MissingOperationResult(bound_identity(expression.unit(), expression.ordinal()))
        }
        LoweringError::MissingRepresentation(role) => Kind::MissingRepresentation(role.as_str()),
        LoweringError::SemanticValueUnavailable => Kind::SemanticValueUnavailable,
        LoweringError::GenericSubstitution(error) => {
            Kind::GenericSubstitution(crate::fact::diagnostic_generic_substitution_failure(*error))
        }
        LoweringError::SemanticValue(error) => {
            Kind::SemanticValue(crate::fact::diagnostic_semantic_value_failure(*error))
        }
        LoweringError::MirCapacity(_) => Kind::MirCapacity,
        LoweringError::MemoryArgumentOrdinalUnrepresentable {
            expression,
            ordinal,
        } => Kind::MemoryArgumentOrdinalUnrepresentable {
            expression: bound_identity(expression.unit(), expression.ordinal()),
            ordinal: u64::from(*ordinal),
        },
        LoweringError::MatchArmOrdinalUnrepresentable {
            expression,
            ordinal,
        } => Kind::MatchArmOrdinalUnrepresentable {
            expression: bound_identity(expression.unit(), expression.ordinal()),
            ordinal: *ordinal,
        },
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

const fn source_construct(node: AnyBoundNodeId) -> DiagnosticSourceConstructKind {
    match node {
        AnyBoundNodeId::Expression(_) => DiagnosticSourceConstructKind::Expression,
        AnyBoundNodeId::Pattern(_) => DiagnosticSourceConstructKind::Pattern,
        AnyBoundNodeId::Block(_) => DiagnosticSourceConstructKind::Block,
        AnyBoundNodeId::CallableBody(_) => DiagnosticSourceConstructKind::CallableBody,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticLoweringFailureKind;
    use bray_ir::MirCapacityError;
    use bray_lowering::LoweringError;
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use crate::LocatedLoweringFailure;

    #[test]
    fn mir_capacity_remains_a_compiler_diagnostic() {
        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let failure = super::lowering_failure(&LocatedLoweringFailure::new(
            LoweringError::MirCapacity(MirCapacityError::IdentityCapacityExceeded),
            source,
        ));

        assert_eq!(failure.kind(), DiagnosticLoweringFailureKind::MirCapacity);
    }
}
