use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::TypeId;

use crate::{
    BoundBlockId, BoundExpressionId, BoundIterationSource, BoundNodeOrigin, BoundPatternId,
    IterationSourceMode,
};

/// One exact match-arm relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundMatchArm {
    pattern: BoundPatternId,
    guard: Option<BoundExpressionId>,
    body: BoundBlockId,
}

impl BoundMatchArm {
    /// Creates one bound match arm.
    pub const fn new(
        pattern: BoundPatternId,
        guard: Option<BoundExpressionId>,
        body: BoundBlockId,
    ) -> Self {
        Self {
            pattern,
            guard,
            body,
        }
    }

    /// Returns the arm pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the optional arm guard.
    pub const fn guard(self) -> Option<BoundExpressionId> {
        self.guard
    }

    /// Returns the arm body.
    pub const fn body(self) -> BoundBlockId {
        self.body
    }
}

/// A match expression preserving exact arm associations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundMatchExpression {
    origin: BoundNodeOrigin,
    subject: BoundExpressionId,
    arms: Arc<[BoundMatchArm]>,
    operands: Arc<[BoundExpressionId]>,
    blocks: Arc<[BoundBlockId]>,
    patterns: Arc<[BoundPatternId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundMatchExpression {
    /// Creates a match expression with source-ordered arms.
    pub fn new(
        origin: BoundNodeOrigin,
        subject: BoundExpressionId,
        arms: impl IntoIterator<Item = BoundMatchArm>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        let arms = shared_slice(arms);
        let operands =
            shared_slice(std::iter::once(subject).chain(arms.iter().filter_map(|arm| arm.guard())));
        let blocks = shared_slice(arms.iter().map(|arm| arm.body()));
        let patterns = shared_slice(arms.iter().map(|arm| arm.pattern()));

        Self {
            origin,
            subject,
            arms,
            operands,
            blocks,
            patterns,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the match subject.
    pub const fn subject(&self) -> BoundExpressionId {
        self.subject
    }

    /// Returns arms in source order.
    pub fn arms(&self) -> &[BoundMatchArm] {
        &self.arms
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this match.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }

    pub(crate) fn blocks(&self) -> &[BoundBlockId] {
        &self.blocks
    }

    pub(crate) fn patterns(&self) -> &[BoundPatternId] {
        &self.patterns
    }
}

/// A for expression preserving its iteration pattern and branch roles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundForExpression {
    origin: BoundNodeOrigin,
    source: BoundExpressionId,
    source_mode: IterationSourceMode,
    pattern: BoundPatternId,
    body: BoundBlockId,
    else_body: Option<BoundBlockId>,
    blocks: Arc<[BoundBlockId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundForExpression {
    /// Creates a for expression.
    pub fn new(
        origin: BoundNodeOrigin,
        source: BoundIterationSource,
        pattern: BoundPatternId,
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            source: source.expression(),
            source_mode: source.mode(),
            pattern,
            body,
            else_body,
            blocks: shared_slice(std::iter::once(body).chain(else_body)),
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the iteration source.
    pub const fn source(&self) -> BoundExpressionId {
        self.source
    }

    /// Returns how this expression accesses its iteration source.
    pub const fn source_mode(&self) -> IterationSourceMode {
        self.source_mode
    }

    /// Returns the iteration pattern.
    pub const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the loop body.
    pub const fn body(&self) -> BoundBlockId {
        self.body
    }

    /// Returns the optional else body.
    pub const fn else_body(&self) -> Option<BoundBlockId> {
        self.else_body
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this for expression.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.source)
    }

    pub(crate) fn blocks(&self) -> &[BoundBlockId] {
        &self.blocks
    }

    pub(crate) fn patterns(&self) -> &[BoundPatternId] {
        std::slice::from_ref(&self.pattern)
    }
}
