/// How a resolved transition changes existing value and lifetime observations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageObservationTransition {
    /// No storage or lifetime changes on this outcome.
    Unchanged,
    /// A source value is consumed without an independent pointee mutation.
    Transfer,
    /// Contents can change while their backing lifetimes remain alive.
    Contents,
    /// A typed read or write changes at most one cell's contents.
    CellContents(bray_symbols::TypeId),
    /// No additional observation survival follows from the change kind.
    #[default]
    Unknown,
}

impl StorageObservationTransition {
    /// Whether the transition leaves backing lifetimes alive.
    pub const fn changes_contents_only(self) -> bool {
        matches!(self, Self::Contents | Self::CellContents(_))
    }
}

/// The state observed through one checked value, independently of access authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageObservationKind {
    /// Contents can change through protected or unresolved reachable state.
    Mutable,
    /// The initialized state of an Uninit owner, without its payload or reached authority.
    Initialization,
    /// An ordinary inline value with no mutable reachable contents.
    Value,
    /// Unchanged ordinary inline data of an unresolved shape, excluding protected or reached state.
    InlineBits,
    /// An address value whose reachable contents remain independent observations.
    Address,
    /// An identity whose protected contents can progress independently.
    Identity,
    /// An empty ownership marker of the exact checked type.
    Marker(bray_symbols::TypeId),
    /// A nullable presence bit or active union tag, separately from its payload.
    Discriminant,
    /// A reference boundary with its original checked type and borrow layers.
    Reference(bray_symbols::TypeId),
}

impl StorageObservationKind {
    /// An inline value or descriptor identity, excluding protected and reached contents.
    pub const fn is_ordinary_inline(self) -> bool {
        !matches!(self, Self::Mutable | Self::Initialization | Self::Reference(_))
    }

    /// Records descriptor survival separately from preserved contents or tags.
    pub fn record_preservation(
        self,
        path: crate::StoragePath,
        descriptors: &mut std::collections::BTreeSet<crate::StoragePath>,
        observations: &mut std::collections::BTreeMap<crate::StoragePath, Self>,
    ) {
        if self == Self::Initialization {
            return;
        }

        if self == Self::Identity {
            descriptors.insert(path);
            return;
        }

        let retained = if matches!(self, Self::Discriminant | Self::Address | Self::InlineBits) { self } else { Self::Value };

        observations.entry(path).and_modify(|previous| {
            if *previous == Self::Discriminant || retained == Self::Value { *previous = retained; }
        }).or_insert(retained);
    }

    /// Whether retaining this exact value preserves all of its observable contents.
    pub const fn is_immutable_inline(self) -> bool {
        matches!(self, Self::Value | Self::Marker(_))
    }

    /// Retains an already classified observation through the same exact typed view.
    pub fn record_pointee_preservation<'a>(
        self,
        targets: impl IntoIterator<Item = &'a crate::StorageTarget>,
        ty: bray_symbols::TypeId,
        projections: &[crate::StorageProjection],
        observations: &mut std::collections::BTreeMap<(crate::StorageTarget, bray_symbols::TypeId), std::collections::BTreeSet<(std::sync::Arc<[crate::StorageProjection]>, Self)>>,
    ) {
        if self.is_ordinary_inline() {
            for target in targets {
                if target.is_exact() && target.value_observation_path().is_none() {
                    observations.entry((target.clone(), ty)).or_default().insert((projections.into(), self));
                }
            }
        }
    }
}

/// Reached result of observing a running task, including nested observations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageTaskOutcome {
    /// The body completed and each enclosing observation transferred its result.
    Completed {
        /// The selected portable normal row, when established.
        row: Option<usize>,
    },
    /// An observed task ended abnormally and published the corresponding tag.
    Abnormal {
        /// The number of enclosing observations that transferred a completed value.
        depth: usize,
        /// The selected terminal kind of the observed task.
        kind: bray_symbols::DependencyFailureKind,
        /// The selected portable failure row, when established.
        row: Option<usize>,
    },
    /// The solver retains the possible changes of several results together.
    Widened,
}

