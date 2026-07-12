use crate::{
    AnyBoundNodeId, BoundBlock, BoundBlockId, BoundCallableBody, BoundCallableBodyId,
    BoundExpression, BoundExpressionId, BoundNodeKind, BoundPattern, BoundPatternId, BoundTree,
    BoundUnitId, BoundUnitKey, BoundUnitView,
};

/// A typed failure while committing a node to a per-unit bound-tree builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundTreeBuildError {
    /// The category-specific arena cannot represent another slot.
    ArenaCapacityExceeded(BoundNodeKind),
    /// A node relationship refers to a different bound unit.
    ForeignNode {
        /// The unit being constructed.
        expected: BoundUnitId,
        /// The unit owning the referenced node.
        actual: BoundUnitId,
        /// The category of the referenced node.
        kind: BoundNodeKind,
    },
    /// A node relationship refers to a slot that has not been committed.
    MissingNode {
        /// The category of the missing node.
        kind: BoundNodeKind,
        /// The category-specific slot that was requested.
        slot: u32,
    },
}

/// An opaque position in one task-local bound-tree builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundTreeCheckpoint {
    unit: BoundUnitId,
    expressions: usize,
    patterns: usize,
    blocks: usize,
    callable_bodies: usize,
}

/// Task-local mutable construction storage that freezes into one immutable [`BoundTree`].
#[derive(Debug)]
pub struct BoundTreeBuilder {
    unit: BoundUnitId,
    expressions: Vec<BoundExpression>,
    patterns: Vec<BoundPattern>,
    blocks: Vec<BoundBlock>,
    callable_bodies: Vec<BoundCallableBody>,
}

impl BoundTreeBuilder {
    /// Creates empty bound storage for one semantic unit.
    pub const fn new(unit: BoundUnitId) -> Self {
        Self {
            unit,
            expressions: Vec::new(),
            patterns: Vec::new(),
            blocks: Vec::new(),
            callable_bodies: Vec::new(),
        }
    }

    /// Returns the semantic unit that will own every committed node.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Commits an expression and returns its deterministic category-specific ID.
    pub fn push_expression(
        &mut self,
        expression: BoundExpression,
    ) -> Result<BoundExpressionId, BoundTreeBuildError> {
        for child in expression.child_expressions() {
            self.validate_expression_id(child)?;
        }

        for block in expression.child_blocks() {
            self.validate_block_id(block)?;
        }

        for pattern in expression.child_patterns() {
            self.validate_pattern_id(pattern)?;
        }

        let slot = next_slot(self.expressions.len(), BoundNodeKind::Expression)?;

        self.expressions.push(expression);

        Ok(BoundExpressionId::from_slot(self.unit, slot))
    }

    /// Commits a pattern and returns its deterministic category-specific ID.
    pub fn push_pattern(
        &mut self,
        pattern: BoundPattern,
    ) -> Result<BoundPatternId, BoundTreeBuildError> {
        for child in pattern.children() {
            self.validate_pattern_id(*child)?;
        }

        let slot = next_slot(self.patterns.len(), BoundNodeKind::Pattern)?;

        self.patterns.push(pattern);

        Ok(BoundPatternId::from_slot(self.unit, slot))
    }

    /// Commits a block after validating every item relationship.
    pub fn push_block(&mut self, block: BoundBlock) -> Result<BoundBlockId, BoundTreeBuildError> {
        for item in block.items() {
            if let Some(pattern) = item.pattern() {
                self.validate_pattern_id(pattern)?;
            }

            if let Some(expression) = item.expression() {
                self.validate_expression_id(expression)?;
            }
        }

        let slot = next_slot(self.blocks.len(), BoundNodeKind::Block)?;

        self.blocks.push(block);

        Ok(BoundBlockId::from_slot(self.unit, slot))
    }

    /// Commits a callable body after validating its block relationship.
    pub fn push_callable_body(
        &mut self,
        body: BoundCallableBody,
    ) -> Result<BoundCallableBodyId, BoundTreeBuildError> {
        if let Some(block) = body.block_id() {
            self.validate_block_id(block)?;
        }

        let slot = next_slot(self.callable_bodies.len(), BoundNodeKind::CallableBody)?;

        self.callable_bodies.push(body);

        Ok(BoundCallableBodyId::from_slot(self.unit, slot))
    }

