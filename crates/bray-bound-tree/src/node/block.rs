use std::sync::Arc;

use bray_base::shared_slice;

use crate::{BoundExpressionId, BoundNodeOrigin};

/// A source-shaped block with expressions in deterministic evaluation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundBlock {
    origin: BoundNodeOrigin,
    expressions: Arc<[BoundExpressionId]>,
    is_recovered: bool,
}

impl BoundBlock {
    /// Creates a block from its checked expressions in source-semantic order.
    pub fn new(
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            expressions: shared_slice(expressions),
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin of this block.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns expressions in deterministic evaluation order.
    pub fn expressions(&self) -> &[BoundExpressionId] {
        &self.expressions
    }

    /// Returns whether recovery was required while checking this block.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}
