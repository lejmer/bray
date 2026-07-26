use std::sync::Arc;

use bray_bound_tree::BoundExpressionId;
use bray_symbols::ConstantValueId;

use crate::constant::ConstantEvaluationUsage;

/// A closed constant value and the reference occurrences consumed to produce it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluatedConstant {
    value: ConstantValueId,
    evaluated_references: Arc<[BoundExpressionId]>,
    usage: ConstantEvaluationUsage,
}

impl EvaluatedConstant {
    pub(super) fn new(
        value: ConstantValueId,
        evaluated_references: impl IntoIterator<Item = BoundExpressionId>,
        usage: ConstantEvaluationUsage,
    ) -> Self {
        Self {
            value,
            evaluated_references: evaluated_references.into_iter().collect(),
            usage,
        }
    }

    /// Returns the closed constant value.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }

    /// Returns reference occurrences reached during evaluation in expression ID order.
    pub fn evaluated_references(&self) -> &[BoundExpressionId] {
        &self.evaluated_references
    }

    /// Returns cumulative work consumed by this evaluation and its nested calls.
    pub const fn usage(&self) -> ConstantEvaluationUsage {
        self.usage
    }
}
