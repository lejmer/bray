/// How a resolved transition changes existing value and lifetime observations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageObservationTransition {
    /// A source value is consumed without an independent pointee mutation.
    Transfer,
    /// Contents can change while their backing lifetimes remain alive.
    Contents,
    /// No additional observation survival follows from the change kind.
    #[default]
    Unknown,
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

/// Identity of a checked operation that creates a new contents observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageMutation {
    /// One explicit or scoped call.
    Operation(crate::BoundExecutionSite),
    /// Cancellation broadcast for one owned task-bearing value.
    Cancellation {
        /// Source boundary containing the broadcast.
        occurrence: crate::BoundExecutionSite,
        /// Owner receiving cancellation.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
    },
    /// One opaque represented component within an ordered cleanup.
    Cleanup {
        /// Source boundary containing the cleanup.
        occurrence: crate::BoundExecutionSite,
        /// Owner being cleaned up.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
    },
    /// One lifecycle invocation within an ordered cleanup.
    Lifecycle {
        /// Source boundary containing the invocation.
        occurrence: crate::BoundExecutionSite,
        /// Owner being cleaned up.
        access: crate::StorageAccessId,
        /// Prepared component within that owner.
        part: usize,
        /// Selected lifecycle phase.
        slot: bray_symbols::TypeAssociatedLifecycleSlot,
    },
}

impl StorageMutation {
    /// Returns the enclosing source occurrence.
    pub const fn occurrence(self) -> crate::BoundExecutionSite {
        match self {
            Self::Operation(occurrence)
            | Self::Lifecycle { occurrence, .. }
            | Self::Cleanup { occurrence, .. }
            | Self::Cancellation { occurrence, .. } => occurrence,
        }
    }
}

/// Checked facts at a reachable normal completion.
/// Value observations and required storage lifetimes are distinct relationships.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct StorageCompletion {
    publications: std::collections::BTreeMap<crate::StorageValueDestination, std::sync::Arc<crate::StorageCapturedValue>>,
    deferred: Option<std::sync::Arc<crate::StorageDeferredTransition>>,
    captures: std::sync::Arc<[crate::StorageValueCapture]>,
    values: std::sync::Arc<[crate::StorageValueTransition]>,
    references: std::collections::BTreeMap<crate::StoragePath, crate::StorageTarget>,
    borrow_origins: std::collections::BTreeMap<
        crate::BoundExecutionSite,
        std::collections::BTreeSet<crate::StoragePath>,
    >,
}

impl StorageCompletion {
    /// Whether this completion publishes no values, captures or reference relationships.
    pub fn is_empty(&self) -> bool {
        self.publications.is_empty() && self.deferred.is_none() && self.values.is_empty() && self.captures.is_empty() && self.references.is_empty() && self.borrow_origins.is_empty()
    }

