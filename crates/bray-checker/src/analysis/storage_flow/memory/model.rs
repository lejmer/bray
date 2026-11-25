use std::sync::Arc;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::StorageAbsence;
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

/// A reached place and the loan that authorized its evaluation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct ReachedAccess {
    pub(in crate::analysis::storage_flow) address: AddressValue,
    pub(in crate::analysis::storage_flow) loan: Option<BorrowCapabilityId>,
}

impl ReachedAccess {
    fn merge(&mut self, other: &Self) -> bool {
        let changed = self.address.merge(&other.address);

        if self.loan.is_some() && self.loan != other.loan {
            self.loan = None;

            return true;
        }

        changed
    }

    pub(super) fn retire(&mut self, current: &Target, retirement: Retirement) {
        self.address.retire(current, retirement);

        if self.address.targets.iter().any(|target| target.previous) {
            self.loan = None;
        }
    }
}

/// Live borrow evidence retains its admitting ancestors with its reached address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RetainedBorrow {
    pub(super) address: AddressValue,
    pub(super) ancestors: std::sync::Arc<[BorrowCapabilityId]>,
}

impl RetainedBorrow {
    fn merge(&mut self, other: &Self) -> bool {
        let changed = self.address.merge(&other.address);

        if self.ancestors.iter().any(|ancestor| !other.ancestors.contains(ancestor)) {
            self.ancestors = self.ancestors.iter().copied().filter(|ancestor| other.ancestors.contains(ancestor)).collect();

            return true;
        }

        changed
    }

    fn forget(&mut self) -> bool {
        let changed = !self.address.is_unknown() || !self.ancestors.is_empty();

        self.address.forget_value();
        self.ancestors = Default::default();

        changed
    }

    pub(super) fn retire(&mut self, current: &Target, retirement: Retirement) {
        self.address.retire(current, retirement);
    }
}

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
    // Competing outcomes share the admitted value bindings until they write.
    pub(super) values: Arc<BTreeMap<Target, ValueBinding>>,
    // Addresses retained between destination evaluation and its assignment.
    pub(super) assignment_destinations: BTreeMap<bray_bound_tree::BoundExpressionId, ReachedAccess>,
    // A current value can carry report subjects only after one was published.
    pub(super) may_have_report_values: bool,
    pub(super) borrows: BTreeMap<BorrowCapabilityId, RetainedBorrow>,
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
                crate::storage::paths_at_root(absence, Some(slot.root)).any(|(path, reason)| {
                    *reason == StorageAbsence::NoPayload && path.contains_storage(slot)
                })
            })
        };

        let left_absent = |slot: &Target| no_payload(absent, slot);
        let right_absent = |slot: &Target| no_payload(incoming_absent, slot);

        let mut changed = !Arc::ptr_eq(&self.values, &incoming.values)
            && merge_stored(
                Arc::make_mut(&mut self.values),
                &incoming.values,
                left_absent,
                right_absent,
            );

        let destination_count = self.assignment_destinations.len();

        self.assignment_destinations.retain(|assignment, destination| {
            let Some(other) = incoming.assignment_destinations.get(assignment) else { return false; };

            changed |= destination.merge(other);

            true
        });

        changed |= destination_count != self.assignment_destinations.len();

        changed |= incoming.may_have_report_values && !self.may_have_report_values;
        self.may_have_report_values |= incoming.may_have_report_values;

        for (reference, roots) in &incoming.returned_inputs {
            let previous = self.returned_inputs.entry(reference.clone()).or_default();

            changed |= previous.merge(roots);
        }

        changed |= merge_borrows(&mut self.borrows, &incoming.borrows);

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

