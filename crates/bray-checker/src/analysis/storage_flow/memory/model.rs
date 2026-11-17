use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::storage_flow::model::StorageAbsence;
use bray_bound_tree::{BorrowCapabilityId, BoundExecutionSite, BoundExpressionId};
use bray_symbols::TypeId;
use super::effect::{CapturedEffects, Effect};

pub(in crate::analysis::storage_flow) use bray_bound_tree::{
    StorageOrigin as Origin, StoragePath as Place, StorageTarget as Target,
};

pub(in crate::analysis::storage_flow) type EffectOutcomes = bray_symbols::DependencyEffectOutcomes<Effect>;

#[derive(Clone, Copy)]
pub(super) enum Retirement {
    Storage,
    Producer,
}

impl Retirement {
    pub(super) fn for_payload(target: &Target) -> Self {
        if target.projections().is_empty()
            && matches!(target.origin, Origin::Returned { .. } | Origin::Allocation { .. })
        {
            Self::Producer
        } else {
            Self::Storage
        }
    }

    pub(super) fn contains(self, current: &Target, target: &Target) -> bool {
        match self {
            Self::Storage => current.contains_storage(target),
            Self::Producer => current.contains(target),
        }
    }
}

pub(super) use super::value::{AddressValue, DependencyCompleteness, ValueBinding};

type Origins = BTreeSet<BoundExpressionId>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Contents {
    pub(super) exposed: bool,
    pub(super) escaped: bool,
    pub(super) storage_type: Option<TypeId>,
    pub(super) initialized: BTreeSet<TypeId>,
    pub(super) pending: BTreeMap<TypeId, Origins>,
    pub(super) allocations: Origins,
    pub(super) invalidations: Origins,
}

impl Contents {
    pub(super) fn merge(&mut self, incoming: &Self, target: &Target) -> bool {
        let previous_exposure = self.exposed;
        let previous_escape = self.escaped;

        self.exposed |= incoming.exposed;
        self.escaped |= incoming.escaped;

        let previous_type = self.storage_type;

        match (self.storage_type, incoming.storage_type) {
            (Some(left), Some(right)) => assert_eq!(
                left, right,
                "one physical storage path has one declared type: {target:?}"
            ),
            (None, Some(ty)) => self.storage_type = Some(ty),
            _ => {}
        }

        let count = self.initialized.len();

        self.initialized
            .retain(|ty| incoming.initialized.contains(ty));

        let mut changed = self.initialized.len() != count
            || self.storage_type != previous_type
            || self.exposed != previous_exposure
            || self.escaped != previous_escape;

        for (&ty, origins) in &incoming.pending {
            changed |= merge_origins(self.pending.entry(ty).or_default(), origins);
        }

        changed |= merge_origins(&mut self.allocations, &incoming.allocations);
        changed |= merge_origins(&mut self.invalidations, &incoming.invalidations);

        changed
    }
}

fn merge_origins(target: &mut Origins, incoming: &Origins) -> bool {
    let count = target.len();

    target.extend(incoming.iter().copied());

    target.len() != count
}

/// A value captured before consumption, with inline protected payload state at its destination.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Transfer {
    // Possible destinations receive a weak merge, never definite replacement or initialization.
    pub(super) weak_outputs: bool,
    /// Evaluated reference descriptors retained by this deferred frame, separate from published outputs.
    pub(super) inputs: BTreeMap<Target, ValueBinding>,
    pub(super) values: BTreeMap<Target, ValueBinding>,
    pub(super) payloads: BTreeMap<Target, Contents>,
    pub(super) failure_effects: Option<std::sync::Arc<CapturedEffects>>,
    pub(super) deferred_effects: Option<super::EffectOutcomes>,
    pub(super) captured_cleanup: Option<BTreeSet<Effect>>,
    pub(super) result_cleanup: Option<BTreeSet<Effect>>,
    pub(super) deferred: super::references::DeferredTransitions,
    pub(super) returned_inputs: BTreeMap<Place, AddressValue>,
}