impl StorageTaskOutcome {
    /// Returns whether these observations can describe the same task result.
    pub fn overlaps(self, other: Self) -> bool {
        let rows_overlap = |left: Option<usize>, right: Option<usize>| {
            left == right || left.is_none() || right.is_none()
        };

        match (self, other) {
            (Self::Widened, _) | (_, Self::Widened) => true,
            (Self::Completed { row: left }, Self::Completed { row: right }) => rows_overlap(left, right),
            (Self::Abnormal { depth, kind, row }, Self::Abnormal {
                depth: other_depth, kind: other_kind, row: other_row,
            }) => depth == other_depth && kind == other_kind && rows_overlap(row, other_row),
            _ => false,
        }
    }
}

/// One action in an ordered cleanup schedule.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCleanupAction {
    /// Source boundary where the action runs.
    pub occurrence: crate::BoundExecutionSite,
    /// Owner receiving the action.
    pub access: crate::StorageAccessId,
    /// Action selected for this component.
    pub kind: StorageCleanupActionKind,
    /// Exact reached operation that performs the action.
    pub operation: crate::AnalysisOperationId,
    /// The checked component within this owner.
    pub part: usize,
}

/// The operation performed at a cleanup position.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageCleanupActionKind {
    /// Issues cancellation without waiting for the owner.
    Broadcast,
    /// Resolves cleanup of a represented component.
    Opaque,
    /// Invokes a selected source or imported lifecycle body.
    Lifecycle(bray_symbols::TypeAssociatedLifecycleSlot),
    /// Continues a region of possible array elements at its first represented part.
    RepeatArray,
}

/// Identity of a checked operation that creates a new contents observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageMutation {
    /// One explicit or scoped call.
    Operation(crate::BoundExecutionSite),
    /// Installs a selected compound operator's completed replacement.
    AssignmentInstall(crate::BoundExpressionId),
    /// Transfers the payload of one reached scoped protocol result.
    ScopedResult {
        /// The completed entry or exit invocation.
        occurrence: crate::BoundExecutionSite,
        /// The case whose payload was transferred.
        variant: bray_symbols::UnionVariantSymbolId,
    },
    /// A reached scope cleanup whose lifecycle dependencies could not be ordered.
    ScopeCleanupOrder {
        /// Exact cleanup operation in the prepared body.
        operation: crate::AnalysisOperationId,
        /// Lexical scope and source exit owning the cleanup.
        point: crate::StorageExitPoint,
    },
    /// Performed changes of a call that did not publish a normal result.
    InvocationFailure {
        /// The admitted call or suspension.
        occurrence: crate::BoundExecutionSite,
        /// The selected abnormal outcome.
        kind: bray_symbols::DependencyFailureKind,
        /// The exact reached edge owning this failed invocation.
        edge: crate::AnalysisEdgeId,
        /// The selected contract row, when the invocation describes its failures.
        row: Option<usize>,
    },
    /// Effects known when a running task's observation publishes a result.
    TaskObservation {
        /// The await that acquires the task observation result.
        occurrence: crate::BoundExecutionSite,
        /// The exact CFG completion applying the observation.
        operation: crate::AnalysisOperationId,
        /// The acquired outcome or the conservative combination of outcomes.
        outcome: StorageTaskOutcome,
    },
    /// The normal outcome of one selected cleanup action.
    Cleanup(StorageCleanupAction),
    /// One guarded normal outcome of a selected lifecycle action.
    CleanupOutcome {
        /// The action whose receiver admitted this row.
        action: StorageCleanupAction,
        /// The selected contract row.
        row: usize,
    },
    /// The performed prefix of a cleanup action that left abnormally.
    CleanupFailure {
        /// Action that failed after admission.
        action: StorageCleanupAction,
        /// The abnormal outcome of the action.
        kind: bray_symbols::DependencyFailureKind,
        /// Its selected outcome row, if the contract describes the failure.
        row: Option<usize>,
    },
}

