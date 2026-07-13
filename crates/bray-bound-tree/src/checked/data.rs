use std::sync::Arc;

use bray_symbols::{
    LocalSymbolRegionKey, LocalSymbolRegionRole, LocalSymbolSnapshot, SymbolFactKind, SymbolKind,
};

use super::CheckedUnitBuildError;
use crate::{
    BoundTree, BoundUnitId, BoundUnitKey, BoundUnitKeyData, BoundUnitKind, DeclaredBoundUnitKey,
};

macro_rules! unit_data_accessors {
    () => {
        /// Returns the compilation-local identity of this bound unit.
        pub const fn unit(&self) -> BoundUnitId {
            self.data.unit()
        }

        /// Returns the immutable source-shaped bound tree owned by this unit.
        pub const fn tree(&self) -> &BoundTree {
            self.data.tree()
        }

        /// Returns the immutable local-symbol and lexical-scope snapshot owned by this unit.
        pub const fn local_symbols(&self) -> &LocalSymbolSnapshot {
            self.data.local_symbols()
        }

        /// Returns directly nested anonymous callable units in canonical source order.
        pub fn nested_units(&self) -> &[BoundUnitKey] {
            self.data.nested_units()
        }
    };
}

pub(super) use unit_data_accessors;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CheckedUnitData {
    tree: BoundTree,
    local_symbols: LocalSymbolSnapshot,
    nested_units: Arc<[BoundUnitKey]>,
}

impl CheckedUnitData {
    pub(super) fn try_new(
        expected_kind: BoundUnitKind,
        key: &BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
    ) -> Result<Self, CheckedUnitBuildError> {
        if key.kind() != expected_kind {
            return Err(CheckedUnitBuildError::UnitKindMismatch {
                expected: expected_kind,
                actual: key.kind(),
            });
        }

        if !local_region_matches(key, local_symbols.key()) {
            return Err(CheckedUnitBuildError::LocalSymbolRegionMismatch);
        }

        let nested_units = nested_units.into_iter().collect::<Arc<[_]>>();

        validate_nested_units(key, &nested_units)?;

        Ok(Self {
            tree,
            local_symbols,
            nested_units,
        })
    }

    pub(super) const fn unit(&self) -> BoundUnitId {
        self.tree.unit()
    }

    pub(super) const fn tree(&self) -> &BoundTree {
        &self.tree
    }

    pub(super) const fn local_symbols(&self) -> &LocalSymbolSnapshot {
        &self.local_symbols
    }

    pub(super) fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }
}

fn local_region_matches(key: &BoundUnitKey, actual: &LocalSymbolRegionKey) -> bool {
    match key.data() {
        BoundUnitKeyData::CallableBody(declared) => {
            declared_region_matches(declared, LocalSymbolRegionRole::CallableBody, actual)
        }
        BoundUnitKeyData::AnonymousCallable(_) => anonymous_region_matches(key, actual),
        BoundUnitKeyData::RuntimeDefault(declared) => {
            let Some(fact) = runtime_default_fact(declared.owner().kind()) else {
                return false;
            };

            declared_region_matches(
                declared,
                LocalSymbolRegionRole::DeclarationFact(fact),
                actual,
            )
        }
        BoundUnitKeyData::ConstantTemplate(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            actual,
        ),
        BoundUnitKeyData::PredicateDefinition(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
            actual,
        ),
        BoundUnitKeyData::Constraint(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::GenericConstraints),
            actual,
        ),
        BoundUnitKeyData::ContractClause(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableContracts),
            actual,
        ),
    }
}

fn declared_region_matches(
    key: &DeclaredBoundUnitKey,
    role: LocalSymbolRegionRole,
    actual: &LocalSymbolRegionKey,
) -> bool {
    actual.owner() == key.owner()
        && actual.role() == role
        && actual.anchors() == [key.source().syntax()]
        && actual.ordinal().is_none()
}

fn anonymous_region_matches(key: &BoundUnitKey, actual: &LocalSymbolRegionKey) -> bool {
    let mut anchors = Vec::new();
    let mut current = key;

    let owner = loop {
        match current.data() {
            BoundUnitKeyData::AnonymousCallable(anonymous) => {
                anchors.push(anonymous.source().syntax());

                current = anonymous.enclosing();
            }
            BoundUnitKeyData::CallableBody(declared)
            | BoundUnitKeyData::RuntimeDefault(declared)
            | BoundUnitKeyData::ConstantTemplate(declared)
            | BoundUnitKeyData::PredicateDefinition(declared)
            | BoundUnitKeyData::Constraint(declared)
            | BoundUnitKeyData::ContractClause(declared) => break declared.owner(),
        }
    };

    anchors.reverse();

    actual.owner() == owner
        && actual.role() == LocalSymbolRegionRole::AnonymousCallable
        && actual.anchors() == anchors
        && actual.ordinal().is_none()
}

const fn runtime_default_fact(owner: SymbolKind) -> Option<SymbolFactKind> {
    match owner {
        SymbolKind::CallableParameterDefaultProvider => {
            Some(SymbolFactKind::CallableParameterDefault)
        }
        SymbolKind::StructFieldDefaultProvider => Some(SymbolFactKind::StructFieldDefault),
        SymbolKind::UnionPayloadDefaultProvider => Some(SymbolFactKind::UnionPayloadFieldDefault),
        _ => None,
    }
}

fn validate_nested_units(
    enclosing: &BoundUnitKey,
    nested_units: &[BoundUnitKey],
) -> Result<(), CheckedUnitBuildError> {
    for (index, nested) in nested_units.iter().enumerate() {
        let BoundUnitKeyData::AnonymousCallable(anonymous) = nested.data() else {
            return Err(CheckedUnitBuildError::InvalidNestedUnit { index });
        };

        if anonymous.enclosing() != enclosing {
            return Err(CheckedUnitBuildError::InvalidNestedUnit { index });
        }

        if index > 0 && nested_units[index - 1].source() >= nested.source() {
            return Err(CheckedUnitBuildError::NonCanonicalNestedUnits { index });
        }
    }

    Ok(())
}