    /// Creates a completion from exact value observations and all possible local borrowed owners.
    pub fn new(
        references: std::collections::BTreeMap<crate::StoragePath, crate::StorageTarget>,
        borrow_origins: std::collections::BTreeMap<
            crate::BoundExecutionSite,
            std::collections::BTreeSet<crate::StoragePath>,
        >,
    ) -> Self {
        Self {
            publications: Default::default(),
            deferred: None,
            captures: std::sync::Arc::from([]),
            values: std::sync::Arc::from([]),
            references,
            borrow_origins,
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

    /// Retains the same prepared latent transition for source checking and execution replay.
    pub fn with_deferred_transition(mut self, transition: std::sync::Arc<crate::StorageDeferredTransition>) -> Self {
        self.deferred = Some(transition);

        self
    }

    /// The latent transition captured at this producer's formation boundary.
    pub fn deferred_transition(&self) -> Option<&std::sync::Arc<crate::StorageDeferredTransition>> { self.deferred.as_ref() }

    /// Publishes actual whole-value transitions resolved before input consumption.
    pub fn with_values(mut self, values: std::sync::Arc<[crate::StorageValueTransition]>) -> Self {
        self.values = values;

        self
    }

    /// Returns actual value publications, independently of may-dependency bounds.
    pub fn values(&self) -> &[crate::StorageValueTransition] { &self.values }

    /// Retains physical input snapshots captured at the operation's admission boundary.
    pub fn with_captures(mut self, captures: std::sync::Arc<[crate::StorageValueCapture]>) -> Self {
        self.captures = captures;

        self
    }

    /// Returns the input snapshots shared by production and execution replay.
    pub fn captures(&self) -> &[crate::StorageValueCapture] { &self.captures }

    /// Returns established referents without discarding entry layers or returned origins.
    pub fn references(
        &self,
    ) -> &std::collections::BTreeMap<crate::StoragePath, crate::StorageTarget> {
        &self.references
    }

    /// Returns the possible invocation-local owners retained through each borrowed input.
    /// An empty set records that checked incoming borrows need no local owner. Their
    /// ordinary dependency contract still retains every caller-owned lifetime.
    pub fn borrow_origins(
        &self,
    ) -> &std::collections::BTreeMap<
        crate::BoundExecutionSite,
        std::collections::BTreeSet<crate::StoragePath>,
    > {
        &self.borrow_origins
    }

    /// Merges alternative normal completions. Exact aliases must agree, and lifetimes accumulate.
    /// A missing origin mapping remains unknown, never an empty lifetime requirement.
    pub fn merge(&mut self, incoming: &Self) {
        crate::StorageDeferredTransition::merge_alternative(&mut self.deferred, &incoming.deferred);

        if self.captures != incoming.captures {
            self.captures = self.captures.iter().filter_map(|capture| {
                let other = incoming.captures.iter().find(|other| other.input == capture.input)?;

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

                Some(crate::StorageValueCapture { input: capture.input, element_bounds,
                    value: std::sync::Arc::new(capture.value.merged(&other.value)), referent })
            }).collect();
        }

        crate::merge_storage_value_transitions(&mut self.values, &incoming.values);

        self.publications.retain(|destination, value| {
            let Some(other) = incoming.publications.get(destination) else { return false; };

            if value != other { *value = std::sync::Arc::new(value.merged(other)); }

            true
        });

        self.references
            .retain(|path, target| incoming.references.get(path) == Some(target));

        self.borrow_origins.retain(|path, targets| {
            let Some(other) = incoming.borrow_origins.get(path) else {
                return false;
            };

            // Each completion retains its own finite set of shared immutable storage paths.
            targets.extend(other.iter().cloned());

            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::StorageCompletion;
    use crate::StorageTarget;
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn completion_merge_intersects_exact_aliases_and_unions_required_origins() {
        let unit = crate::BoundUnitId::new(3);

        let path = |slot| {
            crate::StoragePath::root(crate::StorageIdentityId::from_storage_slot(unit, slot))
        };

        let argument = crate::BoundExpressionId::from_slot(unit, 0);

        let publication = |slot| {
            let mut binding = crate::StorageValueBinding::default();

            binding.address = Some(crate::StorageAddressValue::one(StorageTarget::storage(path(slot))));

            std::sync::Arc::new(crate::StorageCapturedValue::new(StorageTarget::storage(path(0)),
                crate::StorageCapturedTransition {
                    values: [(StorageTarget::storage(path(0)), binding)].into(), ..Default::default()
                }))
        };

        let mut left = StorageCompletion::new(
            BTreeMap::from([(path(0), StorageTarget::storage(path(1)))]),
            BTreeMap::from([(argument.into(), BTreeSet::from([path(1)]))]),
        );

        left.publish(crate::StorageValueDestination::Result, publication(1));

        let mut right = StorageCompletion::new(
            BTreeMap::from([(path(0), StorageTarget::storage(path(2)))]),
            BTreeMap::from([(argument.into(), BTreeSet::from([path(2)]))]),
        );

        right.publish(crate::StorageValueDestination::Result, publication(2));

        left.merge(&right);
        assert!(left.references().is_empty());
        assert_eq!(left.publications().len(), 1);

        let snapshot = &left.publications()[&crate::StorageValueDestination::Result];

        assert!(snapshot.known_address().is_none(), "alternative addresses do not establish one current actual target");
        assert_eq!(snapshot.address_regions(), Some(vec![StorageTarget::storage(path(1)), StorageTarget::storage(path(2))]));

        assert_eq!(
            left.borrow_origins()[&argument.into()],
            BTreeSet::from([path(1), path(2)])
        );

        left.merge(&StorageCompletion::default());
        assert!(!left.borrow_origins().contains_key(&argument.into()));
        assert!(left.publications().is_empty(), "a missing outcome cannot preserve a guaranteed publication");
    }
}
