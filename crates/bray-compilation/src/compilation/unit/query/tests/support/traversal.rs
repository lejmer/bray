use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundUnit, BoundWalkControl,
    BoundWalkEvent, walk_bound_unit_view,
};

pub(in crate::compilation::unit::query::tests) fn first_pattern_reference(
    bound: &BoundUnit,
) -> Option<BoundExpressionId> {
    first_expression(bound, |expression| {
        matches!(expression, BoundExpression::PatternReference(_))
    })
}

pub(in crate::compilation::unit::query::tests) fn first_expression(
    bound: &BoundUnit,
    mut matches: impl FnMut(&BoundExpression) -> bool,
) -> Option<BoundExpressionId> {
    let mut reference = None;

    walk_bound_unit_view(bound.view(), bound.root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        if bound
            .view()
            .expression(expression)
            .is_some_and(&mut matches)
        {
            reference = Some(expression);

            return BoundWalkControl::Stop;
        }

        BoundWalkControl::Continue
    });

    reference
}
