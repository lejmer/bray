use crate::{AnyBoundNodeId, BoundTree};

/// Controls deterministic traversal after one bound-tree event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundWalkControl {
    /// Visit the current node's children in source-semantic order.
    Continue,
    /// Do not visit the current node's children.
    SkipChildren,
    /// Stop traversal immediately.
    Stop,
}

/// One enter or exit event in deterministic bound-tree traversal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundWalkEvent {
    /// Traversal entered a node before its children.
    Enter(AnyBoundNodeId),
    /// Traversal exited a node after its children or after explicitly skipping them.
    Exit(AnyBoundNodeId),
}

/// The reason a bound-tree traversal finished.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundWalkOutcome {
    /// The root and all selected descendants were visited.
    Completed,
    /// The visitor stopped traversal explicitly.
    Stopped,
    /// The root or a committed relationship did not resolve in this tree.
    MissingNode(AnyBoundNodeId),
}

enum PendingEvent {
    Enter(AnyBoundNodeId),
    Exit(AnyBoundNodeId),
}

/// Walks one bound subtree in deterministic source-semantic order.
pub fn walk_bound_tree(
    tree: &BoundTree,
    root: impl Into<AnyBoundNodeId>,
    mut visitor: impl FnMut(BoundWalkEvent) -> BoundWalkControl,
) -> BoundWalkOutcome {
    let mut pending = vec![PendingEvent::Enter(root.into())];

    while let Some(event) = pending.pop() {
        match event {
            PendingEvent::Enter(node) => {
                if !contains(tree, node) {
                    return BoundWalkOutcome::MissingNode(node);
                }

                match visitor(BoundWalkEvent::Enter(node)) {
                    BoundWalkControl::Continue => {
                        pending.push(PendingEvent::Exit(node));
                        push_children(tree, node, &mut pending);
                    }
                    BoundWalkControl::SkipChildren => {
                        if visitor(BoundWalkEvent::Exit(node)) == BoundWalkControl::Stop {
                            return BoundWalkOutcome::Stopped;
                        }
                    }
                    BoundWalkControl::Stop => return BoundWalkOutcome::Stopped,
                }
            }
            PendingEvent::Exit(node) => {
                if visitor(BoundWalkEvent::Exit(node)) == BoundWalkControl::Stop {
                    return BoundWalkOutcome::Stopped;
                }
            }
        }
    }

    BoundWalkOutcome::Completed
}

fn contains(tree: &BoundTree, node: AnyBoundNodeId) -> bool {
    match node {
        AnyBoundNodeId::Expression(id) => tree.expression(id).is_some(),
        AnyBoundNodeId::Pattern(id) => tree.pattern(id).is_some(),
        AnyBoundNodeId::Block(id) => tree.block(id).is_some(),
        AnyBoundNodeId::CallableBody(id) => tree.callable_body(id).is_some(),
    }
}

