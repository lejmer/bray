use crate::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundPatternKind,
    BoundStructuredExpressionKind, BoundUnitId, BoundUnitView,
};

use super::{
    CheckedBlockResult, CheckedBlockResultRole, CheckedControlFlowFactsBuildError,
    CheckedControlTransfer, CheckedControlTransferTarget, CheckedForIterationFacts,
    CheckedMatchFacts, CheckedPatternFacts, CheckedPatternResolution, PatternProjection,
    PatternTest,
};

pub(super) fn validate_control_transfer(
    unit: BoundUnitId,
    view: BoundUnitView<'_>,
    fact: CheckedControlTransfer,
) -> Result<(), CheckedControlFlowFactsBuildError> {
    if !fact.is_valid_for(unit) {
        return Err(CheckedControlFlowFactsBuildError::ForeignControlTransfer(
            fact.expression(),
        ));
    }

    let Some(BoundExpression::ControlTransfer(expression)) = view.expression(fact.expression())
    else {
        return Err(CheckedControlFlowFactsBuildError::InvalidControlTransfer(
            fact.expression(),
        ));
    };

    if !target_matches_transfer(view, expression.kind(), fact.target()) {
        return Err(
            CheckedControlFlowFactsBuildError::InvalidControlTransferTarget(fact.expression()),
        );
    }

    Ok(())
}

fn target_matches_transfer(
    view: BoundUnitView<'_>,
    kind: crate::BoundControlTransferKind,
    target: CheckedControlTransferTarget,
) -> bool {
    use crate::BoundControlTransferKind::{Break, Continue, Return, Yield};

    match (kind, target) {
        (_, CheckedControlTransferTarget::Recovered) => true,
        (Return, CheckedControlTransferTarget::Callable(body)) => {
            view.callable_body(body).is_some()
        }
        (Yield, CheckedControlTransferTarget::Block(block)) => view.block(block).is_some(),
        (Yield | Break | Continue, CheckedControlTransferTarget::Iteration(expression)) => {
            is_iteration_expression(view, expression)
        }
        (Break | Continue, CheckedControlTransferTarget::Loop(expression)) => {
            is_loop_expression(view, expression)
        }
        _ => false,
    }
}

pub(super) fn validate_block_result(
    unit: BoundUnitId,
    view: BoundUnitView<'_>,
    fact: CheckedBlockResult,
) -> Result<(), CheckedControlFlowFactsBuildError> {
    if !fact.is_valid_for(unit) {
        return Err(CheckedControlFlowFactsBuildError::ForeignBlockResult(
            fact.block(),
        ));
    }

    if view.block(fact.block()).is_none()
        || !block_role_matches_block(view, fact.block(), fact.role())
    {
        return Err(CheckedControlFlowFactsBuildError::InvalidBlockResult(
            fact.block(),
        ));
    }

    Ok(())
}

fn block_role_matches_block(
    view: BoundUnitView<'_>,
    block: BoundBlockId,
    role: CheckedBlockResultRole,
) -> bool {
    match role {
        CheckedBlockResultRole::Unit | CheckedBlockResultRole::Recovered => true,
        CheckedBlockResultRole::CallableBody(body) => view
            .callable_body(body)
            .is_some_and(|body| body.block_id() == Some(block)),
        CheckedBlockResultRole::Expression(expression) => view
            .expression(expression)
            .is_some_and(|expression| expression.child_blocks().any(|child| child == block)),
        CheckedBlockResultRole::LoopBody(expression) => {
            is_loop_expression(view, expression)
                && view
                    .expression(expression)
                    .and_then(|expression| expression.child_blocks().next())
                    == Some(block)
        }
        CheckedBlockResultRole::IterationBody(expression) => matches!(
            view.expression(expression),
            Some(BoundExpression::For(iteration)) if iteration.body() == block
        ),
        CheckedBlockResultRole::GeneratorBody(expression) => matches!(
            view.expression(expression),
            Some(BoundExpression::Generator(iteration)) if iteration.body() == block
        ),
    }
}

pub(super) fn validate_match(
    unit: BoundUnitId,
    view: BoundUnitView<'_>,
    fact: CheckedMatchFacts,
) -> Result<(), CheckedControlFlowFactsBuildError> {
    if !fact.is_valid_for(unit) {
        return Err(CheckedControlFlowFactsBuildError::ForeignMatch(
            fact.expression(),
        ));
    }

    if !matches!(
        view.expression(fact.expression()),
        Some(BoundExpression::Match(_))
    ) {
        return Err(CheckedControlFlowFactsBuildError::InvalidMatch(
            fact.expression(),
        ));
    }

    Ok(())
}

