use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpressionId, CheckedExpressionTypes};
use bray_symbols::ConstantValueId;

use super::ConstantEvaluationLimits;

/// The caller-resolved result of one constant reference dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstantReferenceResolution {
    /// The referenced constant instance evaluated successfully.
    Value(ConstantValueId),
    /// The compilation fact graph detected a constant-definition cycle.
    Cycle,
}

/// Checked semantic inputs for one closed constant-expression evaluation.
pub struct ConstantEvaluationInput<'types> {
    expression_types: &'types CheckedExpressionTypes,
    references: BTreeMap<BoundExpressionId, ConstantReferenceResolution>,
    references_are_consistent: bool,
    limits: ConstantEvaluationLimits,
}

impl<'types> ConstantEvaluationInput<'types> {
    /// Creates an evaluation input with the standard deterministic limits.
    pub fn new(expression_types: &'types CheckedExpressionTypes) -> Self {
        Self {
            expression_types,
            references: BTreeMap::new(),
            references_are_consistent: true,
            limits: ConstantEvaluationLimits::default(),
        }
    }

    /// Supplies resolved constant dependencies for reference occurrences in this expression unit.
    /// Repeated occurrences must have the same resolution.
    pub fn with_references(
        mut self,
        references: impl IntoIterator<Item = (BoundExpressionId, ConstantReferenceResolution)>,
    ) -> Self {
        for (expression, resolution) in references {
            if self
                .references
                .insert(expression, resolution)
                .is_some_and(|existing| existing != resolution)
            {
                self.references_are_consistent = false;
            }
        }

        self
    }

    /// Uses explicit deterministic resource limits for this evaluation.
    pub const fn with_limits(mut self, limits: ConstantEvaluationLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Returns the complete checked expression types used by evaluation.
    pub const fn expression_types(&self) -> &CheckedExpressionTypes {
        self.expression_types
    }

    /// Returns the caller-resolved dependency for one constant reference occurrence.
    pub fn reference(&self, expression: BoundExpressionId) -> Option<ConstantReferenceResolution> {
        self.references.get(&expression).copied()
    }

    pub(crate) const fn references_are_consistent(&self) -> bool {
        self.references_are_consistent
    }

    /// Returns the deterministic resource limits for this request.
    pub const fn limits(&self) -> ConstantEvaluationLimits {
        self.limits
    }
}
