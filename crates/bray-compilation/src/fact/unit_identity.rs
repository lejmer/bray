use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Mutex;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKind};
use bray_declarations::SyntaxAnchor;
use bray_syntax::{SyntaxTree, SyntaxWalkEvent, walk_syntax_tree};

use super::FactQueryError;

#[derive(Debug)]
pub(crate) struct BoundUnitIdentityMap {
    source_ordinals: BTreeMap<BoundSourceAnchor, u32>,
    claimed_units: Mutex<BTreeMap<BoundUnitId, BoundUnitKey>>,
}

impl BoundUnitIdentityMap {
    pub(crate) fn from_syntax(syntax: &SyntaxTree) -> Result<Self, FactQueryError> {
        let mut source_ordinals = BTreeMap::new();
        let mut overflowed = false;

        walk_syntax_tree(syntax, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return bray_syntax::SyntaxWalkControl::Continue;
            };

            let source =
                BoundSourceAnchor::new(SyntaxAnchor::from_node(&node), node.source().version());

            let Ok(ordinal) = u32::try_from(source_ordinals.len()) else {
                overflowed = true;

                return bray_syntax::SyntaxWalkControl::Stop;
            };

            if let Entry::Vacant(entry) = source_ordinals.entry(source) {
                entry.insert(ordinal);
            }

            bray_syntax::SyntaxWalkControl::Continue
        });

        if overflowed {
            return Err(FactQueryError::InfrastructureFailure);
        }

        Ok(Self {
            source_ordinals,
            claimed_units: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn unit_id(&self, key: &BoundUnitKey) -> Result<BoundUnitId, FactQueryError> {
        let Some(source_ordinal) = self.source_ordinals.get(&key.source()).copied() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let raw = source_ordinal
            .checked_mul(UNIT_KIND_COUNT)
            .and_then(|base| base.checked_add(unit_kind_ordinal(key.kind())))
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let unit = BoundUnitId::new(raw);

        let mut claimed_units = self
            .claimed_units
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if let Some(existing) = claimed_units.get(&unit) {
            return if existing == key {
                Ok(unit)
            } else {
                Err(FactQueryError::InfrastructureFailure)
            };
        }

        // The collision registry shares the key's Arc-backed immutable identity.
        claimed_units.insert(unit, key.clone());

        Ok(unit)
    }
}

const UNIT_KIND_COUNT: u32 = 7;

const fn unit_kind_ordinal(kind: BoundUnitKind) -> u32 {
    match kind {
        BoundUnitKind::CallableBody => 0,
        BoundUnitKind::AnonymousCallable => 1,
        BoundUnitKind::RuntimeDefault => 2,
        BoundUnitKind::ConstantTemplate => 3,
        BoundUnitKind::PredicateDefinition => 4,
        BoundUnitKind::Constraint => 5,
        BoundUnitKind::ContractClause => 6,
    }
}
