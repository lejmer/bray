use bray_symbols::{SymbolOrdinal, TypeId};

use super::{StorageProjection, StorageTarget};

/// Ordering of physical retirements within one reached operation or outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValueBoundary {
    /// Consumed input slots retire once before either outcome executes.
    Admission,
    /// Effects already performed before logical completion and publication.
    Execution,
    /// Last-use and consumed-handle retirement after that phase completes.
    Retirement,
}

/// Physical information captured with one receiver-first evaluated input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageValueCapture {
    /// The actual evaluated input, independently of its source slot's later contents.
    pub input: SymbolOrdinal,
    /// Captured inline descriptors, initialized shape and deferred environment.
    pub value: std::sync::Arc<crate::StorageCapturedValue>,
    /// Current typed referent captured at execution entry, when its address is established.
    pub referent: Option<(TypeId, std::sync::Arc<crate::StorageCapturedValue>)>,
    /// Possible element dependencies behind a buffer descriptor, without identifying a particular element.
    pub element_bounds: Option<(StorageProjection, std::sync::Arc<crate::StorageCapturedValue>)>,
}

/// Retains checked input captures common to the reached alternatives.
pub fn merge_storage_value_captures(
    observed: &mut std::sync::Arc<[StorageValueCapture]>,
    incoming: &[StorageValueCapture],
) {
    if observed.as_ref() == incoming {
        return;
    }

    *observed = observed.iter().filter_map(|capture| {
        let other = incoming.iter().find(|other| other.input == capture.input)?;

        let referent = match (&capture.referent, &other.referent) {
            (Some((ty, left)), Some((other_ty, right))) if ty == other_ty => Some((*ty, std::sync::Arc::new(left.merged(right)))),
            _ => None,
        };

        let element_bounds = match (&capture.element_bounds, &other.element_bounds) {
            (Some((pointer, left)), Some((other_pointer, right))) if pointer == other_pointer =>
                Some((*pointer, std::sync::Arc::new(left.merged(right)))),
            (Some((pointer, bounds)), None) | (None, Some((pointer, bounds))) =>
                Some((*pointer, std::sync::Arc::new(bounds.with_unknown_alternative()))),
            _ => None,
        };

        Some(StorageValueCapture {
            input: capture.input,
            element_bounds,
            value: std::sync::Arc::new(capture.value.merged(&other.value)),
            referent,
        })
    }).collect();
}

/// A source in the operation's captured entry environment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValueInput {
    /// One receiver-first operand of a checked intrinsic.
    Argument(SymbolOrdinal),
    /// A checked formal value term in the selected callable's receiver-first environment.
    Term(bray_symbols::ConstantTermId),
}

/// One publication destination resolved against the operation's incoming state.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValueDestination {
    /// The operation's private result, published after its required cleanup.
    Result,
    /// Possible dependencies written somewhere within this region. No current value is established.
    Region {
        /// Region possibly written through the evaluated descriptor.
        target: StorageTarget,
        /// The checked destination that supplied the descriptor. Separate writes
        /// can reach the same region and must retain both sets of dependencies.
        via: bray_symbols::DependencySubject,
    },
    /// Actual typed storage, independently of its conservative effect footprint.
    Current {
        /// The captured storage address. A region bound cannot supply this field.
        target: StorageTarget,
        /// Type established by the checked storage operation.
        ty: TypeId,
        /// The selected physical transition already installed this value.
        installed: bool,
    },
}

/// An evaluated value retained by a normal transition, independently of its old storage.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageSnapshotRoot {
    /// A captured input snapshot selected before execution changes current memory.
    Argument(SymbolOrdinal),
    /// A checked extraction from an admitted reference, before normal effects.
    Extracted {
        /// The body that owns the admitted reference for a deferred activation.
        producer: Option<crate::StorageDeferredProducer>,
        /// The checked input of the ownership transfer.
        term: bray_symbols::ConstantTermId,
    },
    /// The private result of a checked memory read.
    PendingResult,
    /// The same prebound value that private production will install.
    Produced(StorageValueDestination),
}

