use bray_bound_tree::MatchExhaustiveness;

use crate::analysis::build::trivially_irrefutable_pattern;

pub(super) fn match_exhaustiveness(
    view: bray_bound_tree::BoundUnitView<'_>,
    expression: &bray_bound_tree::BoundMatchExpression,
) -> MatchExhaustiveness {
    let has_unguarded_catch_all = expression
        .arms()
        .iter()
        .any(|arm| arm.guard().is_none() && trivially_irrefutable_pattern(view, arm.pattern()));

    if !expression.is_recovered() && has_unguarded_catch_all {
        MatchExhaustiveness::Exhaustive
    } else {
        MatchExhaustiveness::Recovered
    }
}
