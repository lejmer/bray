use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, CheckedExpressionTypes, CheckedPatternFacts,
    CheckedSemanticSelections,
};
use bray_symbols::{ConstantTermId, ConstantValueId, TypeId};

use super::{ConstantCallResolver, ConstantEvaluationLimits};

/// The exact root evaluated by one constant request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ConstantEvaluationRoot {
    /// One expression and all semantic children it demands.
    Expression(BoundExpressionId),
    /// One callable or control-flow block.
    Block(BoundBlockId),
}

/// The caller-resolved result of one constant reference dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConstantReferenceResolution {
    /// The referenced constant instance evaluated successfully.
    Value(ConstantValueId),
    /// The reference remains a checked symbolic constant term.
    Term(ConstantTermId),
    /// The compilation fact graph detected a constant-definition cycle.
    Cycle,
    /// The reference is not permitted by this constant-evaluation context.
    Invalid,
}

/// Checked semantic inputs for one constant-expression checking or evaluation request.
pub struct ConstantEvaluationInput<'facts> {
    expression_types: &'facts CheckedExpressionTypes,
    semantic_selections: &'facts CheckedSemanticSelections,
    pattern_facts: Option<&'facts CheckedPatternFacts>,
    root: Option<ConstantEvaluationRoot>,
    result_type: Option<TypeId>,
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
            pattern_facts: None,
            root: None,
            result_type: None,
            references: BTreeMap::new(),
            references_are_consistent: true,
            call_resolver: None,
            limits: ConstantEvaluationLimits::default(),
        }
    }

    /// Selects one expression inside the checked unit as the evaluation root.
    pub const fn with_root(mut self, root: BoundExpressionId) -> Self {
        self.root = Some(ConstantEvaluationRoot::Expression(root));

        self
    }

    /// Selects one block inside the checked unit as the evaluation root.
    pub const fn with_block_root(mut self, root: BoundBlockId, result_type: TypeId) -> Self {
        self.root = Some(ConstantEvaluationRoot::Block(root));
        self.result_type = Some(result_type);

        self
    }

    /// Supplies checked pattern structure for constant match evaluation.
    pub const fn with_pattern_facts(mut self, facts: &'facts CheckedPatternFacts) -> Self {
        self.pattern_facts = Some(facts);

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

    pub(crate) const fn root(&self) -> Option<ConstantEvaluationRoot> {
        self.root
    }

    pub(crate) const fn result_type(&self) -> Option<TypeId> {
        self.result_type
    }

    pub(crate) const fn pattern_facts(&self) -> Option<&CheckedPatternFacts> {
        self.pattern_facts
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
