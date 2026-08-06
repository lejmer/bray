use bray_symbols::{ConstantValueId, TypeId};

use crate::{MirBlockId, MirOperationId, MirPlace, MirSourceAnchor};

/// A scalar value represented directly in MIR without semantic interning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirImmediateValue {
    /// A scalar boolean value.
    Boolean(bool),
    /// The single value of the unit type.
    Unit,
    /// The absent value of a nullable type.
    NullableAbsent,
}

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
    /// A deterministic compiler-defined immediate value.
    Immediate {
        /// The immediate value category.
        value: MirImmediateValue,
        /// The value's checked type.
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
            Self::Constant { ty, .. } | Self::Immediate { ty, .. } => Some(*ty),
            Self::Copy(place) | Self::Move(place) => Some(place.ty()),
            Self::Value(_) => None,
        }
    }

    /// Returns whether this operand reads from the exact storage place.
    pub fn reads_from(&self, place: &MirPlace) -> bool {
        matches!(self, Self::Copy(source) | Self::Move(source) if source == place)
    }
}

#[cfg(test)]
mod tests {
    use super::{MirImmediateValue, MirOperand, MirPlace};
    use crate::test_support::test_type;
    use crate::{MirStorageId, MirUnitId};

    #[test]
    fn immediate_values_carry_types_without_semantic_value_ids() {
        let ty = test_type();

        let operand = MirOperand::Immediate {
            value: MirImmediateValue::Unit,
            ty,
        };

        assert_eq!(operand.explicit_type(), Some(ty));
    }

    #[test]
    fn storage_operands_report_exact_place_reads() {
        let ty = test_type();
        let unit = MirUnitId::new(0);
        let place = MirPlace::new(MirStorageId::from_slot(unit, 0), [], ty);
        let other = MirPlace::new(MirStorageId::from_slot(unit, 1), [], ty);

        assert!(MirOperand::Copy(place.clone()).reads_from(&place));
        assert!(MirOperand::Move(place.clone()).reads_from(&place));
        assert!(!MirOperand::Copy(other).reads_from(&place));
    }
}
