use bray_bound_tree::{
    BoundControlTransferKind, BoundPatternId, BoundPatternKind, BoundUnitView,
    CheckedControlTransferTarget, CheckedPatternChildProjection, CheckedPatternFacts,
    PatternProjection, PatternTest,
};
use bray_declarations::SyntaxAnchor;

use super::model::{ControlFlowGraphBuilder, LoopContext};

impl ControlFlowGraphBuilder<'_> {
    pub(super) fn checked_transfer_target(
        &self,
        kind: BoundControlTransferKind,
        target: Option<SyntaxAnchor>,
        target_loop: Option<LoopContext>,
    ) -> CheckedControlTransferTarget {
        match kind {
            BoundControlTransferKind::Return => self
                .callable
                .map(CheckedControlTransferTarget::Callable)
                .unwrap_or(CheckedControlTransferTarget::Recovered),
            BoundControlTransferKind::Break | BoundControlTransferKind::Continue => target_loop
                .map(|context| context.checked_target)
                .unwrap_or(CheckedControlTransferTarget::Recovered),
            BoundControlTransferKind::Yield => {
                if let Some(context) = target_loop
                    && target.is_some_and(|target| target == context.target)
                    && matches!(
                        context.checked_target,
                        CheckedControlTransferTarget::Iteration(_)
                    )
                {
                    return context.checked_target;
                }

                target
                    .and_then(|target| {
                        self.block_targets
                            .iter()
                            .rev()
                            .find(|context| context.target == target)
                    })
                    .map(|context| CheckedControlTransferTarget::Block(context.block))
                    .unwrap_or(CheckedControlTransferTarget::Recovered)
            }
        }
    }

    pub(super) fn record_pattern_fact(
        &mut self,
        id: BoundPatternId,
        pattern: &bray_bound_tree::BoundPattern,
    ) {
        let fact = if pattern.is_recovered() {
            Some(CheckedPatternFacts::recovered(id))
        } else {
            self.request()
                .control_fact_selections()
                .pattern(id)
                .cloned()
                .or_else(|| resolved_pattern_fact(self.view, id, pattern))
        };

        if let Some(fact) = fact {
            self.facts.push_pattern(fact);
        }
    }
}

fn resolved_pattern_fact(
    view: BoundUnitView<'_>,
    id: BoundPatternId,
    pattern: &bray_bound_tree::BoundPattern,
) -> Option<CheckedPatternFacts> {
    let irrefutable = trivially_irrefutable_pattern(view, id);

    match pattern.kind() {
        BoundPatternKind::Binding | BoundPatternKind::Discard | BoundPatternKind::Remaining
            if pattern.children().is_empty() =>
        {
            Some(CheckedPatternFacts::resolved(
                id,
                true,
                PatternTest::Always,
                [],
            ))
        }
        BoundPatternKind::NullableAbsent if pattern.children().is_empty() => Some(
            CheckedPatternFacts::resolved(id, false, PatternTest::NullableAbsent, []),
        ),
        BoundPatternKind::NullablePresent if pattern.children().len() == 1 => {
            let child = pattern.children().first().copied()?;

            Some(CheckedPatternFacts::resolved(
                id,
                false,
                PatternTest::NullablePresent,
                [CheckedPatternChildProjection::new(
                    child,
                    PatternProjection::NullableValue,
                )],
            ))
        }
        BoundPatternKind::Box if pattern.children().len() == 1 => {
            let child = pattern.children().first().copied()?;

            Some(CheckedPatternFacts::resolved(
                id,
                irrefutable,
                PatternTest::Always,
                [CheckedPatternChildProjection::new(
                    child,
                    PatternProjection::OwnedTarget,
                )],
            ))
        }
        BoundPatternKind::Grouped => Some(CheckedPatternFacts::resolved(
            id,
            irrefutable,
            PatternTest::Always,
            identity_projections(pattern.children()),
        )),
        BoundPatternKind::Tuple => Some(CheckedPatternFacts::resolved(
            id,
            irrefutable,
            PatternTest::Always,
            ordinal_projections(pattern.children(), PatternProjection::TupleElement)?,
        )),
        BoundPatternKind::Array => Some(CheckedPatternFacts::resolved(
            id,
            irrefutable,
            PatternTest::Always,
            array_projections(view, pattern.children())?,
        )),
        BoundPatternKind::Alternative => Some(CheckedPatternFacts::resolved(
            id,
            irrefutable,
            PatternTest::Always,
            identity_projections(pattern.children()),
        )),
        _ => None,
    }
}