impl StorageMutation {
    /// Returns the enclosing source occurrence.
    pub const fn occurrence(self) -> crate::BoundExecutionSite {
        match self {
            Self::Operation(occurrence)
            | Self::ScopedResult { occurrence, .. }
            | Self::InvocationFailure { occurrence, .. }
            | Self::TaskObservation { occurrence, .. } => occurrence,
            Self::AssignmentInstall(expression) => crate::BoundExecutionSite::Node(crate::AnyBoundNodeId::Expression(expression)),
            Self::Cleanup(action) | Self::CleanupOutcome { action, .. }
            | Self::CleanupFailure { action, .. } => action.occurrence,
            Self::ScopeCleanupOrder { point, .. } => crate::BoundExecutionSite::Cleanup(point.exit()),
        }
    }

    /// The invocation that admitted the inputs still owned by this transition.
    pub const fn invocation(self) -> Option<crate::BoundExecutionSite> {
        match self {
            Self::Operation(occurrence) | Self::InvocationFailure { occurrence, .. }
            | Self::TaskObservation { occurrence, .. } => Some(occurrence),
            _ => None,
        }
    }
}

/// Writes bound to actual destinations of one admitted failure outcome.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageFailureWrites {
    /// The intrinsic's admitted output also describes its committed cleanup writes.
    AdmittedOutputs,
    /// The failure's checked writes in the admitted receiver-first environment.
    Bound {
        /// The selected deferred body. An ordinary call uses its own entry.
        producer: Option<crate::StorageDeferredProducer>,
        /// The argument count used to interpret checked terms.
        input_count: usize,
        /// The reached portable rows with their resolved destinations.
        values: std::sync::Arc<[crate::StorageValueTransition]>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CompletionEntry {
    Cleanup(std::sync::Arc<crate::StorageCapturedValue>, std::sync::Arc<crate::StorageValueTransition>),
    Failure(std::sync::Arc<[StorageFailureWrites]>),
}

/// A reached physical value and the checked view through which it was observed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageObservedTarget {
    /// The physical value's established origin.
    pub target: crate::StorageTarget,
    /// The reached type when this observation reads through a stored borrow.
    pub pointee: Option<bray_symbols::TypeId>,
}

impl StorageObservedTarget {
    /// An observation of the stored value itself.
    pub const fn direct(target: crate::StorageTarget) -> Self {
        Self { target, pointee: None }
    }

    /// An observation through an already evaluated borrow.
    pub const fn borrowed(target: crate::StorageTarget, pointee: bray_symbols::TypeId) -> Self {
        Self { target, pointee: Some(pointee) }
    }
}

/// Checked facts after an operation's reached effects.
/// Value observations and required storage lifetimes are distinct relationships.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct StorageCompletion {
    entry: Option<std::sync::Arc<CompletionEntry>>,
    publications: std::collections::BTreeMap<crate::StorageValueDestination, std::sync::Arc<crate::StorageCapturedValue>>,
    references: std::collections::BTreeMap<crate::StoragePath, StorageObservedTarget>,
}

impl StorageCompletion {
    /// Creates a completion from reached referent observations.
    pub fn new(
        references: std::collections::BTreeMap<crate::StoragePath, StorageObservedTarget>,
    ) -> Self {
        Self {
            entry: None,
            publications: Default::default(),
            references,
        }
    }

    /// Retains the reached lifecycle receiver and the checked writes of its selected outcome.
    pub fn record_lifecycle_receiver(&mut self, input: std::sync::Arc<crate::StorageCapturedValue>, transition: std::sync::Arc<crate::StorageValueTransition>) {
        self.entry = Some(std::sync::Arc::new(CompletionEntry::Cleanup(input, transition)));
    }

    /// The reached receiver and exact destination bindings shared with proof.
    pub fn lifecycle_receiver(&self) -> Option<(&crate::StorageCapturedValue, &crate::StorageValueTransition)> {
        match self.entry.as_deref()? {
            CompletionEntry::Cleanup(input, transition) => Some((input, transition)),
            CompletionEntry::Failure(_) => None,
        }
    }

    /// Retains the failure write bindings used to publish these snapshots.
    pub fn record_failure_writes(&mut self, writes: StorageFailureWrites) {
        self.entry = Some(std::sync::Arc::new(CompletionEntry::Failure([writes].into())));
    }

