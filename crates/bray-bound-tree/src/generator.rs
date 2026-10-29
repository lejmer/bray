use crate::{
    BoundExpressionId, StorageAccessId, StorageBinding, StorageBindingTarget, StoragePlan,
};
use bray_symbols::LocalBindingSymbolId;
use std::collections::{BTreeMap, BTreeSet};

/// Proves that a Boolean generator's value is observed exclusively through selected Boolean folds.
/// The producer must still run eagerly and completely. Only collection storage may be eliminated.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct GeneratorObservations {
    aliases: BTreeMap<BoundExpressionId, BoundExpressionId>,
    bindings: BTreeMap<LocalBindingSymbolId, BoundExpressionId>,
    folds: BTreeMap<BoundExpressionId, BoundExpressionId>,
    members: BTreeSet<BoundExpressionId>,
}

impl GeneratorObservations {
    /// Whether the body contains no eligible generator observations.
    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }

    /// Retains the alias and fold relationships established by checked source analysis.
    pub fn from_checked_parts(
        aliases: BTreeMap<BoundExpressionId, BoundExpressionId>,
        bindings: BTreeMap<LocalBindingSymbolId, BoundExpressionId>,
        folds: BTreeMap<BoundExpressionId, BoundExpressionId>,
        members: BTreeSet<BoundExpressionId>,
    ) -> Self {
        Self {
            aliases,
            bindings,
            folds,
            members,
        }
    }

    /// Returns the eager producer whose evaluation supplies this alias.
    pub fn producer(&self, expression: BoundExpressionId) -> Option<BoundExpressionId> {
        self.aliases.get(&expression).copied()
    }

    /// Returns the eager producer observed by this exact selected fold.
    pub fn fold(&self, expression: BoundExpressionId) -> Option<BoundExpressionId> {
        self.folds.get(&expression).copied()
    }

    /// Whether an expression belongs to the established observation.
    pub fn contains(&self, expression: BoundExpressionId) -> bool {
        self.members.contains(&expression)
    }

    /// Whether the observed value has no collection storage to clean up.
    pub fn eliminates(&self, storage: &StoragePlan, access: StorageAccessId) -> bool {
        let Some(root) = storage.root_identity(access) else {
            return false;
        };

        if storage
            .identity(root)
            .and_then(|identity| identity.definition_node())
            .is_some_and(|node| match node {
                crate::AnyBoundNodeId::Expression(expression) => {
                    self.producer(expression).is_some()
                }
                _ => false,
            })
        {
            return true;
        }

        storage.binding_targets(StorageBinding::Identity(root)).any(|target| matches!(target, StorageBindingTarget::Local(binding) if self.bindings.contains_key(&binding)))
    }
}
