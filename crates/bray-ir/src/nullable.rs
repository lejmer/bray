use bray_symbols::TypeId;

use crate::MirOperand;

/// Compiler-provided nullable state query selected during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirNullableQueryKind {
    /// Test whether the nullable contains a value.
    IsPresent,
    /// Test whether the nullable is absent.
    IsAbsent,
}

/// One nullable state query with its checked input and result types.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirNullableQuery {
    kind: MirNullableQueryKind,
    operand: MirOperand,
    operand_type: TypeId,
    nullable_type: TypeId,
    result_type: TypeId,
}

impl MirNullableQuery {
    pub(crate) fn remap_local_ids(
        &mut self,
        mappings: &impl crate::unit::local_id_remap::MirLocalIdMapping,
    ) {
        self.operand.remap_local_ids(mappings);
    }

    /// Creates one checked nullable state query.
    pub const fn new(
        kind: MirNullableQueryKind,
        operand: MirOperand,
        operand_type: TypeId,
        nullable_type: TypeId,
        result_type: TypeId,
    ) -> Self {
        Self {
            kind,
            operand,
            operand_type,
            nullable_type,
            result_type,
        }
    }

    /// Returns the selected state query.
    pub const fn kind(&self) -> MirNullableQueryKind {
        self.kind
    }

    /// Returns the evaluated nullable value.
    pub const fn operand(&self) -> &MirOperand {
        &self.operand
    }

    /// Returns the exact lowered operand type, including any receiver borrow.
    pub const fn operand_type(&self) -> TypeId {
        self.operand_type
    }

    /// Returns the nullable value type whose state is queried.
    pub const fn nullable_type(&self) -> TypeId {
        self.nullable_type
    }

    /// Returns the exact checked Boolean result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }
}