    /// The already resolved writes of the reached failure alternatives.
    pub fn failure_writes(&self) -> &[StorageFailureWrites] {
        match self.entry.as_deref() {
            Some(CompletionEntry::Failure(writes)) => writes,
            _ => &[],
        }
    }

    /// Records the physical snapshot that survived cleanup at this normal destination.
    pub fn publish(&mut self, destination: crate::StorageValueDestination, value: std::sync::Arc<crate::StorageCapturedValue>) {
        self.publications.insert(destination, value);
    }

    /// Final physical publications paired with their current logical values.
    pub fn publications(&self) -> &std::collections::BTreeMap<crate::StorageValueDestination, std::sync::Arc<crate::StorageCapturedValue>> {
        &self.publications
    }

    /// Returns established referents without discarding entry layers or returned origins.
    pub fn references(
        &self,
    ) -> &std::collections::BTreeMap<crate::StoragePath, StorageObservedTarget> {
        &self.references
    }

    /// Merges alternative normal completions, retaining only the exact aliases they agree on.
    pub fn merge(&mut self, incoming: &Self) {
        if self.entry != incoming.entry {
            self.entry = match (self.entry.as_deref(), incoming.entry.as_deref()) {
                (Some(CompletionEntry::Cleanup(left, recipe)), Some(CompletionEntry::Cleanup(right, other))) if recipe == other => {
                    Some(std::sync::Arc::new(CompletionEntry::Cleanup(
                        std::sync::Arc::new(left.merged(right)), std::sync::Arc::clone(recipe),
                    )))
                }
                (Some(CompletionEntry::Failure(left)), Some(CompletionEntry::Failure(right))) => {
                    let values = bray_base::sorted_unique_shared_slice(left.iter().chain(right.iter()).cloned());

                    Some(std::sync::Arc::new(CompletionEntry::Failure(values)))
                }
                _ => None,
            };
        }

        self.publications.retain(|destination, value| {
            let Some(other) = incoming.publications.get(destination) else { return false; };

            if value != other { *value = std::sync::Arc::new(value.merged(other)); }

            true
        });

        self.references
            .retain(|path, target| incoming.references.get(path) == Some(target));
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageCompletion, StorageObservedTarget};
    use crate::StorageTarget;
    use std::collections::BTreeMap;
    #[test]
    fn completion_merge_joins_publications_and_intersects_exact_aliases() {
        let unit = crate::BoundUnitId::new(3);
        let values = bray_symbols::SemanticValueStore::try_new().expect("fixture value store");
        let pointee = values.intern_type(bray_symbols::TypeData::tuple([])).expect("fixture pointee type");

        let path = |slot| {
            crate::StoragePath::root(crate::StorageIdentityId::from_storage_slot(unit, slot))
        };

        let publication = |slot| {
            let mut binding = crate::StorageValueBinding::default();

            binding.address = Some(crate::StorageAddressValue::one(StorageTarget::storage(path(slot))));

            std::sync::Arc::new(crate::StorageCapturedValue::new(StorageTarget::storage(path(0)),
                crate::StorageCapturedTransition {
                    values: [(StorageTarget::storage(path(0)), binding)].into(), ..Default::default()
                }))
        };

        let observation = |slot| StorageObservedTarget::borrowed(StorageTarget::storage(path(slot)), pointee);
        let mut left = StorageCompletion::new(BTreeMap::from([(path(0), observation(1))]));

        left.publish(crate::StorageValueDestination::Result, publication(1));

        let mut right = StorageCompletion::new(BTreeMap::from([(path(0), observation(2))]));

        right.publish(crate::StorageValueDestination::Result, publication(2));

        left.merge(&right);
        assert!(left.references().is_empty());
        assert_eq!(left.publications().len(), 1);

        let snapshot = &left.publications()[&crate::StorageValueDestination::Result];

        assert!(snapshot.known_address().is_none(), "alternative addresses do not establish one current actual target");
        assert_eq!(snapshot.address_regions(), Some(vec![StorageTarget::storage(path(1)), StorageTarget::storage(path(2))]));

        left.merge(&StorageCompletion::default());
        assert!(left.publications().is_empty(), "a missing outcome cannot preserve a guaranteed publication");
    }
}