pub(super) fn validate_pattern(
    unit: BoundUnitId,
    view: BoundUnitView<'_>,
    fact: &CheckedPatternFacts,
) -> Result<(), CheckedControlFlowFactsBuildError> {
    if !fact.is_valid_for(unit) {
        return Err(CheckedControlFlowFactsBuildError::ForeignPattern(
            fact.pattern(),
        ));
    }

    let Some(pattern) = view.pattern(fact.pattern()) else {
        return Err(CheckedControlFlowFactsBuildError::InvalidPattern(
            fact.pattern(),
        ));
    };

    if !pattern_resolution_matches_shape(view, pattern, fact.resolution()) {
        return Err(CheckedControlFlowFactsBuildError::InvalidPattern(
            fact.pattern(),
        ));
    }

    Ok(())
}

fn pattern_resolution_matches_shape(
    view: BoundUnitView<'_>,
    pattern: &crate::BoundPattern,
    resolution: &CheckedPatternResolution,
) -> bool {
    if pattern.is_recovered() {
        return matches!(resolution, CheckedPatternResolution::Recovered);
    }

    let CheckedPatternResolution::Resolved {
        test, projections, ..
    } = resolution
    else {
        return true;
    };

    if !pattern
        .children()
        .iter()
        .copied()
        .eq(projections.iter().map(|projection| projection.child()))
    {
        return false;
    }

    match pattern.kind() {
        BoundPatternKind::Binding | BoundPatternKind::Discard | BoundPatternKind::Remaining => {
            *test == PatternTest::Always && projections.is_empty()
        }
        BoundPatternKind::Literal => {
            matches!(test, PatternTest::Constant(_)) && projections.is_empty()
        }
        BoundPatternKind::NullableAbsent => {
            *test == PatternTest::NullableAbsent && projections.is_empty()
        }
        BoundPatternKind::NullablePresent => {
            *test == PatternTest::NullablePresent
                && projections.len() == 1
                && projections_have_kind(projections, |projection| {
                    projection == PatternProjection::NullableValue
                })
        }
        BoundPatternKind::Box => {
            *test == PatternTest::Always
                && projections.len() == 1
                && projections_have_kind(projections, |projection| {
                    projection == PatternProjection::OwnedTarget
                })
        }
        BoundPatternKind::Grouped => {
            *test == PatternTest::Always
                && projections.len() == 1
                && projections_have_kind(projections, |projection| {
                    projection == PatternProjection::Identity
                })
        }
        BoundPatternKind::Alternative => {
            *test == PatternTest::Always
                && projections.len() > 1
                && projections_have_kind(projections, |projection| {
                    projection == PatternProjection::Identity
                })
        }
        BoundPatternKind::Tuple => {
            *test == PatternTest::Always && tuple_projections_are_valid(projections)
        }
        BoundPatternKind::Array => {
            *test == PatternTest::Always && array_projections_are_valid(view, projections)
        }
        BoundPatternKind::Product => {
            *test == PatternTest::Always
                && projections.iter().all(|projection| {
                    projection_matches_remainder(view, projection, |projection| {
                        matches!(projection, PatternProjection::ProductField(_))
                    })
                })
        }
        BoundPatternKind::Path | BoundPatternKind::Variant => match test {
            PatternTest::Constant(_) => projections.is_empty(),
            PatternTest::UnionVariant(variant) => projections.iter().all(|projection| {
                projection_matches_remainder(view, projection, |projection| {
                    matches!(
                        projection,
                        PatternProjection::UnionPayloadField {
                            variant: projected,
                            ..
                        } if projected == *variant
                    )
                })
            }),
            _ => false,
        },
        BoundPatternKind::Error => false,
    }
}

fn projections_have_kind(
    projections: &[super::CheckedPatternChildProjection],
    predicate: impl Fn(PatternProjection) -> bool,
) -> bool {
    projections
        .iter()
        .all(|projection| predicate(projection.projection()))
}

fn projection_matches_remainder(
    view: BoundUnitView<'_>,
    projection: &super::CheckedPatternChildProjection,
    ordinary: impl Fn(PatternProjection) -> bool,
) -> bool {
    match view
        .pattern(projection.child())
        .map(|pattern| pattern.kind())
    {
        Some(BoundPatternKind::Remaining) => {
            projection.projection() == PatternProjection::Remainder
        }
        Some(_) => ordinary(projection.projection()),
        None => false,
    }
}