    /// Returns a read-only view over nodes committed so far.
    pub fn view<'builder>(&'builder self, key: &'builder BoundUnitKey) -> BoundUnitView<'builder> {
        BoundUnitView::building(self, key)
    }

    /// Captures the current arena lengths for later transactional rollback.
    pub const fn checkpoint(&self) -> BoundTreeCheckpoint {
        BoundTreeCheckpoint {
            unit: self.unit,
            expressions: self.expressions.len(),
            patterns: self.patterns.len(),
            blocks: self.blocks.len(),
            callable_bodies: self.callable_bodies.len(),
        }
    }

    /// Returns whether this builder can restore the supplied checkpoint without mutation.
    pub fn can_rollback_to(&self, checkpoint: BoundTreeCheckpoint) -> bool {
        checkpoint.unit == self.unit
            && checkpoint.expressions <= self.expressions.len()
            && checkpoint.patterns <= self.patterns.len()
            && checkpoint.blocks <= self.blocks.len()
            && checkpoint.callable_bodies <= self.callable_bodies.len()
    }

    /// Restores every category arena to a checkpoint from this unit.
    pub fn rollback(&mut self, checkpoint: BoundTreeCheckpoint) -> bool {
        if !self.can_rollback_to(checkpoint) {
            return false;
        }

        self.expressions.truncate(checkpoint.expressions);
        self.patterns.truncate(checkpoint.patterns);
        self.blocks.truncate(checkpoint.blocks);
        self.callable_bodies.truncate(checkpoint.callable_bodies);

        true
    }

    /// Freezes all committed nodes into immutable dense storage.
    pub fn finish(self) -> BoundTree {
        BoundTree::new(
            self.unit,
            self.expressions,
            self.patterns,
            self.blocks,
            self.callable_bodies,
        )
    }

    pub(crate) fn expression(&self, id: BoundExpressionId) -> Option<&BoundExpression> {
        entry(self.unit, id.unit(), id.to_index(), &self.expressions)
    }

    pub(crate) fn pattern(&self, id: BoundPatternId) -> Option<&BoundPattern> {
        entry(self.unit, id.unit(), id.to_index(), &self.patterns)
    }

    pub(crate) fn block(&self, id: BoundBlockId) -> Option<&BoundBlock> {
        entry(self.unit, id.unit(), id.to_index(), &self.blocks)
    }

    pub(crate) fn callable_body(&self, id: BoundCallableBodyId) -> Option<&BoundCallableBody> {
        entry(self.unit, id.unit(), id.to_index(), &self.callable_bodies)
    }

    fn validate_expression_id(&self, id: BoundExpressionId) -> Result<(), BoundTreeBuildError> {
        validate_id(
            self.unit,
            id.unit(),
            id.to_index(),
            AnyBoundNodeId::from(id).slot(),
            self.expressions.len(),
            BoundNodeKind::Expression,
        )
    }

    fn validate_pattern_id(&self, id: BoundPatternId) -> Result<(), BoundTreeBuildError> {
        validate_id(
            self.unit,
            id.unit(),
            id.to_index(),
            AnyBoundNodeId::from(id).slot(),
            self.patterns.len(),
            BoundNodeKind::Pattern,
        )
    }

    fn validate_block_id(&self, id: BoundBlockId) -> Result<(), BoundTreeBuildError> {
        validate_id(
            self.unit,
            id.unit(),
            id.to_index(),
            AnyBoundNodeId::from(id).slot(),
            self.blocks.len(),
            BoundNodeKind::Block,
        )
    }
}

fn next_slot(length: usize, kind: BoundNodeKind) -> Result<u32, BoundTreeBuildError> {
    u32::try_from(length).map_err(|_| BoundTreeBuildError::ArenaCapacityExceeded(kind))
}

fn validate_id(
    expected_unit: BoundUnitId,
    actual_unit: BoundUnitId,
    index: Option<usize>,
    slot: u32,
    length: usize,
    kind: BoundNodeKind,
) -> Result<(), BoundTreeBuildError> {
    if actual_unit != expected_unit {
        return Err(BoundTreeBuildError::ForeignNode {
            expected: expected_unit,
            actual: actual_unit,
            kind,
        });
    }

    let Some(index) = index else {
        return Err(BoundTreeBuildError::MissingNode { kind, slot });
    };

    if index >= length {
        return Err(BoundTreeBuildError::MissingNode { kind, slot });
    }

    Ok(())
}

