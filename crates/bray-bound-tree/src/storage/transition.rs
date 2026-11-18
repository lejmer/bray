use bray_symbols::{SymbolOrdinal, TypeId};

use super::{StorageProjection, StorageTarget};

/// The existing operation boundary at which captured physical information changes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValuePhase {
    /// Evaluated arguments enter their captured environment.
    Input,
    /// Admitted execution applies its effects.
    Attempt,
    /// A private result is initialized before required cleanup.
    Production,
    /// Normal publication and consumption of remaining input handles.
    Completion,
}

/// Ordering of physical retirements within one existing operation phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageValueBoundary {
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
    },
}

/// An evaluated value retained by a normal transition, independently of its old storage.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageSnapshotRoot {
    /// A captured input snapshot selected before execution changes current memory.
    Input(bray_symbols::DependencySubjectRoot),
    /// The same prebound value that private production will install.
    Produced(StorageValueDestination),
}

/// One guarded set of whole-value publications over evaluated inputs and actual destinations.
pub type StorageValueTransition = bray_symbols::CapturedValueOutcome<StorageValueDestination, StorageValueInput,
    StorageProjection, bray_symbols::StaticReferenceSelection, bray_symbols::ConstantTermId>;

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

        let incoming = Arc::from([row(1)]);
        let mut observed = Arc::from([row(0)]);

        merge_storage_value_transitions(&mut observed, &incoming);
        assert!(observed.contains(&row(0)) && observed.contains(&row(1)));

        merge_storage_value_transitions(&mut observed, &Arc::default());
        assert!(observed.contains(&StorageValueTransition::new(None, [])));

        let snapshot = Arc::clone(&observed);

        merge_storage_value_transitions(&mut observed, &incoming);
        assert_eq!(observed, snapshot);
    }
}