impl Transfer {
    /// Production consumes output snapshots while entry captures remain available to outcomes.
    pub(super) fn take_outputs(&mut self) -> Self {
        Self {
            weak_outputs: std::mem::take(&mut self.weak_outputs),
            values: std::mem::take(&mut self.values),
            payloads: std::mem::take(&mut self.payloads),
            deferred: std::mem::take(&mut self.deferred),
            returned_inputs: std::mem::take(&mut self.returned_inputs),
            ..Self::default()
        }
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let changed = merge_stored(&mut self.inputs, &incoming.inputs, |_| false, |_| false);
        let changed = merge_stored(&mut self.values, &incoming.values, |_| false, |_| false) || changed;

        let mut changed = merge_contents(&mut self.payloads, &incoming.payloads) || changed;

        changed |= !self.weak_outputs && incoming.weak_outputs;
        self.weak_outputs |= incoming.weak_outputs;

        changed |= super::references::merge_deferred(&mut self.deferred, &incoming.deferred);

        for (reference, bounds) in &incoming.returned_inputs {
            changed |= self
                .returned_inputs
                .entry(reference.clone())
                .or_default()
                .merge(bounds);
        }

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

        if let Some(current) = &mut self.deferred_effects {
            if let Some(incoming) = &incoming.deferred_effects {
                changed |= current.merge(incoming);
            } else {
                self.deferred_effects = None;
                changed = true;
            }
        }

        if let Some(incoming) = &incoming.failure_effects {
            if let Some(current) = &mut self.failure_effects {
                if current != incoming {
                    // Outcome branches share immutable captures until a join adds possible effects.
                    let mut merged = current.as_ref().clone();

                    merged.merge(incoming);
                    changed |= &merged != current.as_ref();
                    *current = std::sync::Arc::new(merged);
                }
            } else {
                self.failure_effects = Some(std::sync::Arc::clone(incoming));
                changed = true;
            }
        }

        changed
    }
}

/// Reference values are copied between slots. Contents and obligations belong to targets once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct State {
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
    pub(super) deferred: super::references::DeferredTransitions,
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

        changed |= super::references::merge_deferred(&mut self.deferred, &incoming.deferred);

        changed |= merge_contents(&mut self.contents, &incoming.contents);

        changed
    }
}

fn merge_contents(
    target: &mut BTreeMap<Target, Contents>,
    incoming: &BTreeMap<Target, Contents>,
) -> bool {
    let mut changed = false;

    for (key, contents) in target.iter_mut() {
        match incoming.get(key) {
            Some(incoming) => changed |= contents.merge(incoming, key),
            None => {
                changed |= !contents.initialized.is_empty();
                contents.initialized.clear();
            }
        }
    }

    for (key, contents) in incoming {
        if !target.contains_key(key) {
            // Joined contents retain possible obligations, but no one-sided initialization proof.
            let mut contents = contents.clone();

            contents.initialized.clear();
            target.insert(key.clone(), contents);
            changed = true;
        }
    }

    changed
}

fn merge_stored<K: Clone + Ord>(
    target: &mut BTreeMap<K, ValueBinding>,
    incoming: &BTreeMap<K, ValueBinding>,
    target_absent: impl Fn(&K) -> bool,
    incoming_absent: impl Fn(&K) -> bool,
) -> bool {
    let mut changed = false;

    for (slot, value) in target.iter_mut() {
        let previous = value.clone();

        match incoming.get(slot) {
            Some(other) if target_absent(slot) => *value = other.clone(),
            Some(_) if incoming_absent(slot) => {},
            Some(other) => {
                value.join(other);
            }
            None if !incoming_absent(slot) => {
                value.lose_information();
            }
            None => {}
        }

        changed |= *value != previous;
    }

    for (slot, value) in incoming {
        if !target.contains_key(slot) {
            let mut value = value.clone();

            if !target_absent(slot) {
                value.lose_information();
            }

            target.insert(slot.clone(), value);
            changed = true;
        }
    }

    changed
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
