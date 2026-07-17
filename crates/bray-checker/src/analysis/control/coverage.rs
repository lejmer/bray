use bray_bound_tree::{BoundExpression, MatchExhaustiveness};

use crate::analysis::build::trivially_irrefutable_pattern;

pub(super) fn match_exhaustiveness_with_selection(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: &bray_bound_tree::BoundMatchExpression,
    selection: Option<bray_bound_tree::CheckedMatchFacts>,
) -> Option<MatchExhaustiveness> {
    if match_is_recovered(view, expression) {
        return Some(MatchExhaustiveness::Recovered);
    }

    let has_unguarded_catch_all = expression
        .arms()
        .iter()
        .any(|arm| arm.guard().is_none() && trivially_irrefutable_pattern(view, arm.pattern()));

    if has_unguarded_catch_all {
        Some(MatchExhaustiveness::Exhaustive)
    } else {
        selection.map(|fact| fact.exhaustiveness())
    }
}

fn match_is_recovered(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: &bray_bound_tree::BoundMatchExpression,
) -> bool {
    expression.is_recovered()
        || view
            .expression(expression.subject())
            .is_some_and(BoundExpression::is_recovered)
        || expression.arms().iter().any(|arm| {
            view.pattern(arm.pattern())
                .is_some_and(bray_bound_tree::BoundPattern::is_recovered)
                || arm
                    .guard()
                    .and_then(|guard| view.expression(guard))
                    .is_some_and(BoundExpression::is_recovered)
                || view
                    .block(arm.body())
                    .is_some_and(bray_bound_tree::BoundBlock::is_recovered)
        })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBlock, BoundControlTransferExpression, BoundControlTransferKind, BoundExpression,
        BoundExpressionId, BoundMatchArm, BoundMatchExpression, BoundNodeOrigin, BoundPattern,
        BoundPatternId, BoundPatternKind, BoundPatternMode, BoundTreeBuilder, BoundUnitId,
        CheckedMatchFacts, MatchExhaustiveness,
    };

    use super::match_exhaustiveness_with_selection;
    use crate::test_support::{callable_key, error_type};

    #[test]
    fn multi_arm_coverage_preserves_exhaustive_and_non_exhaustive_selections() {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(18));
        let subject = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                None,
                None,
                Some(error_type()),
                false,
            )),
        );
        let first_pattern = push_pattern(&mut tree, origin);
        let second_pattern = push_pattern(&mut tree, origin);
        let first_body = push_block(&mut tree, origin);
        let second_body = push_block(&mut tree, origin);
        let match_expression = push_expression(
            &mut tree,
            BoundExpression::Match(BoundMatchExpression::new(
                origin,
                subject,
                [
                    BoundMatchArm::new(first_pattern, None, first_body),
                    BoundMatchArm::new(second_pattern, None, second_body),
                ],
                Some(error_type()),
                false,
            )),
        );
        let tree = tree.finish();
        let Some(BoundExpression::Match(match_node)) = tree.view(&key).expression(match_expression)
        else {
            panic!("test match expression must be available");
        };

        assert_eq!(
            match_exhaustiveness_with_selection(
                tree.view(&key),
                match_node,
                Some(CheckedMatchFacts::new(
                    match_expression,
                    MatchExhaustiveness::Exhaustive,
                )),
            ),
            Some(MatchExhaustiveness::Exhaustive)
        );

        assert_eq!(
            match_exhaustiveness_with_selection(
                tree.view(&key),
                match_node,
                Some(CheckedMatchFacts::new(
                    match_expression,
                    MatchExhaustiveness::NonExhaustive,
                )),
            ),
            Some(MatchExhaustiveness::NonExhaustive)
        );
    }

    fn push_expression(
        tree: &mut BoundTreeBuilder,
        expression: BoundExpression,
    ) -> BoundExpressionId {
        match tree.push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test expression must fit: {error:?}"),
        }
    }

    fn push_pattern(tree: &mut BoundTreeBuilder, origin: BoundNodeOrigin) -> BoundPatternId {
        match tree.push_pattern(BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Match,
            BoundPatternKind::Literal,
            [],
            [],
        )) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test pattern must fit: {error:?}"),
        }
    }

    fn push_block(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> bray_bound_tree::BoundBlockId {
        match tree.push_block(BoundBlock::new(origin, [], false)) {
            Ok(block) => block,
            Err(error) => panic!("test block must fit: {error:?}"),
        }
    }
}
