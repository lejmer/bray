use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, LocalBindingSymbolId, SymbolName, TypeId};

use crate::{BoundExpressionId, BoundNodeOrigin, BoundPatternId, BoundUnresolvedReferenceKind};

/// The exact semantic identity reached by a bound reference expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundReferenceTarget {
    /// A body-local identity.
    Local(AnyLocalSymbolId),
    /// A compilation-wide identity.
    Surface(AnySymbolId),
}

/// A value reference whose pattern-introduced local remains subject-dependent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPatternReferenceExpression {
    origin: BoundNodeOrigin,
    pattern: BoundPatternId,
    binding: LocalBindingSymbolId,
    name: SymbolName,
    unresolved_kind: BoundUnresolvedReferenceKind,
    candidates: Arc<[BoundReferenceTarget]>,
    is_recovered: bool,
}

impl BoundPatternReferenceExpression {
    /// Creates one deferred pattern-local reference.
    pub fn new(
        origin: BoundNodeOrigin,
        pattern: BoundPatternId,
        binding: LocalBindingSymbolId,
        name: SymbolName,
        unresolved_kind: BoundUnresolvedReferenceKind,
        candidates: impl IntoIterator<Item = BoundReferenceTarget>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            pattern,
            binding,
            name,
            unresolved_kind,
            candidates: shared_slice(candidates),
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the pattern whose name resolution controls this reference.
    pub const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the candidate local binding.
    pub const fn binding(&self) -> LocalBindingSymbolId {
        self.binding
    }

    /// Returns the referenced source name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the ordinary lookup failure category.
    pub const fn unresolved_kind(&self) -> BoundUnresolvedReferenceKind {
        self.unresolved_kind
    }

    /// Returns viable ordinary lookup candidates in canonical order.
    pub fn candidates(&self) -> &[BoundReferenceTarget] {
        &self.candidates
    }

    /// Returns whether source recovery contributed to this reference.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A resolved source reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundNameExpression {
    origin: BoundNodeOrigin,
    target: BoundReferenceTarget,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundNameExpression {
    /// Creates a resolved source reference.
    pub const fn new(
        origin: BoundNodeOrigin,
        target: BoundReferenceTarget,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            target,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact referenced identity.
    pub const fn target(self) -> BoundReferenceTarget {
        self.target
    }

    /// Returns the resolved type when available.
    pub const fn ty(self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this expression.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// A source member selector awaiting receiver-aware lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundMemberSelector {
    /// A named member selector.
    Name(SymbolName),
    /// A tuple element selector.
    TupleElement(u32),
}

/// A variant reference whose containing type is supplied contextually.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundLeadingDotVariantExpression {
    origin: BoundNodeOrigin,
    selector: Option<BoundMemberSelector>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundLeadingDotVariantExpression {
    /// Creates a contextually typed variant reference.
    pub const fn new(
        origin: BoundNodeOrigin,
        selector: Option<BoundMemberSelector>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            selector,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the contextually selected variant.
    pub const fn selector(&self) -> Option<&BoundMemberSelector> {
        self.selector.as_ref()
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this reference.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A receiver member selection awaiting receiver-aware checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundMemberAccessExpression {
    origin: BoundNodeOrigin,
    receiver: BoundExpressionId,
    selector: Option<BoundMemberSelector>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundMemberAccessExpression {
    /// Creates a receiver member selection.
    pub const fn new(
        origin: BoundNodeOrigin,
        receiver: BoundExpressionId,
        selector: Option<BoundMemberSelector>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            receiver,
            selector,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the selected receiver.
    pub const fn receiver(&self) -> BoundExpressionId {
        self.receiver
    }

    /// Returns the requested source member selector.
    pub const fn selector(&self) -> Option<&BoundMemberSelector> {
        self.selector.as_ref()
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this selection.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.receiver)
    }
}

/// A trait-qualified member selection preserving its trait application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundTraitQualifiedMemberExpression {
    origin: BoundNodeOrigin,
    receiver: BoundExpressionId,
    trait_syntax: SyntaxAnchor,
    selector: Option<BoundMemberSelector>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundTraitQualifiedMemberExpression {
    /// Creates a trait-qualified receiver member selection.
    pub const fn new(
        origin: BoundNodeOrigin,
        receiver: BoundExpressionId,
        trait_syntax: SyntaxAnchor,
        selector: Option<BoundMemberSelector>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            receiver,
            trait_syntax,
            selector,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the selected receiver.
    pub const fn receiver(&self) -> BoundExpressionId {
        self.receiver
    }

    /// Returns the exact trait-application syntax anchor.
    pub const fn trait_syntax(&self) -> SyntaxAnchor {
        self.trait_syntax
    }

    /// Returns the requested source member selector.
    pub const fn selector(&self) -> Option<&BoundMemberSelector> {
        self.selector.as_ref()
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this selection.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.receiver)
    }
}