fn identity_projections(children: &[BoundPatternId]) -> Vec<CheckedPatternChildProjection> {
    children
        .iter()
        .map(|child| CheckedPatternChildProjection::new(*child, PatternProjection::Identity))
        .collect()
}

fn ordinal_projections(
    children: &[BoundPatternId],
    projection: fn(bray_symbols::SymbolOrdinal) -> PatternProjection,
) -> Option<Vec<CheckedPatternChildProjection>> {
    children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let ordinal = u32::try_from(index).ok()?;

            Some(CheckedPatternChildProjection::new(
                *child,
                projection(bray_symbols::SymbolOrdinal::new(ordinal)),
            ))
        })
        .collect()
}

fn array_projections(
    view: BoundUnitView<'_>,
    children: &[BoundPatternId],
) -> Option<Vec<CheckedPatternChildProjection>> {
    let remainder = children.iter().position(|child| {
        view.pattern(*child)
            .is_some_and(|pattern| pattern.kind() == BoundPatternKind::Remaining)
    });

    children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let projection = match remainder {
                Some(remainder) if index == remainder => PatternProjection::Remainder,
                Some(remainder) if index > remainder => {
                    let from_end = children.len().checked_sub(index + 1)?;

                    PatternProjection::ArrayElementFromEnd(bray_symbols::SymbolOrdinal::new(
                        u32::try_from(from_end).ok()?,
                    ))
                }
                _ => PatternProjection::ArrayElement(bray_symbols::SymbolOrdinal::new(
                    u32::try_from(index).ok()?,
                )),
            };

            Some(CheckedPatternChildProjection::new(*child, projection))
        })
        .collect()
}

pub(in crate::analysis) fn trivially_irrefutable_pattern(
    view: BoundUnitView<'_>,
    id: BoundPatternId,
) -> bool {
    let Some(pattern) = view.pattern(id) else {
        return false;
    };

    if pattern.is_recovered() {
        return false;
    }

    match pattern.kind() {
        BoundPatternKind::Binding | BoundPatternKind::Discard | BoundPatternKind::Remaining => {
            pattern.children().is_empty()
        }
        BoundPatternKind::Grouped
        | BoundPatternKind::Box
        | BoundPatternKind::Product
        | BoundPatternKind::Tuple
        | BoundPatternKind::Array => pattern
            .children()
            .iter()
            .all(|child| trivially_irrefutable_pattern(view, *child)),
        BoundPatternKind::Alternative => pattern
            .children()
            .iter()
            .any(|child| trivially_irrefutable_pattern(view, *child)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundNodeOrigin, BoundPattern, BoundPatternKind, BoundPatternMode, BoundTreeBuilder,
        BoundUnitId, CheckedPatternResolution, PatternProjection,
    };
    use bray_symbols::SymbolOrdinal;

    use super::resolved_pattern_fact;
    use crate::test_support::{callable_key, error_type};

    #[test]
    fn array_remainder_projects_suffix_elements_from_the_end() {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(11));
        let first = push_pattern(&mut tree, origin, BoundPatternKind::Binding);
        let remainder = push_pattern(&mut tree, origin, BoundPatternKind::Remaining);
        let last = push_pattern(&mut tree, origin, BoundPatternKind::Binding);
        let array = match tree.push_pattern(BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Match,
            BoundPatternKind::Array,
            [first, remainder, last],
            [],
        )) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test array pattern must fit: {error:?}"),
        };
        let tree = tree.finish();
        let view = tree.view(&key);
        let Some(pattern) = view.pattern(array) else {
            panic!("test array pattern must exist");
        };
        let Some(facts) = resolved_pattern_fact(view, array, pattern) else {
            panic!("array pattern must publish resolved projections");
        };
        let CheckedPatternResolution::Resolved { projections, .. } = facts.resolution() else {
            panic!("array pattern must not recover");
        };

        assert_eq!(
            projections
                .iter()
                .map(|projection| projection.projection())
                .collect::<Vec<_>>(),
            [
                PatternProjection::ArrayElement(SymbolOrdinal::new(0)),
                PatternProjection::Remainder,
                PatternProjection::ArrayElementFromEnd(SymbolOrdinal::new(0)),
            ]
        );
    }

    fn push_pattern(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        kind: BoundPatternKind,
    ) -> bray_bound_tree::BoundPatternId {
        match tree.push_pattern(BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Match,
            kind,
            [],
            [],
        )) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test child pattern must fit: {error:?}"),
        }
    }
}