fn entry<T>(
    expected_unit: BoundUnitId,
    actual_unit: BoundUnitId,
    index: Option<usize>,
    entries: &[T],
) -> Option<&T> {
    if actual_unit != expected_unit {
        return None;
    }

    index.and_then(|index| entries.get(index))
}

#[cfg(test)]
mod tests {
    use super::{BoundTreeBuildError, BoundTreeBuilder};
    use crate::test_support::{error_expression, source_anchor};
    use crate::{
        BoundBlock, BoundBlockItem, BoundExpressionId, BoundNodeKind, BoundNodeOrigin, BoundUnitId,
    };

    #[test]
    fn category_arenas_assign_dense_deterministic_ids() {
        let unit = BoundUnitId::new(3);
        let mut builder = BoundTreeBuilder::new(unit);

        let first = push_error_expression(&mut builder);
        let second = push_error_expression(&mut builder);

        assert_eq!(first, BoundExpressionId::from_slot(unit, 0));
        assert_eq!(second, BoundExpressionId::from_slot(unit, 1));
    }

    #[test]
    fn relationships_reject_foreign_and_uncommitted_nodes() {
        let unit = BoundUnitId::new(1);
        let mut builder = BoundTreeBuilder::new(unit);
        let origin = BoundNodeOrigin::source(source_anchor());

        let foreign = BoundBlock::new(
            origin,
            [BoundBlockItem::Expression(BoundExpressionId::from_slot(
                BoundUnitId::new(2),
                0,
            ))],
            true,
        );

        assert_eq!(
            builder.push_block(foreign),
            Err(BoundTreeBuildError::ForeignNode {
                expected: unit,
                actual: BoundUnitId::new(2),
                kind: BoundNodeKind::Expression,
            })
        );

        let missing = BoundBlock::new(
            origin,
            [BoundBlockItem::Expression(BoundExpressionId::from_slot(
                unit, 0,
            ))],
            true,
        );

        assert_eq!(
            builder.push_block(missing),
            Err(BoundTreeBuildError::MissingNode {
                kind: BoundNodeKind::Expression,
                slot: 0,
            })
        );
    }

    #[test]
    fn finished_storage_is_immutable_and_typed() {
        let unit = BoundUnitId::new(5);
        let mut builder = BoundTreeBuilder::new(unit);
        let expression = push_error_expression(&mut builder);

        let tree = builder.finish();

        assert!(tree.expression(expression).is_some());
        assert_eq!(
            tree.expression(BoundExpressionId::from_slot(BoundUnitId::new(6), 0)),
            None
        );
        assert_eq!(tree.expression(BoundExpressionId::from_slot(unit, 1)), None);
    }

    #[test]
    fn checkpoints_restore_dense_category_slots_without_cloning_arenas() {
        let unit = BoundUnitId::new(7);
        let mut builder = BoundTreeBuilder::new(unit);

        let retained = push_error_expression(&mut builder);
        let checkpoint = builder.checkpoint();
        let abandoned = push_error_expression(&mut builder);

        assert!(builder.rollback(checkpoint));
        assert!(builder.expression(retained).is_some());
        assert!(builder.expression(abandoned).is_none());

        let reused = push_error_expression(&mut builder);

        assert_eq!(reused, abandoned);
    }

    #[test]
    fn checkpoints_from_another_unit_are_rejected_without_mutation() {
        let mut first = BoundTreeBuilder::new(BoundUnitId::new(8));
        let second = BoundTreeBuilder::new(BoundUnitId::new(9));
        let expression = push_error_expression(&mut first);

        assert!(!first.rollback(second.checkpoint()));
        assert!(first.finish().expression(expression).is_some());
    }

    fn push_error_expression(builder: &mut BoundTreeBuilder) -> BoundExpressionId {
        let Ok(id) = builder.push_expression(error_expression()) else {
            panic!("one test expression must fit in an empty arena");
        };

        id
    }
}
