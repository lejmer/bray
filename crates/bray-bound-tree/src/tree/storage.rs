use std::sync::Arc;

use crate::{
    BoundBlock, BoundBlockId, BoundCallableBody, BoundCallableBodyId, BoundExpression,
    BoundExpressionId, BoundPattern, BoundPatternId, BoundUnitId, BoundUnitKey, BoundUnitView,
};

/// Immutable bound nodes for one checked semantic unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundTree {
    unit: BoundUnitId,
    expressions: Arc<[BoundExpression]>,
    expression_parents: Arc<[Option<BoundExpressionId>]>,
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
        let expression_parents = expression_parents(unit, &expressions);

        Self {
            unit,
            expressions: expressions.into(),
            expression_parents: expression_parents.into(),
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
        unit_entries(self.unit, &self.expressions, BoundExpressionId::from_slot)
    }

    /// Returns the direct expression parent of one expression, when it has one.
    pub fn expression_parent(&self, id: BoundExpressionId) -> Option<BoundExpressionId> {
        self.entry(id.unit(), id.to_index(), &self.expression_parents)
            .copied()
            .flatten()
    }

    /// Returns the pattern identified within this tree, when present.
    pub fn pattern(&self, id: BoundPatternId) -> Option<&BoundPattern> {
        self.entry(id.unit(), id.to_index(), &self.patterns)
    }

    /// Returns committed patterns with their exact unit-local IDs.
    pub fn patterns(&self) -> impl ExactSizeIterator<Item = (BoundPatternId, &BoundPattern)> {
        unit_entries(self.unit, &self.patterns, BoundPatternId::from_slot)
    }

    /// Returns the block identified within this tree, when present.
    pub fn block(&self, id: BoundBlockId) -> Option<&BoundBlock> {
        self.entry(id.unit(), id.to_index(), &self.blocks)
    }

    /// Returns blocks with their unit-local identities.
    pub fn blocks(&self) -> impl ExactSizeIterator<Item = (BoundBlockId, &BoundBlock)> {
        unit_entries(self.unit, &self.blocks, BoundBlockId::from_slot)
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

fn expression_parents(
    unit: BoundUnitId,
    expressions: &[BoundExpression],
) -> Vec<Option<BoundExpressionId>> {
    let mut parents = vec![None; expressions.len()];

    for (parent, expression) in unit_entries(unit, expressions, BoundExpressionId::from_slot) {
        for child in expression.child_expressions() {
            let Some(entry) = child.to_index().and_then(|index| parents.get_mut(index)) else {
                continue;
            };

            if entry.is_none() {
                *entry = Some(parent);
            }
        }
    }

    parents
}

fn unit_entries<T, I>(
    unit: BoundUnitId,
    entries: &[T],
    make_id: impl Fn(BoundUnitId, u32) -> I,
) -> impl ExactSizeIterator<Item = (I, &T)> {
    entries.iter().enumerate().map(move |(index, entry)| {
        // BoundTreeBuilder rejects node counts that cannot be represented by IDs.
        let slot =
            u32::try_from(index).unwrap_or_else(|_| panic!("bound node slot must fit in u32"));

        (make_id(unit, slot), entry)
    })
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_expression, error_type, source_anchor};
    use crate::{
        BoundNodeOrigin, BoundPattern, BoundPatternKind, BoundPatternMode, BoundTreeBuilder,
        BoundUnitId,
    };

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

    #[test]
    fn published_trees_iterate_patterns_with_their_exact_ids() {
        let mut builder = BoundTreeBuilder::new(BoundUnitId::new(4));
        let origin = BoundNodeOrigin::source(source_anchor());

        let first = builder
            .push_pattern(error_pattern(origin))
            .unwrap_or_else(|error| panic!("first pattern must fit: {error:?}"));

        let second = builder
            .push_pattern(error_pattern(origin))
            .unwrap_or_else(|error| panic!("second pattern must fit: {error:?}"));

        let tree = builder.finish();
        let entries = tree.patterns().map(|(id, _)| id).collect::<Vec<_>>();

        assert_eq!(entries, [first, second]);
    }

    fn error_pattern(origin: BoundNodeOrigin) -> BoundPattern {
        BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Declaration,
            BoundPatternKind::Error,
            [],
            [],
        )
    }
}
