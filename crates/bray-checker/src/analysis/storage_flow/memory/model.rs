use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::storage_flow::model::StorageAbsence;
use bray_bound_tree::{BorrowCapabilityId, BoundExecutionSite};
use bray_symbols::TypeId;
use super::effect::Effect;
pub(super) use bray_bound_tree::{StorageContents as Contents, StorageCapturedTransition as Transfer};
use bray_bound_tree::{merge_storage_bindings as merge_stored, merge_storage_contents as merge_contents};

pub(in crate::analysis::storage_flow) use bray_bound_tree::{
    StorageOrigin as Origin, StoragePath as Place, StorageTarget as Target,
};

pub(in crate::analysis::storage_flow) type EffectOutcomes = bray_symbols::DependencyEffectOutcomes<Effect>;

pub(super) use bray_bound_tree::{
    StorageAddressValue as AddressValue, StorageDependencyCompleteness as DependencyCompleteness,
    StorageValueBinding as ValueBinding, StorageValueRetirement as Retirement,
};

/// Reference values are copied between slots. Contents and obligations belong to targets once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct State {
    pub(super) storage_slots: std::sync::Arc<[bray_bound_tree::StorageIdentity]>,
    pub(super) retirements: BTreeSet<(Target, Retirement)>,
    // Complete reference shapes, with whether the type contains no reference slots.
    pub(super) complete_reference_types: std::sync::Arc<BTreeMap<TypeId, bool>>,
    pub(super) entry_borrows:
        std::sync::Arc<BTreeMap<bray_bound_tree::StorageIdentityId, bray_symbols::BorrowKind>>,
    pub(super) owned_storage: std::sync::Arc<[bray_bound_tree::StorageIdentityId]>,
    pub(super) unaddressed_storage: std::sync::Arc<[bray_bound_tree::StorageIdentityId]>,
    // Bounds retain the producing slot and whether its evaluation has retired.
    pub(super) returned_inputs: BTreeMap<(Place, bool), AddressValue>,
    pub(super) values: BTreeMap<Target, ValueBinding>,
    pub(super) borrows: BTreeMap<BorrowCapabilityId, AddressValue>,
    pub(super) pending: BTreeMap<BoundExecutionSite, Transfer>,
    pub(super) contents: BTreeMap<Target, Contents>,
    // Consumed inline values remain available to the current operation's value transfers.
    // Proof queries read physical contents only. This snapshot never crosses an operation edge.
    pub(super) evaluated: BTreeMap<Target, Contents>,
}

impl State {
    pub(in crate::analysis::storage_flow) fn merge(
        &mut self,
        incoming: &Self,
        absent: &BTreeMap<Place, StorageAbsence>,
        incoming_absent: &BTreeMap<Place, StorageAbsence>,
    ) -> bool {
        assert!(
            self.evaluated.is_empty() && incoming.evaluated.is_empty(),
            "evaluated values must be transferred before joining control flow"
        );

        assert!(self.retirements.is_empty() && incoming.retirements.is_empty(), "captured descriptor retirements must publish before joining control flow");

        let no_payload = |absence: &BTreeMap<Place, StorageAbsence>, slot: &Target| {
            slot.storage_path().is_some_and(|slot| {
                absence.iter().any(|(path, reason)| {
                    *reason == StorageAbsence::NoPayload && path.contains_storage(slot)
                })
            })
        };

        let left_absent = |slot: &Target| no_payload(absent, slot);
        let right_absent = |slot: &Target| no_payload(incoming_absent, slot);

        let mut changed = merge_stored(
            &mut self.values,
            &incoming.values,
            left_absent,
            right_absent,
        );

        for (reference, roots) in &incoming.returned_inputs {
            let previous = self.returned_inputs.entry(reference.clone()).or_default();

            changed |= previous.merge(roots);
        }

        changed |= merge_references(&mut self.borrows, &incoming.borrows, |_| false, |_| false);

        let empty = Transfer::default();

        for (call, transfer) in &mut self.pending {
            changed |= transfer.merge(incoming.pending.get(call).unwrap_or(&empty));
        }

        for (call, transfer) in &incoming.pending {
            if !self.pending.contains_key(call) {
                let mut joined = Transfer::default();

                changed |= joined.merge(transfer);
                self.pending.insert(*call, joined);
            }
        }

        changed |= merge_contents(&mut self.contents, &incoming.contents);

        changed
    }
}

fn merge_references<K: Clone + Ord>(
    target: &mut BTreeMap<K, AddressValue>,
    incoming: &BTreeMap<K, AddressValue>,
    target_absent: impl Fn(&K) -> bool,
    incoming_absent: impl Fn(&K) -> bool,
) -> bool {
    let mut changed = false;

    for (slot, references) in target.iter_mut() {
        match incoming.get(slot) {
            Some(incoming) if target_absent(slot) => {
                if references != incoming {
                    // The inactive predecessor contributes no address alternative.
                    // Each value owns its alternatives and shares immutable target paths.
                    *references = incoming.clone();
                    changed = true;
                }
            }
            Some(_) if incoming_absent(slot) => {}
            Some(incoming) => {
                changed |= references.merge(incoming);
            }
            None if !references.is_unknown() && !incoming_absent(slot) => {
                references.forget_value();
                changed = true;
            }
            None => {}
        }
    }

    for (slot, references) in incoming {
        if !target.contains_key(slot) {
            // Joined slots own their target alternatives independently of predecessor state.
            let mut references = references.clone();

            if !target_absent(slot) { references.forget_value(); }
            target.insert(slot.clone(), references);
            changed = true;
        }
    }

    changed
}