fn tuple_projections_are_valid(projections: &[super::CheckedPatternChildProjection]) -> bool {
    projections.iter().enumerate().all(|(index, projection)| {
        let Ok(index) = u32::try_from(index) else {
            return false;
        };

        projection.projection()
            == PatternProjection::TupleElement(bray_symbols::SymbolOrdinal::new(index))
    })
}

fn array_projections_are_valid(
    view: BoundUnitView<'_>,
    projections: &[super::CheckedPatternChildProjection],
) -> bool {
    let remainder = projections.iter().position(|projection| {
        view.pattern(projection.child())
            .is_some_and(|pattern| pattern.kind() == BoundPatternKind::Remaining)
    });

    projections.iter().enumerate().all(|(index, projection)| {
        let expected = match remainder {
            Some(remainder) if index == remainder => PatternProjection::Remainder,
            Some(remainder) if index > remainder => {
                let Some(from_end) = projections.len().checked_sub(index + 1) else {
                    return false;
                };
                let Ok(from_end) = u32::try_from(from_end) else {
                    return false;
                };

                PatternProjection::ArrayElementFromEnd(bray_symbols::SymbolOrdinal::new(from_end))
            }
            _ => {
                let Ok(index) = u32::try_from(index) else {
                    return false;
                };

                PatternProjection::ArrayElement(bray_symbols::SymbolOrdinal::new(index))
            }
        };

        projection.projection() == expected
    })
}

pub(super) fn validate_for_iteration(
    unit: BoundUnitId,
    view: BoundUnitView<'_>,
    fact: &CheckedForIterationFacts,
) -> Result<(), CheckedControlFlowFactsBuildError> {
    if !fact.is_valid_for(unit) {
        return Err(CheckedControlFlowFactsBuildError::ForeignForIteration(
            fact.expression(),
        ));
    }

    if !matches!(
        view.expression(fact.expression()),
        Some(BoundExpression::For(_))
    ) {
        return Err(CheckedControlFlowFactsBuildError::InvalidForIteration(
            fact.expression(),
        ));
    }

    Ok(())
}

fn is_loop_expression(view: BoundUnitView<'_>, expression: BoundExpressionId) -> bool {
    matches!(
        view.expression(expression),
        Some(BoundExpression::Structured(expression))
            if matches!(
                expression.kind(),
                BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop
            )
    )
}

fn is_iteration_expression(view: BoundUnitView<'_>, expression: BoundExpressionId) -> bool {
    matches!(
        view.expression(expression),
        Some(BoundExpression::For(_) | BoundExpression::Generator(_))
    )
}

pub(super) fn reject_duplicate_control_transfers(
    facts: &[CheckedControlTransfer],
) -> Result<(), CheckedControlFlowFactsBuildError> {
    reject_adjacent_duplicate(facts, |fact| fact.expression())
        .map_err(CheckedControlFlowFactsBuildError::DuplicateControlTransfer)
}

pub(super) fn reject_duplicate_block_results(
    facts: &[CheckedBlockResult],
) -> Result<(), CheckedControlFlowFactsBuildError> {
    reject_adjacent_duplicate(facts, |fact| fact.block())
        .map_err(CheckedControlFlowFactsBuildError::DuplicateBlockResult)
}

pub(super) fn reject_duplicate_matches(
    facts: &[CheckedMatchFacts],
) -> Result<(), CheckedControlFlowFactsBuildError> {
    reject_adjacent_duplicate(facts, |fact| fact.expression())
        .map_err(CheckedControlFlowFactsBuildError::DuplicateMatch)
}

pub(super) fn reject_duplicate_patterns(
    facts: &[CheckedPatternFacts],
) -> Result<(), CheckedControlFlowFactsBuildError> {
    reject_adjacent_duplicate(facts, CheckedPatternFacts::pattern)
        .map_err(CheckedControlFlowFactsBuildError::DuplicatePattern)
}

pub(super) fn reject_duplicate_for_iterations(
    facts: &[CheckedForIterationFacts],
) -> Result<(), CheckedControlFlowFactsBuildError> {
    reject_adjacent_duplicate(facts, CheckedForIterationFacts::expression)
        .map_err(CheckedControlFlowFactsBuildError::DuplicateForIteration)
}

fn reject_adjacent_duplicate<T, K: Copy + Eq>(items: &[T], key: impl Fn(&T) -> K) -> Result<(), K> {
    for pair in items.windows(2) {
        let [first, second] = pair else {
            continue;
        };

        if key(first) == key(second) {
            return Err(key(first));
        }
    }

    Ok(())
}