type StorageValueOutcome = bray_symbols::CapturedValueOutcome<StorageValueDestination, StorageValueInput,
    StorageProjection, bray_symbols::StaticReferenceSelection, bray_symbols::ConstantTermId>;

/// One guarded set of reached publications and its source row in the callable contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageValueTransition {
    /// The contract row whose effects and dependencies accompany this publication.
    pub source_row: Option<usize>,
    /// The guarded publications bound to this operation's actual destinations.
    pub outcome: StorageValueOutcome,
    write_inputs: std::sync::Arc<[(SymbolOrdinal, std::sync::Arc<[StorageProjection]>, StorageTarget)]>,
}

impl StorageValueTransition {
    /// Records reached publications with no portable contract row, such as a memory intrinsic.
    pub fn new(
        guard: Option<bray_symbols::ConstantTermId>,
        bindings: impl IntoIterator<Item = (StorageValueDestination, bray_symbols::CapturedValueTemplate<StorageValueInput, StorageProjection>)>,
    ) -> Self {
        Self {
            source_row: None,
            outcome: StorageValueOutcome::new(guard, bindings),
            write_inputs: Default::default(),
        }
    }

    /// Retains the actual admitted descriptors that establish exact writes in this row.
    pub fn with_write_inputs(mut self, inputs: impl IntoIterator<Item = (SymbolOrdinal, std::sync::Arc<[StorageProjection]>, StorageTarget)>) -> Self {
        self.write_inputs = bray_base::sorted_unique_shared_slice(inputs);

        self
    }

    /// The input ordinals, stored descriptor selectors, and evaluated addresses behind exact writes.
    pub fn write_inputs(&self) -> &[(SymbolOrdinal, std::sync::Arc<[StorageProjection]>, StorageTarget)] {
        &self.write_inputs
    }
}

/// Joins finite checked alternatives observed at the same operation during CFG replay.
/// A path with no known publications remains feasible alongside the other paths.
pub fn merge_storage_value_transitions(
    observed: &mut std::sync::Arc<[StorageValueTransition]>,
    incoming: &std::sync::Arc<[StorageValueTransition]>,
) {
    if observed == incoming { return; }

    let opaque = (observed.is_empty() || incoming.is_empty()).then(|| StorageValueTransition::new(None, []));

    *observed = bray_base::sorted_unique_shared_slice(observed.iter().chain(incoming.iter()).cloned().chain(opaque));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{CapturedValueTemplate, DependencySubject, DependencySubjectRoot, SymbolOrdinal};
    use crate::{BoundUnitId, StorageIdentityId, StoragePath, StorageTarget};

    use super::{StorageValueDestination, StorageValueInput, StorageValueTransition, merge_storage_value_transitions};

    #[test]
    fn replay_keeps_possible_writes_without_establishing_a_current_write() {
        let target = StorageTarget::storage(StoragePath::root(StorageIdentityId::from_slot(BoundUnitId::new(1), 0)));

        let destination = StorageValueDestination::Region {
            target,
            via: DependencySubject::root(DependencySubjectRoot::Receiver),
        };

        let row = |ordinal| StorageValueTransition::new(None, [(
            destination.clone(),
            CapturedValueTemplate::Input(StorageValueInput::Argument(SymbolOrdinal::new(ordinal))),
        )]);

        let mut selected = row(0);

        selected.source_row = Some(2);

        let incoming = Arc::from([row(1), selected.clone()]);
        let mut observed = Arc::from([row(0)]);

        merge_storage_value_transitions(&mut observed, &incoming);
        assert!(observed.contains(&row(0)) && observed.contains(&row(1)));
        assert!(observed.contains(&selected), "identical writes can accompany different contract effects");

        merge_storage_value_transitions(&mut observed, &Arc::default());
        assert!(observed.contains(&StorageValueTransition::new(None, [])));

        let snapshot = Arc::clone(&observed);

        merge_storage_value_transitions(&mut observed, &incoming);
        assert_eq!(observed, snapshot);
    }
}