fn push_children(tree: &BoundTree, node: AnyBoundNodeId, pending: &mut Vec<PendingEvent>) {
    match node {
        AnyBoundNodeId::Expression(id) => {
            if let Some(block) = tree
                .expression(id)
                .and_then(|expression| expression.block())
            {
                pending.push(PendingEvent::Enter(block.into()));
            }
        }
        AnyBoundNodeId::Pattern(id) => {
            if let Some(pattern) = tree.pattern(id) {
                pending.extend(
                    pattern
                        .children()
                        .iter()
                        .rev()
                        .map(|pattern| PendingEvent::Enter((*pattern).into())),
                );
            }
        }
        AnyBoundNodeId::Block(id) => {
            if let Some(block) = tree.block(id) {
                for item in block.items().iter().rev() {
                    if let Some(expression) = item.expression() {
                        pending.push(PendingEvent::Enter(expression.into()));
                    }

                    if let Some(pattern) = item.pattern() {
                        pending.push(PendingEvent::Enter(pattern.into()));
                    }
                }
            }
        }
        AnyBoundNodeId::CallableBody(id) => {
            if let Some(block) = tree.callable_body(id).and_then(|body| body.block_id()) {
                pending.push(PendingEvent::Enter(block.into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, walk_bound_tree};
    use crate::test_support::{error_type, source_anchor};
    use crate::{
        AnyBoundNodeId, BoundBlock, BoundBlockItem, BoundCallableBody, BoundErrorExpression,
        BoundExpression, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId,
    };

    #[test]
    fn traversal_uses_deterministic_relationship_order() {
        let (tree, root, first, second, block) = tree_with_two_expressions();
        let mut events = Vec::new();

        let outcome = walk_bound_tree(&tree, root, |event| {
            events.push(event);

            BoundWalkControl::Continue
        });

        assert_eq!(outcome, BoundWalkOutcome::Completed);

        assert_eq!(
            events,
            [
                BoundWalkEvent::Enter(root.into()),
                BoundWalkEvent::Enter(block.into()),
                BoundWalkEvent::Enter(first.into()),
                BoundWalkEvent::Exit(first.into()),
                BoundWalkEvent::Enter(second.into()),
                BoundWalkEvent::Exit(second.into()),
                BoundWalkEvent::Exit(block.into()),
                BoundWalkEvent::Exit(root.into()),
            ]
        );
    }

    #[test]
    fn traversal_can_skip_or_stop_without_mutating_the_tree() {
        let (tree, root, _, _, block) = tree_with_two_expressions();
        let mut events = Vec::new();

        let outcome = walk_bound_tree(&tree, root, |event| {
            events.push(event);

            match event {
                BoundWalkEvent::Enter(node) if node == AnyBoundNodeId::from(block) => {
                    BoundWalkControl::SkipChildren
                }
                _ => BoundWalkControl::Continue,
            }
        });

        assert_eq!(outcome, BoundWalkOutcome::Completed);

        assert_eq!(
            events,
            [
                BoundWalkEvent::Enter(root.into()),
                BoundWalkEvent::Enter(block.into()),
                BoundWalkEvent::Exit(block.into()),
                BoundWalkEvent::Exit(root.into()),
            ]
        );

        let outcome = walk_bound_tree(&tree, root, |_| BoundWalkControl::Stop);

        assert_eq!(outcome, BoundWalkOutcome::Stopped);
        assert!(tree.callable_body(root).is_some());
    }

    #[test]
    fn traversal_rejects_a_root_from_another_unit() {
        let (tree, _, _, _, _) = tree_with_two_expressions();
        let foreign = crate::BoundExpressionId::from_slot(BoundUnitId::new(9), 0);

        let outcome = walk_bound_tree(&tree, foreign, |_| BoundWalkControl::Continue);

        assert_eq!(
            outcome,
            BoundWalkOutcome::MissingNode(AnyBoundNodeId::from(foreign))
        );
    }

    fn tree_with_two_expressions() -> (
        crate::BoundTree,
        crate::BoundCallableBodyId,
        crate::BoundExpressionId,
        crate::BoundExpressionId,
        crate::BoundBlockId,
    ) {
        let unit = BoundUnitId::new(8);
        let origin = BoundNodeOrigin::source(source_anchor());

        let mut builder = BoundTreeBuilder::new(unit);

        let first = push_error_expression(&mut builder, origin);
        let second = push_error_expression(&mut builder, origin);

        let Ok(block) = builder.push_block(BoundBlock::new(
            origin,
            [
                BoundBlockItem::Expression(first),
                BoundBlockItem::Expression(second),
            ],
            true,
        )) else {
            panic!("test expressions belong to the test builder");
        };

        let Ok(root) = builder.push_callable_body(BoundCallableBody::block(origin, block)) else {
            panic!("test block belongs to the test builder");
        };

        (builder.finish(), root, first, second, block)
    }

    fn push_error_expression(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> crate::BoundExpressionId {
        let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type()));

        let Ok(id) = builder.push_expression(expression) else {
            panic!("one test expression must fit in the test arena");
        };

        id
    }
}
