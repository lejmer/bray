use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_bound_tree::BoundExpressionId;
use bray_symbols::TypeId;

/// Canonical type evidence established for one expression by an earlier focused rule.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExpressionTypeEvidence {
    expression: BoundExpressionId,
    ty: TypeId,
}

impl ExpressionTypeEvidence {
    /// Creates exact type evidence for one expression occurrence.
    pub const fn new(expression: BoundExpressionId, ty: TypeId) -> Self {
        Self { expression, ty }
    }

    /// Returns the expression constrained by this evidence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the canonical type established by the producing rule.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// An expected type applied to one expression without selecting its operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExpressionTypeExpectation {
    expression: BoundExpressionId,
    ty: TypeId,
}

impl ExpressionTypeExpectation {
    /// Creates one expected-type constraint.
    pub const fn new(expression: BoundExpressionId, ty: TypeId) -> Self {
        Self { expression, ty }
    }

    /// Returns the expression checked against the expectation.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the canonical expected type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// Additional typed inputs supplied by focused literal and selection rules.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpressionTypeInput {
    evidence: Arc<[ExpressionTypeEvidence]>,
    expectations: Arc<[ExpressionTypeExpectation]>,
}

impl ExpressionTypeInput {
    /// Creates an empty expression-typing input.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces canonical type evidence supplied by other checker services.
    pub fn with_evidence(
        mut self,
        evidence: impl IntoIterator<Item = ExpressionTypeEvidence>,
    ) -> Self {
        self.evidence = sorted_unique_shared_slice(evidence);

        self
    }

    /// Replaces expected types supplied by the binding context.
    pub fn with_expectations(
        mut self,
        expectations: impl IntoIterator<Item = ExpressionTypeExpectation>,
    ) -> Self {
        self.expectations = sorted_unique_shared_slice(expectations);

        self
    }

    pub(crate) fn evidence(&self) -> &[ExpressionTypeEvidence] {
        &self.evidence
    }

    pub(crate) fn expectations(&self) -> &[ExpressionTypeExpectation] {
        &self.expectations
    }
}
