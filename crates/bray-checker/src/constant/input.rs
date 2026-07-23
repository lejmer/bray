use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpressionId, CheckedExpressionTypes, CheckedSemanticSelections};
use bray_symbols::{ConstantTermId, ConstantValueId};

use super::{ConstantCallResolver, ConstantEvaluationLimits};

/// The caller-resolved result of one constant reference dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstantReferenceResolution {
    /// The referenced constant instance evaluated successfully.
    Value(ConstantValueId),
    /// The reference remains a checked symbolic constant term.
    Term(ConstantTermId),
    /// The compilation fact graph detected a constant-definition cycle.
    Cycle,
}

/// Checked semantic inputs for one constant-expression checking or evaluation request.
pub struct ConstantEvaluationInput<'facts> {
    expression_types: &'facts CheckedExpressionTypes,
    semantic_selections: &'facts CheckedSemanticSelections,
    root: Option<BoundExpressionId>,
    references: BTreeMap<BoundExpressionId, ConstantReferenceResolution>,
    references_are_consistent: bool,
    call_resolver: Option<&'facts dyn ConstantCallResolver>,
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
            root: None,
            references: BTreeMap::new(),
            references_are_consistent: true,
            call_resolver: None,
            limits: ConstantEvaluationLimits::default(),
        }
    }

    /// Selects one expression inside the checked unit as the evaluation root.
    pub const fn with_root(mut self, root: BoundExpressionId) -> Self {
        self.root = Some(root);

        self
    }

    /// Supplies the demand-driven dependency boundary for selected constant calls.
    pub const fn with_call_resolver(mut self, resolver: &'facts dyn ConstantCallResolver) -> Self {
        self.call_resolver = Some(resolver);

        self
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

    /// Returns the complete checked operations used by evaluation.
    pub const fn semantic_selections(&self) -> &CheckedSemanticSelections {
        self.semantic_selections
    }

    pub(crate) const fn root(&self) -> Option<BoundExpressionId> {
        self.root
    }

    /// Returns the caller-resolved dependency for one constant reference occurrence.
    pub fn reference(&self, expression: BoundExpressionId) -> Option<ConstantReferenceResolution> {
        self.references.get(&expression).copied()
    }

    pub(crate) const fn call_resolver(&self) -> Option<&dyn ConstantCallResolver> {
        self.call_resolver
    }

    pub(crate) const fn references_are_consistent(&self) -> bool {
        self.references_are_consistent
    }

    /// Returns the deterministic resource limits for this request.
    pub const fn limits(&self) -> ConstantEvaluationLimits {
        self.limits
    }
}
