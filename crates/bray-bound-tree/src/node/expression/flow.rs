use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;
use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundNodeOrigin};

/// The exact category of a source control transfer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundControlTransferKind {
    /// Transfer a value from a block or generator region.
    Yield,
    /// Return from the current callable.
    Return,
    /// Exit the current loop.
    Break,
    /// Continue the current loop.
    Continue,
}

/// A source control transfer with its exact semantic target.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundControlTransferExpression {
    origin: BoundNodeOrigin,
    kind: BoundControlTransferKind,
    operand: Option<BoundExpressionId>,
    operands: Arc<[BoundExpressionId]>,
    target: Option<SyntaxAnchor>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundControlTransferExpression {
    /// Creates a source control transfer.
    pub fn new(
        origin: BoundNodeOrigin,
        kind: BoundControlTransferKind,
        operand: Option<BoundExpressionId>,
        target: Option<SyntaxAnchor>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            operand,
            operands: shared_slice(operand),
            target,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact transfer category.
    pub const fn kind(&self) -> BoundControlTransferKind {
        self.kind
    }

    /// Returns the optional transferred value.
    pub const fn operand(&self) -> Option<BoundExpressionId> {
        self.operand
    }

    /// Returns the exact target when one was available.
    pub const fn target(&self) -> Option<SyntaxAnchor> {
        self.target
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this transfer.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}
