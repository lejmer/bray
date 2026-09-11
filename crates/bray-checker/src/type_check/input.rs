use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_bound_tree::{BoundExpressionId, SelectedIterationSource, SemanticSelectionEntry};
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

/// Additional typed inputs supplied by cooperating checking rules.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExpressionTypeInput {
    evidence: Arc<[ExpressionTypeEvidence]>,
    expectations: Arc<[ExpressionTypeExpectation]>,
    box_storage_policies: Arc<[(BoundExpressionId, TypeId)]>,
    iteration_sources: Arc<[SelectedIterationSource]>,
    operation_selections: Arc<[SemanticSelectionEntry]>,
    callable_result_type: Option<TypeId>,
}

impl ExpressionTypeInput {
    /// Creates an empty expression-typing input.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces explicit storage-policy types keyed by their box construction occurrences.
    pub fn with_box_storage_policies(
        mut self,
        policies: impl IntoIterator<Item = (BoundExpressionId, TypeId)>,
    ) -> Self {
        self.box_storage_policies = sorted_unique_shared_slice(policies);

        self
    }

    pub(crate) fn box_storage_policies(&self) -> &[(BoundExpressionId, TypeId)] {
        &self.box_storage_policies
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

    /// Sets the declared result type for return-expression checking.
    pub const fn with_callable_result_type(mut self, ty: TypeId) -> Self {
        self.callable_result_type = Some(ty);

        self
    }

    /// Replaces exact semantic selections established by operation resolution.
    pub fn with_operation_selections(
        mut self,
        selections: impl IntoIterator<Item = SemanticSelectionEntry>,
    ) -> Self {
        self.operation_selections = shared_slice(selections);

        self
    }

    /// Replaces exact iteration-source selections established before expression checking.
    pub fn with_iteration_sources(
        mut self,
        sources: impl IntoIterator<Item = SelectedIterationSource>,
    ) -> Self {
        self.iteration_sources = shared_slice(sources);

        self
    }

    pub(crate) fn evidence(&self) -> &[ExpressionTypeEvidence] {
        &self.evidence
    }

    pub(crate) fn expectations(&self) -> &[ExpressionTypeExpectation] {
        &self.expectations
    }

    pub(crate) fn operation_selections(&self) -> &[SemanticSelectionEntry] {
        &self.operation_selections
    }

    pub(crate) fn iteration_sources(&self) -> &[SelectedIterationSource] {
        &self.iteration_sources
    }

    pub(crate) const fn callable_result_type(&self) -> Option<TypeId> {
        self.callable_result_type
    }
}
