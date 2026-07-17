use bray_bound_tree::{
    AnyBoundNodeId, BoundUnit, BoundUnitRoot, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
    CheckedExpressionFactEntry, CheckedExpressionFactInput, CheckedExpressionFacts,
    CheckedExpressionResult, CheckedExpressionStatus, walk_bound_tree,
};
use bray_symbols::TypeData;

use crate::{CheckerOutcome, UnitCheckRequest};

/// Complete expression facts established for one bound semantic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpressionFactCheckResult {
    facts: CheckedExpressionFacts,
}

impl ExpressionFactCheckResult {
    pub(crate) const fn new(facts: CheckedExpressionFacts) -> Self {
        Self { facts }
    }

    /// Returns the durable expression facts established by this check.
    pub fn into_facts(self) -> CheckedExpressionFacts {
        self.facts
    }
}

pub(crate) fn check_expression_facts(
    unit: &BoundUnit,
    request: UnitCheckRequest<'_>,
) -> CheckerOutcome<ExpressionFactCheckResult> {
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let mut results = Vec::new();
    let mut cancelled = false;
    let outcome = walk_bound_tree(unit.tree(), root_node(unit.root()), |event| {
        if request.is_cancelled() {
            cancelled = true;

            return BoundWalkControl::Stop;
        }

        if let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event {
            let Some(bound) = unit.tree().expression(expression) else {
                panic!("bound-tree traversal yielded an absent expression");
            };
            let (ty, status) = match bound.ty() {
                Some(ty) if !bound.is_recovered() => (ty, CheckedExpressionStatus::Valid),
                Some(ty) => (ty, CheckedExpressionStatus::Recovered),
                None => {
                    let ty = match request.semantic_values().intern_type(TypeData::Error) {
                        Ok(ty) => ty,
                        Err(error) => {
                            panic!(
                                "canonical error type must be available for recovery: {error:?}"
                            );
                        }
                    };

                    (ty, CheckedExpressionStatus::Recovered)
                }
            };

            results.push(CheckedExpressionFactEntry::new(
                expression,
                CheckedExpressionResult::new(ty, status),
            ));
        }

        BoundWalkControl::Continue
    });

    if cancelled {
        return CheckerOutcome::Cancelled;
    }

    match outcome {
        BoundWalkOutcome::Completed => {}
        BoundWalkOutcome::MissingNode(node) => {
            panic!(
                "bound-tree traversal reached a missing node during expression checking: {node:?}"
            );
        }
        BoundWalkOutcome::Stopped => {
            panic!("expression-fact traversal stopped without cancellation");
        }
    }

    let facts =
        match CheckedExpressionFacts::try_new(unit, CheckedExpressionFactInput::new(results)) {
            Ok(facts) => facts,
            Err(error) => panic!("bound expression facts violated checker invariants: {error:?}"),
        };

    CheckerOutcome::without_diagnostics(ExpressionFactCheckResult::new(facts))
}

const fn root_node(root: BoundUnitRoot) -> AnyBoundNodeId {
    match root {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            AnyBoundNodeId::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
    }
}
