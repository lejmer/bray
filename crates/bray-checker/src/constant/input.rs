use std::collections::BTreeMap;
use std::num::NonZeroU16;

use bray_bound_tree::{BoundExpressionId, CheckedExpressionTypes, CheckedSemanticSelections};
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
pub struct ConstantEvaluationInput<'facts> {
    expression_types: &'facts CheckedExpressionTypes,
    semantic_selections: &'facts CheckedSemanticSelections,
    references: BTreeMap<BoundExpressionId, ConstantReferenceResolution>,
    references_are_consistent: bool,
    target_integer_width_bits: Option<NonZeroU16>,
    limits: ConstantEvaluationLimits,
}

impl<'facts> ConstantEvaluationInput<'facts> {
    /// Creates an evaluation input with the standard deterministic limits.
    pub fn new(
        expression_types: &'facts CheckedExpressionTypes,
        semantic_selections: &'facts CheckedSemanticSelections,
    ) -> Self {
        Self {
            expression_types,
            semantic_selections,
            references: BTreeMap::new(),
            references_are_consistent: true,
            target_integer_width_bits: None,
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

    /// Supplies the selected target width used by `isize` and `usize`.
    pub const fn with_target_integer_width_bits(mut self, width: NonZeroU16) -> Self {
        self.target_integer_width_bits = Some(width);

        self
    }

    /// Returns the complete checked expression types used by evaluation.
    pub const fn expression_types(&self) -> &CheckedExpressionTypes {
        self.expression_types
    }

    /// Returns the complete checked operations used by evaluation.
    pub const fn semantic_selections(&self) -> &CheckedSemanticSelections {
        self.semantic_selections
    }

    /// Returns the caller-resolved dependency for one constant reference occurrence.
    pub fn reference(&self, expression: BoundExpressionId) -> Option<ConstantReferenceResolution> {
        self.references.get(&expression).copied()
    }

    pub(crate) const fn references_are_consistent(&self) -> bool {
        self.references_are_consistent
    }

    pub(crate) const fn target_integer_width_bits(&self) -> Option<NonZeroU16> {
        self.target_integer_width_bits
    }

    /// Returns the deterministic resource limits for this request.
    pub const fn limits(&self) -> ConstantEvaluationLimits {
        self.limits
    }
}
