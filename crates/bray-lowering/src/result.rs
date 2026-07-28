use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_ir::MirUnit;

/// One semantic unit after its lowering policy has been applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoweredUnit {
    /// A runtime unit represented by validated MIR.
    Mir(Box<MirUnit>),
    /// A checked unit whose meaning is consumed entirely before runtime.
    CompileTime(CompileTimeUnit),
}

impl LoweredUnit {
    /// Returns validated MIR when this unit has runtime execution.
    pub fn mir(&self) -> Option<&MirUnit> {
        match self {
            Self::Mir(mir) => Some(mir.as_ref()),
            Self::CompileTime(_) => None,
        }
    }

    /// Returns the compile-time classification when this unit produces no MIR.
    pub const fn compile_time(&self) -> Option<&CompileTimeUnit> {
        match self {
            Self::CompileTime(unit) => Some(unit),
            Self::Mir(_) => None,
        }
    }
}

/// A checked semantic unit that requires no runtime representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileTimeUnit {
    key: BoundUnitKey,
}

impl CompileTimeUnit {
    /// Classifies a bound unit whose semantics are complete before runtime.
    pub fn try_new(key: BoundUnitKey) -> Option<Self> {
        (!requires_mir(key.kind())).then_some(Self { key })
    }

    /// Returns the exact checked semantic unit.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the unit's exact compile-time semantic category.
    pub fn kind(&self) -> BoundUnitKind {
        self.key.kind()
    }
}

pub(crate) const fn requires_mir(kind: BoundUnitKind) -> bool {
    match kind {
        BoundUnitKind::ConstantTemplate
        | BoundUnitKind::EmbeddedConstant
        | BoundUnitKind::PredicateDefinition
        | BoundUnitKind::Constraint
        | BoundUnitKind::ContractClause
        | BoundUnitKind::TargetGate => false,
        BoundUnitKind::CallableBody
        | BoundUnitKind::AnonymousCallable
        | BoundUnitKind::RuntimeDefault => true,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitKind;

    use super::requires_mir;

    #[test]
    fn compile_time_classification_covers_every_non_executable_unit_kind() {
        let compile_time = [
            BoundUnitKind::ConstantTemplate,
            BoundUnitKind::EmbeddedConstant,
            BoundUnitKind::PredicateDefinition,
            BoundUnitKind::Constraint,
            BoundUnitKind::ContractClause,
            BoundUnitKind::TargetGate,
        ];

        let executable = [
            BoundUnitKind::CallableBody,
            BoundUnitKind::AnonymousCallable,
            BoundUnitKind::RuntimeDefault,
        ];

        assert!(!compile_time.into_iter().any(requires_mir));
        assert!(executable.into_iter().all(requires_mir));
    }
}