/// Whether a boundary origin is known independently of its exact cell identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) enum ReferenceBounds {
    Known,
    Displaced,
    /// Possible call-input regions, without established borrow provenance.
    InputRegions,
    Unknown,
}

pub(in crate::analysis::storage_flow) enum ReferenceExitValue<'a> {
    Address {
        targets: &'a BTreeSet<Target>,
        nullable: bool,
        bounds: ReferenceBounds,
    },
    NoPayload,
}

/// Reference values at a unit boundary, with the live-payload conditions used by the same join.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis::storage_flow) struct ReferenceExit {
    pub(super) slots: BTreeMap<Place, ValueBinding>,
    pub(super) stored: BTreeMap<Target, ValueBinding>,
    pub(super) absent: BTreeSet<Place>,
    pub(super) input_regions: BTreeSet<Place>,
}

impl ReferenceExit {
    pub(in crate::analysis::storage_flow) fn absent_payloads(&self) -> &BTreeSet<Place> {
        &self.absent
    }

    pub(in crate::analysis::storage_flow) fn whole_sources(&self) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.stored.iter().filter_map(|(target, value)| value.whole_sources().map(|(sources, complete)| (target, sources, complete)))
    }

    pub(in crate::analysis::storage_flow) fn stored_bindings(&self) -> impl Iterator<Item = (&Target, ReferenceExitValue<'_>)> {
        self.stored.iter().flat_map(|(slot, value)| {
            value.is_no_payload().then_some((slot, ReferenceExitValue::NoPayload)).into_iter()
                .chain(value.possible_references().map(move |references| {
                    (slot, ReferenceExitValue::Address {
                        targets: &references.targets,
                        nullable: value.address.as_ref().is_some_and(AddressValue::has_empty_alternative),
                        bounds: if references.is_unknown() { ReferenceBounds::Unknown }
                            else if references.is_displaced() { ReferenceBounds::Displaced }
                            else { ReferenceBounds::Known },
                    })
                }))
        })
    }

    pub(in crate::analysis::storage_flow) fn empty_slots(&self) -> impl Iterator<Item = &Place> {
        self.slots.iter().filter_map(|(slot, value)| {
            let references = value.address.as_ref()?;

            references.has_empty_alternative().then_some(slot)
        })
    }

    pub(in crate::analysis::storage_flow) fn no_payload_slots(&self) -> impl Iterator<Item = &Place> {
        self.slots.iter().filter_map(|(slot, value)| {
            (value.is_no_payload() || self.absent.iter().any(|absent| absent.contains_storage(slot))).then_some(slot)
        })
    }

    pub(in crate::analysis::storage_flow) fn merge(&mut self, incoming: &Self) {
        merge_stored(
            &mut self.stored,
            &incoming.stored,
            |target| target.storage_path().is_some_and(|slot| self.absent.iter().any(|path| path.contains_storage(slot))),
            |target| target.storage_path().is_some_and(|slot| incoming.absent.iter().any(|path| path.contains_storage(slot))),
        );

        merge_stored(
            &mut self.slots,
            &incoming.slots,
            |slot| self.absent.iter().any(|path| path.contains_storage(slot)),
            |slot| incoming.absent.iter().any(|path| path.contains_storage(slot)),
        );

        self.input_regions
            .extend(incoming.input_regions.iter().cloned());

        // Retain only absence established by both predecessors, including enclosing payloads.
        self.absent = self
            .absent
            .iter()
            .chain(&incoming.absent)
            .filter(|slot| {
                self.absent.iter().any(|path| path.contains(slot))
                    && incoming.absent.iter().any(|path| path.contains(slot))
            })
            .cloned()
            .collect();
    }

    pub(in crate::analysis::storage_flow) fn targets(
        &self,
    ) -> impl Iterator<Item = (&Place, &Origin, bool, ReferenceBounds)> {
        self.slots.iter().filter_map(|(slot, value)| Some((slot, value.reference()?))).flat_map(move |(slot, references)| {
            references.targets.iter().map(move |target| {
                (
                    slot,
                    &target.origin,
                    references.is_exact(),
                    if references.is_unknown() {
                        ReferenceBounds::Unknown
                    } else if self.input_regions.contains(slot) {
                        ReferenceBounds::InputRegions
                    } else if references.is_displaced() {
                        ReferenceBounds::Displaced
                    } else {
                        ReferenceBounds::Known
                    },
                )
            })
        })
    }
}
