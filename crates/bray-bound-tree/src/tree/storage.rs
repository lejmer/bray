use std::sync::Arc;

use crate::{
    BoundBlock, BoundBlockId, BoundCallableBody, BoundCallableBodyId, BoundExpression,
    BoundExpressionId, BoundPattern, BoundPatternId, BoundUnitId, BoundUnitKey, BoundUnitView,
};

/// Immutable bound nodes for one checked semantic unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundTree {
    unit: BoundUnitId,
    expressions: Arc<[BoundExpression]>,
    patterns: Arc<[BoundPattern]>,
    blocks: Arc<[BoundBlock]>,
    callable_bodies: Arc<[BoundCallableBody]>,
}

impl BoundTree {
    pub(crate) fn new(
        unit: BoundUnitId,
        expressions: Vec<BoundExpression>,
        patterns: Vec<BoundPattern>,
        blocks: Vec<BoundBlock>,
        callable_bodies: Vec<BoundCallableBody>,
    ) -> Self {
        Self {
            unit,
            expressions: expressions.into(),
            patterns: patterns.into(),
            blocks: blocks.into(),
            callable_bodies: callable_bodies.into(),
        }
    }

    /// Returns the semantic unit owning every node in this tree.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the expression identified within this tree, when present.
    pub fn expression(&self, id: BoundExpressionId) -> Option<&BoundExpression> {
        self.entry(id.unit(), id.to_index(), &self.expressions)
    }

    /// Returns committed expressions with their exact unit-local IDs.
    pub fn expressions(
        &self,
    ) -> impl ExactSizeIterator<Item = (BoundExpressionId, &BoundExpression)> {
        self.expressions
            .iter()
            .enumerate()
            .map(|(index, expression)| {
                // BoundTreeBuilder rejects expression counts that cannot be represented by IDs.
                let slot = u32::try_from(index)
                    .unwrap_or_else(|_| panic!("bound expression slot must fit in u32"));

                (BoundExpressionId::from_slot(self.unit, slot), expression)
            })
    }

    /// Returns the pattern identified within this tree, when present.
    pub fn pattern(&self, id: BoundPatternId) -> Option<&BoundPattern> {
        self.entry(id.unit(), id.to_index(), &self.patterns)
    }

    /// Returns the block identified within this tree, when present.
    pub fn block(&self, id: BoundBlockId) -> Option<&BoundBlock> {
        self.entry(id.unit(), id.to_index(), &self.blocks)
    }

    /// Returns the callable body identified within this tree, when present.
    pub fn callable_body(&self, id: BoundCallableBodyId) -> Option<&BoundCallableBody> {
        self.entry(id.unit(), id.to_index(), &self.callable_bodies)
    }

    /// Returns a read-only view of this tree and its unit key.
    pub fn view<'tree>(&'tree self, key: &'tree BoundUnitKey) -> BoundUnitView<'tree> {
        BoundUnitView::published(self, key)
    }

    fn entry<'tree, T>(
        &self,
        unit: BoundUnitId,
        index: Option<usize>,
        entries: &'tree [T],
    ) -> Option<&'tree T> {
        if unit != self.unit {
            return None;
        }

        index.and_then(|index| entries.get(index))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::error_expression;
    use crate::{BoundTreeBuilder, BoundUnitId};

    #[test]
    fn published_trees_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::BoundTree>();
    }

    #[test]
    fn published_trees_iterate_expressions_with_their_exact_ids() {
        let mut builder = BoundTreeBuilder::new(BoundUnitId::new(4));

        let first = builder
            .push_expression(error_expression())
            .unwrap_or_else(|error| panic!("first expression must fit: {error:?}"));

        let second = builder
            .push_expression(error_expression())
            .unwrap_or_else(|error| panic!("second expression must fit: {error:?}"));

        let tree = builder.finish();
        let entries = tree.expressions().map(|(id, _)| id).collect::<Vec<_>>();

        assert_eq!(entries, [first, second]);
    }
}
