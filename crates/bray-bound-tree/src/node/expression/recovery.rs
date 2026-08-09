use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundNodeOrigin};

use super::{BoundArgument, BoundGenericArgument, BoundReferenceTarget};

/// The exact lookup failure retained by an unresolved reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundUnresolvedReferenceKind {
    /// No declaration matched the requested name.
    NotFound,
    /// Matching declarations exist but none belong to the requested lookup namespace.
    WrongKind,
    /// More than one declaration remains viable after lookup.
    Ambiguous,
    /// Matching declarations exist but are not accessible from the binding context.
    Inaccessible,
    /// Lookup could not interpret the malformed source reference.
    Malformed,
}

/// An unresolved source reference retaining every viable semantic candidate.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundUnresolvedReferenceExpression {
    origin: BoundNodeOrigin,
    kind: BoundUnresolvedReferenceKind,
    candidates: Arc<[BoundReferenceTarget]>,
    ty: TypeId,
}

impl BoundUnresolvedReferenceExpression {
    /// Creates an unresolved reference with candidates in canonical lookup order.
    pub fn new(
        origin: BoundNodeOrigin,
        kind: BoundUnresolvedReferenceKind,
        candidates: impl IntoIterator<Item = BoundReferenceTarget>,
        ty: TypeId,
    ) -> Self {
        Self {
            origin,
            kind,
            candidates: shared_slice(candidates),
            ty,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact lookup failure category.
    pub const fn kind(&self) -> BoundUnresolvedReferenceKind {
        self.kind
    }

    /// Returns retained semantic candidates in canonical lookup order.
    pub fn candidates(&self) -> &[BoundReferenceTarget] {
        &self.candidates
    }

    /// Returns the useful recovery type or canonical error type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

/// A call whose source shape survived semantic recovery.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundErrorCallExpression {
    origin: BoundNodeOrigin,
    callee: BoundExpressionId,
    generic_arguments: Arc<[BoundGenericArgument]>,
    arguments: Arc<[BoundArgument]>,
    operands: Arc<[BoundExpressionId]>,
    ty: TypeId,
}

impl BoundErrorCallExpression {
    /// Creates a recovered call retaining its callee and arguments.
    pub fn new(
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        generic_arguments: impl IntoIterator<Item = BoundGenericArgument>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        ty: TypeId,
    ) -> Self {
        let generic_arguments = shared_slice(generic_arguments);
        let arguments = shared_slice(arguments);

        let operands = shared_slice(
            std::iter::once(callee).chain(arguments.iter().map(BoundArgument::expression)),
        );

        Self {
            origin,
            callee,
            generic_arguments,
            arguments,
            operands,
            ty,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the recovered callee expression.
    pub const fn callee(&self) -> BoundExpressionId {
        self.callee
    }

    /// Returns source-ordered generic arguments retained through recovery.
    pub fn generic_arguments(&self) -> &[BoundGenericArgument] {
        &self.generic_arguments
    }

    /// Returns source-ordered recovered arguments.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }

    /// Returns the useful recovery type or canonical error type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}

/// A conversion whose operand and target survived semantic recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundErrorConversionExpression {
    origin: BoundNodeOrigin,
    operand: BoundExpressionId,
    target_syntax: SyntaxAnchor,
    target_type: Option<TypeId>,
    ty: TypeId,
}

impl BoundErrorConversionExpression {
    /// Creates a recovered conversion.
    pub const fn new(
        origin: BoundNodeOrigin,
        operand: BoundExpressionId,
        target_syntax: SyntaxAnchor,
        target_type: Option<TypeId>,
        ty: TypeId,
    ) -> Self {
        Self {
            origin,
            operand,
            target_syntax,
            target_type,
            ty,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the recovered operand.
    pub const fn operand(self) -> BoundExpressionId {
        self.operand
    }

    /// Returns the exact target type-expression syntax anchor.
    pub const fn target_syntax(self) -> SyntaxAnchor {
        self.target_syntax
    }

    /// Returns the resolved target type when recovery could determine it.
    pub const fn target_type(self) -> Option<TypeId> {
        self.target_type
    }

    /// Returns the useful recovery type or canonical error type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.operand)
    }
}
