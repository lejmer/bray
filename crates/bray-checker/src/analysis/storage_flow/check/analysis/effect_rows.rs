use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;

use bray_bound_tree::StorageEffectRow;
use bray_symbols::{
    CapturedFailureOutcome, CapturedValueOutcome, DependencyEffectOutcomes, DependencyFailureKind,
    DependencyStorageEffect,
};

use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes};

pub(super) type EffectRows = BTreeMap<StorageEffectRow, Arc<[Effect]>>;

/// Associates checked portable rows with their effects until the admitted operands are bound.
pub(super) struct SelectedEffectRows<'a> {
    filter_normal: bool,
    has_failure_table: bool,
    failure_fallback: [bool; 2],
    sources: BTreeMap<&'a DependencyStorageEffect, Vec<StorageEffectRow>>,
    retained: BTreeMap<StorageEffectRow, Vec<Effect>>,
}

impl<'a> SelectedEffectRows<'a> {
    pub(super) fn new(filter_normal: bool) -> Self {
        Self {
            filter_normal,
            has_failure_table: false,
            failure_fallback: [false; 2],
            sources: BTreeMap::new(),
            retained: BTreeMap::new(),
        }
    }

    pub(super) fn select_failures<Guard, Report, Write>(
        &mut self,
        rows: Option<&'a [CapturedFailureOutcome<Guard, Report, DependencyStorageEffect, Write>]>,
        mut admitted: impl FnMut(usize) -> bool,
        mut retain: impl FnMut(StorageEffectRow) -> bool,
    ) {
        let Some(rows) = rows else { return; };

        self.has_failure_table = true;

        for (index, row) in rows.iter().enumerate().filter(|(index, _)| admitted(*index)) {
            for kind in DependencyFailureKind::ALL.into_iter().filter(|kind| row.covers(*kind)) {
                let key = StorageEffectRow::Abnormal(kind, index);
                let retain = retain(key);

                if let Some(effects) = &row.effects {
                    self.select(key, effects, retain);
                } else {
                    self.failure_fallback[kind.index()] = true;
                }

                if retain && let Some(cells) = &row.cell_changes {
                    self.select(StorageEffectRow::AbnormalCells(kind, index), cells, true);
                }
            }
        }
    }

    pub(super) fn select_normal(&mut self, index: usize, row: &'a CapturedValueOutcome, retain: bool) {
        if retain || self.filter_normal {
            self.select(StorageEffectRow::Normal(index), &row.effects, retain);
        }

        if let Some(cells) = &row.cell_changes {
            self.select(StorageEffectRow::NormalCells(index), cells, true);
        }
    }

    fn select(
        &mut self,
        row: StorageEffectRow,
        effects: &'a [DependencyStorageEffect],
        retain: bool,
    ) {
        if retain {
            self.retained.insert(row, Vec::new());
        }

        for effect in effects {
            self.sources.entry(effect).or_default().push(row);
        }
    }

    pub(super) fn bind(
        mut self,
        templates: &DependencyEffectOutcomes<DependencyStorageEffect>,
        mut resolve: impl FnMut(&DependencyStorageEffect) -> Vec<Effect>,
    ) -> (EffectOutcomes, EffectRows) {
        let additional = self.sources.iter().filter_map(|(&effect, rows)| {
            rows.iter().any(|row| !matches!(row, StorageEffectRow::Normal(_))).then_some(effect)
        }).collect::<Vec<_>>();

        let outcomes = templates.try_flat_map_selected(self.has_failure_table, additional, |effect, in_envelope| {
            let resolved = resolve(effect);
            let mut normal = !self.filter_normal;

            let mut failure = DependencyFailureKind::ALL.map(|kind| {
                self.failure_fallback[kind.index()] && templates.failure(kind).binary_search(effect).is_ok()
            });

            let mut possible = in_envelope;

            for row in self.sources.remove(effect).into_iter().flatten() {
                match row {
                    StorageEffectRow::Normal(_) => normal = true,
                    StorageEffectRow::Abnormal(kind, _) => {
                        failure[kind.index()] = true;
                        possible = true;
                    }
                    StorageEffectRow::NormalCells(_) | StorageEffectRow::AbnormalCells(..) => {
                        if let [update] = resolved.as_slice() && update.cell_change().is_some() {
                            self.retained.entry(row).or_default().push(update.clone());
                        }

                        continue;
                    }
                }

                if let Some(effects) = self.retained.get_mut(&row) {
                    effects.extend(resolved.iter().cloned());
                }
            }

            Ok::<_, Infallible>((normal, failure, if possible { resolved } else { Vec::new() }))
        }).expect("binding retains an explicit conservative effect for unresolved storage");

        let rows = self.retained.into_iter().map(|(row, effects)| {
            (row, bray_base::sorted_unique_shared_slice(effects))
        }).collect();

        (outcomes, rows)
    }
}
