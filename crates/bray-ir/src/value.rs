use bray_symbols::{ConstantValueId, TypeId};

use crate::{MirBlockId, MirOperationId, MirPlace, MirSourceAnchor};

/// How one MIR value is defined.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirValueOrigin {
    /// An incoming value accepted by a control-flow block.
    BlockParameter(MirBlockId),
    /// The result produced by one MIR operation.
    Operation(MirOperationId),
}

/// One typed value owned by a MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirValue {
    source: MirSourceAnchor,
    ty: TypeId,
    origin: MirValueOrigin,
}

impl MirValue {
    pub(crate) const fn new(source: MirSourceAnchor, ty: TypeId, origin: MirValueOrigin) -> Self {
        Self { source, ty, origin }
    }

    /// Returns the source construct associated with this value.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the value's checked type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the definition that owns this value.
    pub const fn origin(&self) -> MirValueOrigin {
        self.origin
    }
}

/// A typed input consumed by a MIR operation or control-flow edge.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirOperand {
    /// A previously computed MIR value.
    Value(crate::MirValueId),
    /// A canonical compile-time constant.
    Constant {
        /// The concrete constant value.
        value: ConstantValueId,
        /// The constant's checked type.
        ty: TypeId,
    },
    /// A non-consuming read from storage.
    Copy(MirPlace),
    /// A consuming read from storage.
    Move(MirPlace),
}

impl MirOperand {
    /// Returns the operand's known type when it is carried directly by the operand.
    pub const fn explicit_type(&self) -> Option<TypeId> {
        match self {
            Self::Constant { ty, .. } => Some(*ty),
            Self::Copy(place) | Self::Move(place) => Some(place.ty()),
            Self::Value(_) => None,
        }
    }
}
