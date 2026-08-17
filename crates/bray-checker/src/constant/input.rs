use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, CheckedExpressionTypes, CheckedPatterns,
    CheckedSemanticSelections,
};
use bray_source::SourceSpan;
use bray_symbols::{AnyLocalSymbolId, ConstantTermId, ConstantValueId, TypeId};

use super::{ConstantCallResolver, ConstantEvaluationLimits, EvaluatedConstantCall};

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
    /// The referenced constant evaluated with transitive deterministic work usage.
    Evaluated(EvaluatedConstantCall),
    /// The reference remains a checked symbolic constant term.
    Term(ConstantTermId),
    /// Constant evaluation reached a cycle, retaining the referenced definition when source-owned.
    Cycle {
        /// The declaration that closed the cycle when it has a source location.
        definition: Option<SourceSpan>,
    },
    /// The reference is not permitted by this constant-evaluation context.
    Invalid,
}

/// Checked semantic inputs for one constant-expression checking or evaluation request.
pub struct ConstantEvaluationInput<'input> {
    expression_types: &'input CheckedExpressionTypes,
    semantic_selections: &'input CheckedSemanticSelections,
    patterns: Option<&'input CheckedPatterns>,
    root: Option<ConstantEvaluationRoot>,
    result_type: Option<TypeId>,
    references: BTreeMap<BoundExpressionId, ConstantReferenceResolution>,
    local_terms: BTreeMap<AnyLocalSymbolId, ConstantTermId>,
    is_consistent: bool,
    call_resolver: Option<&'input dyn ConstantCallResolver>,
    allow_static_address_borrows: bool,
    limits: ConstantEvaluationLimits,
}

impl<'input> ConstantEvaluationInput<'input> {
    /// Creates an evaluation input with the standard deterministic limits.
    pub fn new(
        expression_types: &'input CheckedExpressionTypes,
        semantic_selections: &'input CheckedSemanticSelections,
    ) -> Self {
        Self {
            expression_types,
            semantic_selections,
            patterns: None,
            root: None,
            result_type: None,
            references: BTreeMap::new(),
            local_terms: BTreeMap::new(),
            is_consistent: true,
            call_resolver: None,
            allow_static_address_borrows: false,
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
    pub const fn with_patterns(mut self, patterns: &'input CheckedPatterns) -> Self {
        self.patterns = Some(patterns);

        self
    }

    /// Supplies the demand-driven dependency boundary for selected constant calls.
    pub const fn with_call_resolver(mut self, resolver: &'input dyn ConstantCallResolver) -> Self {
        self.call_resolver = Some(resolver);

        self
    }

    /// Permits shared address formation for static storage while validating an initializer
    /// template. The resulting term is a validation recovery value and is not evaluated.
    pub const fn with_static_address_borrows(mut self) -> Self {
        self.allow_static_address_borrows = true;

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
                self.is_consistent = false;
            }
        }

        self
    }

    /// Supplies closed or symbolic values for local constants visible at the selected root.
    /// Repeated local identities must have the same term.
    pub fn with_local_terms(
        mut self,
        terms: impl IntoIterator<Item = (AnyLocalSymbolId, ConstantTermId)>,
    ) -> Self {
        for (local, term) in terms {
            if self
                .local_terms
                .insert(local, term)
                .is_some_and(|existing| existing != term)
            {
                self.is_consistent = false;
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

    pub(crate) const fn patterns(&self) -> Option<&CheckedPatterns> {
        self.patterns
    }

    /// Returns the caller-resolved dependency for one constant reference occurrence.
    pub fn reference(&self, expression: BoundExpressionId) -> Option<ConstantReferenceResolution> {
        self.references.get(&expression).copied()
    }

    pub(crate) fn local_term(&self, local: AnyLocalSymbolId) -> Option<ConstantTermId> {
        self.local_terms.get(&local).copied()
    }

    pub(crate) const fn call_resolver(&self) -> Option<&dyn ConstantCallResolver> {
        self.call_resolver
    }

    pub(crate) const fn is_consistent(&self) -> bool {
        self.is_consistent
    }

    pub(crate) const fn allows_static_address_borrows(&self) -> bool {
        self.allow_static_address_borrows
    }

    /// Returns the deterministic resource limits for this request.
    pub const fn limits(&self) -> ConstantEvaluationLimits {
        self.limits
    }
}