fn merge_borrows(
    target: &mut BTreeMap<BorrowCapabilityId, RetainedBorrow>,
    incoming: &BTreeMap<BorrowCapabilityId, RetainedBorrow>,
) -> bool {
    let mut changed = false;

    for (loan, reached) in target.iter_mut() {
        changed |= match incoming.get(loan) {
            Some(incoming) => reached.merge(incoming),
            None => reached.forget(),
        };
    }

    for (loan, reached) in incoming {
        if let std::collections::btree_map::Entry::Vacant(slot) = target.entry(*loan) {
            let mut reached = reached.clone();

            reached.forget();
            slot.insert(reached);
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
    /// Possible input or allocation regions, without established address provenance.
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

/// Physical values and their active payloads at one reachable unit boundary.
pub(in crate::analysis::storage_flow) struct ReferenceExit<'a> {
    pub(super) slots: BTreeMap<Place, Cow<'a, ValueBinding>>,
    pub(super) stored: &'a BTreeMap<Target, ValueBinding>,
    pub(super) absent: BTreeSet<Place>,
    pub(super) input_regions: BTreeSet<Place>,
}

impl ReferenceExit<'_> {
    pub(in crate::analysis::storage_flow) fn observed_slots(&self) -> impl Iterator<Item = &Place> {
        self.slots.keys()
    }

    pub(in crate::analysis::storage_flow) fn owned_backings(&self) -> impl Iterator<Item = (&Place, &Target)> {
        self.slots.iter().filter_map(|(slot, value)| {
            let backing = value.owned_backing().filter(|backing| backing.is_exact())?;
            let target = backing.targets.first().filter(|target| !target.previous)?;

            (!self.absent.iter().any(|absent| absent.contains_storage(slot))).then_some((slot, target))
        })
    }

    pub(in crate::analysis::storage_flow) fn absent_payloads(&self) -> &BTreeSet<Place> {
        &self.absent
    }

    pub(in crate::analysis::storage_flow) fn report_payloads(&self, result: bray_bound_tree::StorageIdentityId)
        -> impl Iterator<Item = (&Place, &bray_bound_tree::ReportDependencies, bool)> {
        crate::storage::paths_at_root(&self.slots, Some(result)).filter_map(|(slot, value)| {
            let (report, complete) = value.report_dependencies()?;

            (!self.absent.iter().any(|absent| absent.contains_storage(slot)))
                .then_some((slot, report.as_ref(), complete))
        })
    }

    pub(in crate::analysis::storage_flow) fn whole_sources(&self) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.stored.iter().filter(|(target, _)| !target.previous)
            .filter_map(|(target, value)| value.whole_sources().map(|(sources, complete)| (target, sources, complete)))
    }

    pub(in crate::analysis::storage_flow) fn stored_bindings(&self) -> impl Iterator<Item = (&Target, ReferenceExitValue<'_>)> {
        self.stored.iter().filter(|(target, _)| !target.previous).flat_map(|(slot, value)| {
            value.is_no_payload().then_some((slot, ReferenceExitValue::NoPayload)).into_iter()
                .chain(value.possible_references().map(move |references| {
                    (slot, ReferenceExitValue::Address {
                        targets: &references.targets,
                        nullable: references.has_empty_alternative(),
                        bounds: if references.is_unknown() { ReferenceBounds::Unknown }
                            else if references.is_bounded() { ReferenceBounds::InputRegions }
                            else if references.is_displaced() { ReferenceBounds::Displaced }
                            else { ReferenceBounds::Known },
                    })
                }))
        })
    }

    pub(in crate::analysis::storage_flow) fn empty_slots(&self) -> impl Iterator<Item = &Place> {
        self.slots.iter().filter_map(|(slot, value)| {
            value.possible_references().any(AddressValue::has_empty_alternative).then_some(slot)
        })
    }

    pub(in crate::analysis::storage_flow) fn no_payload_slots(&self) -> impl Iterator<Item = &Place> {
        self.slots.iter().filter_map(|(slot, value)| {
            (value.is_no_payload() || self.absent.iter().any(|absent| absent.contains_storage(slot))).then_some(slot)
        })
    }

    pub(in crate::analysis::storage_flow) fn addresses(
        &self,
    ) -> impl Iterator<Item = (&Place, &AddressValue, ReferenceBounds)> {
        self.slots.iter().flat_map(move |(slot, value)| {
            value.possible_references().map(move |references| {
                let carried_bound = value.address_bounds.as_ref()
                    .is_some_and(|bounds| std::ptr::eq(bounds, references));

                let bounds = if references.is_unknown() && !carried_bound {
                    ReferenceBounds::Unknown
                } else if self.input_regions.contains(slot) || carried_bound {
                    ReferenceBounds::InputRegions
                } else if references.is_displaced() {
                    ReferenceBounds::Displaced
                } else {
                    ReferenceBounds::Known
                };

                (slot, references, bounds)
            })
        })
    }

    pub(in crate::analysis::storage_flow) fn targets(
        &self,
    ) -> impl Iterator<Item = (&Place, &Target, bool, ReferenceBounds)> {
        self.addresses().flat_map(|(slot, references, bounds)| {
            let exact = references.is_exact();

            references.targets.iter().map(move |target| (slot, target, exact, bounds))
        })
    }
}
