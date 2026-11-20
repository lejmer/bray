use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{StorageEffect, StorageTarget, StorageValueRetirement};

type EffectOutcomes = bray_symbols::DependencyEffectOutcomes<StorageEffect>;

/// Checked identity of one producer carried by a future or task.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageDeferredProducer {
    /// A frame formed by a checked invocation in this unit.
    Local(crate::BoundExecutionSite),
    /// A frame exported as part of another invocation's captured value.
    Summary {
        /// The checked transition that instantiated the captured value.
        instantiation: crate::StorageMutation,
        /// The portable selected callable, when known.
        callee: Option<Arc<bray_symbols::CapturedCallable>>,
        /// The position of the frame in its captured value record.
        alternative: usize,
    },
}

impl StorageDeferredProducer {
    /// Returns the local formation site of a directly created frame.
    pub fn local(&self) -> Option<crate::BoundExecutionSite> {
        match self {
            Self::Local(site) => Some(*site),
            Self::Summary { .. } => None,
        }
    }
}

impl From<crate::BoundExecutionSite> for StorageDeferredProducer {
    fn from(site: crate::BoundExecutionSite) -> Self { Self::Local(site) }
}

impl From<crate::BoundExpressionId> for StorageDeferredProducer {
    fn from(expression: crate::BoundExpressionId) -> Self { Self::Local(expression.into()) }
}

/// A producer's portable outcome row with effects resolved at activation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageEffectRow {
    /// A selected normal result row.
    Normal(usize),
    /// A selected prefix leading to an abnormal task result.
    Abnormal(bray_symbols::DependencyFailureKind, usize),
    /// Exact cells changed on a selected normal exit.
    NormalCells(usize),
    /// Exact cells changed before a selected abnormal exit.
    AbnormalCells(bray_symbols::DependencyFailureKind, usize),
}

impl StorageEffectRow {
    fn is_cell_change(self) -> bool {
        matches!(self, Self::NormalCells(_) | Self::AbnormalCells(..))
    }
}

/// Resolved effects of one frame and its remaining cleanup duties.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageDeferredTransition {
    /// Effects of executing the frame after admission.
    pub effects: Option<EffectOutcomes>,
    /// Activation-time effects performed by admitted portable outcome rows.
    pub effect_rows: Option<BTreeMap<StorageEffectRow, Arc<[StorageEffect]>>>,
    /// Cleanup effects of retiring the frame's input environment.
    pub captured_cleanup: Option<BTreeSet<StorageEffect>>,
    /// Cleanup effects of discarding an unobserved result.
    pub result_cleanup: Option<BTreeSet<StorageEffect>>,
}

impl StorageDeferredTransition {
    /// Joins alternative captures of one producer. Missing knowledge stays unknown.
    pub fn merge_alternative(current: &mut Option<Arc<Self>>, incoming: &Option<Arc<Self>>) -> bool {
        let Some(previous) = current else { return false; };

        let Some(incoming) = incoming else {
            *current = None;

            return true;
        };

        previous != incoming && Arc::make_mut(previous).merge(incoming)
    }

    fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = false;

        for (effects, incoming) in [
            (&mut self.captured_cleanup, &incoming.captured_cleanup),
            (&mut self.result_cleanup, &incoming.result_cleanup),
        ] {
            if let Some(current) = effects {
                if let Some(incoming) = incoming {
                    let before = current.len();

                    current.extend(incoming.iter().cloned());
                    changed |= before != current.len();
                } else {
                    *effects = None;
                    changed = true;
                }
            }
        }

        if let Some(current) = &mut self.effects {
            if let Some(incoming) = &incoming.effects {
                changed |= current.merge(incoming);
            } else {
                self.effects = None;
                changed = true;
            }
        }

        if let Some(rows) = &mut self.effect_rows {
            if let Some(incoming) = &incoming.effect_rows {
                let prior_count = rows.len();

                rows.retain(|row, current| {
                    if !row.is_cell_change() {
                        return true;
                    }

                    let Some(common) = bray_symbols::common_cell_changes(
                        [Some(&*current), incoming.get(row)].into_iter()
                    ) else {
                        return false;
                    };

                    changed |= *current != common;
                    *current = common;

                    true
                });

                changed |= rows.len() != prior_count;

                for (row, effects) in incoming.iter().filter(|(row, _)| !row.is_cell_change()) {
                    match rows.entry(*row) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(Arc::clone(effects));
                            changed = true;
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            let previous = entry.get_mut();

                            if previous != effects {
                                let merged = bray_base::sorted_unique_shared_slice(previous.iter().chain(&**effects).cloned());

                                changed |= &merged != previous;
                                *previous = merged;
                            }
                        }
                    }
                }
            } else {
                self.effect_rows = None;
                changed = true;
            }
        }

        changed
    }

    /// Applies the reached retirements to the frame's effects and cleanup duties.
    pub fn retired(&self, retirements: &[(StorageTarget, StorageValueRetirement)]) -> Option<Self> {
        let affected = |effect: &StorageEffect| retirements.iter().any(|(current, retirement)| {
            effect.references(current, *retirement)
        });

        let changed = self.effects.iter().flat_map(|effects| effects.possible())
            .chain(self.effect_rows.iter().flat_map(BTreeMap::values).flat_map(|row| row.iter()))
            .chain([&self.captured_cleanup, &self.result_cleanup].into_iter().flatten().flatten())
            .any(&affected);

        if !changed { return None; }

        let retire = |mut effect: StorageEffect| {
            for (current, retirement) in retirements {
                effect.retire(current, *retirement);
            }

            effect
        };

        let mut updated = self.clone();

        if let Some(effects) = &mut updated.effects {
            *effects = effects.try_map(|effect| Ok::<_, std::convert::Infallible>(retire(effect.clone())))
                .expect("retirement preserves resolved effect outcomes");
        }

        if let Some(rows) = &mut updated.effect_rows {
            rows.retain(|row, effects| {
                if !effects.iter().any(&affected) { return true; }

                *effects = bray_base::sorted_unique_shared_slice(effects.iter()
                    .filter(|effect| !row.is_cell_change() || !affected(effect))
                    .cloned().map(&retire));

                !row.is_cell_change() || !effects.is_empty()
            });
        }

        for effects in [&mut updated.captured_cleanup, &mut updated.result_cleanup].into_iter().flatten() {
            if effects.iter().any(&affected) {
                *effects = std::mem::take(effects).into_iter().map(&retire).collect();
            }
        }

        Some(updated)
    }
}
