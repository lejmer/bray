use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_ir::MirUnit;

/// One semantic unit after its lowering policy has been applied.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CompileTimeUnit {
    key: BoundUnitKey,
}

impl CompileTimeUnit {
    /// Classifies a bound unit whose semantics are complete before runtime.
    pub fn try_new(key: BoundUnitKey) -> Option<Self> {
        (!requires_mir(&key)).then_some(Self { key })
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

pub(crate) fn requires_mir(key: &BoundUnitKey) -> bool {
    requires_mir_kind(key.kind(), key.declared_owner().kind())
}

fn requires_mir_kind(kind: BoundUnitKind, owner: bray_symbols::SymbolKind) -> bool {
    match kind {
        BoundUnitKind::ConstantTemplate => owner == bray_symbols::SymbolKind::Static,
        BoundUnitKind::EmbeddedConstant
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
    use bray_symbols::SymbolKind;

    use super::requires_mir_kind;

    #[test]
    fn only_static_constant_templates_join_executable_unit_kinds() {
        assert!(requires_mir_kind(
            BoundUnitKind::ConstantTemplate,
            SymbolKind::Static
        ));

        assert!(!requires_mir_kind(
            BoundUnitKind::ConstantTemplate,
            SymbolKind::Constant
        ));

        for kind in [
            BoundUnitKind::EmbeddedConstant,
            BoundUnitKind::PredicateDefinition,
            BoundUnitKind::Constraint,
            BoundUnitKind::ContractClause,
            BoundUnitKind::TargetGate,
        ] {
            assert!(!requires_mir_kind(kind, SymbolKind::Module));
        }

        for kind in [
            BoundUnitKind::CallableBody,
            BoundUnitKind::AnonymousCallable,
            BoundUnitKind::RuntimeDefault,
        ] {
            assert!(requires_mir_kind(kind, SymbolKind::Function));
        }
    }
}
