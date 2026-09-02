use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Mutex;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKind};
use bray_declarations::SyntaxAnchor;
use bray_syntax::{SyntaxTree, SyntaxWalkEvent, walk_syntax_tree};

use super::{FactQueryError, FactRuntimeFailure};

#[derive(Debug)]
pub(crate) struct BoundUnitIdentityMap {
    source_ordinals: BTreeMap<BoundSourceAnchor, u32>,
    claimed_units: Mutex<BTreeMap<BoundUnitId, BoundUnitKey>>,
}

impl BoundUnitIdentityMap {
    pub(crate) fn from_syntax(syntax: &SyntaxTree) -> Result<Self, FactQueryError> {
        let mut source_ordinals = BTreeMap::new();
        let mut overflowed = None;

        walk_syntax_tree(syntax, |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return bray_syntax::SyntaxWalkControl::Continue;
            };

            let source =
                BoundSourceAnchor::new(SyntaxAnchor::from_node(&node), node.source().version());

            let Ok(ordinal) = u32::try_from(source_ordinals.len()) else {
                overflowed = Some(source_ordinals.len());

                return bray_syntax::SyntaxWalkControl::Stop;
            };

            if let Entry::Vacant(entry) = source_ordinals.entry(source) {
                entry.insert(ordinal);
            }

            bray_syntax::SyntaxWalkControl::Continue
        });

        if let Some(source_count) = overflowed {
            return Err(FactRuntimeFailure::UnitSourceCapacityExhausted { source_count }.into());
        }

        Ok(Self {
            source_ordinals,
            claimed_units: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn unit_id(&self, key: &BoundUnitKey) -> Result<BoundUnitId, FactQueryError> {
        // Identity failures escape this borrowed lookup and therefore own their exact unit keys.
        let Some(source_ordinal) = self.source_ordinals.get(&key.source()).copied() else {
            return Err(FactRuntimeFailure::UnknownUnitSource { unit: key.clone() }.into());
        };

        let Some(raw) = source_ordinal
            .checked_mul(UNIT_KIND_COUNT)
            .and_then(|base| base.checked_add(unit_kind_ordinal(key.kind())))
        else {
            return Err(FactRuntimeFailure::UnitIdentityCapacityExhausted {
                unit: key.clone(),
                source_ordinal,
            }
            .into());
        };

        let unit = BoundUnitId::new(raw);

        let mut claimed_units = self
            .claimed_units
            .lock()
            .map_err(|_| FactRuntimeFailure::UnitIdentityStatePoisoned {
                unit: key.clone(),
            })?;

        if let Some(existing) = claimed_units.get(&unit) {
            return if existing == key {
                Ok(unit)
            } else {
                Err(FactRuntimeFailure::UnitIdentityCollision {
                    identity: unit,
                    expected: existing.clone(),
                    actual: key.clone(),
                }
                .into())
            };
        }

        // The collision registry shares the key's Arc-backed immutable identity.
        claimed_units.insert(unit, key.clone());

        Ok(unit)
    }
}

const UNIT_KIND_COUNT: u32 = 9;

const fn unit_kind_ordinal(kind: BoundUnitKind) -> u32 {
    match kind {
        BoundUnitKind::CallableBody => 0,
        BoundUnitKind::AnonymousCallable => 1,
        BoundUnitKind::RuntimeDefault => 2,
        BoundUnitKind::ConstantTemplate => 3,
        BoundUnitKind::EmbeddedConstant => 4,
        BoundUnitKind::PredicateDefinition => 5,
        BoundUnitKind::Constraint => 6,
        BoundUnitKind::ContractClause => 7,
        BoundUnitKind::TargetGate => 8,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Mutex;

    use bray_bound_tree::BoundUnitId;

    use super::BoundUnitIdentityMap;
    use crate::fact::{FactQueryError, FactRuntimeFailure};
    use crate::test_support::callable_body_key;

    #[test]
    fn unknown_unit_source_retains_the_unit_key() {
        let key = callable_body_key(0);

        let identities = BoundUnitIdentityMap {
            source_ordinals: BTreeMap::new(),
            claimed_units: Mutex::new(BTreeMap::new()),
        };

        let error = match identities.unit_id(&key) {
            Ok(_) => panic!("an unknown unit source must not receive an identity"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::UnknownUnitSource { unit } if unit == &key
                )
        ));
    }

    #[test]
    fn unit_identity_capacity_retains_source_ordinal_and_key() {
        let key = callable_body_key(1);

        let identities = BoundUnitIdentityMap {
            source_ordinals: BTreeMap::from([(key.source(), u32::MAX)]),
            claimed_units: Mutex::new(BTreeMap::new()),
        };

        let error = match identities.unit_id(&key) {
            Ok(_) => panic!("an overflowing source ordinal must not receive an identity"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::UnitIdentityCapacityExhausted {
                        unit,
                        source_ordinal: u32::MAX,
                    } if unit == &key
                )
        ));
    }

    #[test]
    fn unit_identity_collision_retains_both_unit_keys() {
        let requested = callable_body_key(2);
        let existing = callable_body_key(3);

        let identities = BoundUnitIdentityMap {
            source_ordinals: BTreeMap::from([(requested.source(), 0)]),
            claimed_units: Mutex::new(BTreeMap::from([(
                BoundUnitId::new(0),
                existing.clone(),
            )])),
        };

        let error = match identities.unit_id(&requested) {
            Ok(_) => panic!("a colliding unit key must not receive an identity"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::UnitIdentityCollision {
                        identity,
                        expected,
                        actual,
                    } if *identity == BoundUnitId::new(0)
                        && expected == &existing
                        && actual == &requested
                )
        ));
    }

    #[test]
    fn poisoned_unit_identity_state_retains_the_requested_key() {
        let key = callable_body_key(4);

        let identities = BoundUnitIdentityMap {
            source_ordinals: BTreeMap::from([(key.source(), 0)]),
            claimed_units: Mutex::new(BTreeMap::new()),
        };

        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _claimed = identities
                .claimed_units
                .lock()
                .unwrap_or_else(|_| panic!("test identity map should begin available"));

            panic!("poison unit identity state");
        }));

        let error = match identities.unit_id(&key) {
            Ok(_) => panic!("identity lookup must report poisoned state"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::UnitIdentityStatePoisoned { unit }
                        if unit == &key
                )
        ));
    }
}
