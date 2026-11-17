use std::sync::Arc;

use super::{DependencyProjection, DependencySubject};

/// Possible transitions and the subset that can precede normal completion.
/// Abnormal execution can retain any performed prefix of the possible transitions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyEffectOutcomes<Effect> {
    possible: Arc<[Effect]>,
    normal: Arc<[Effect]>,
}

impl<Effect: Ord + Clone> DependencyEffectOutcomes<Effect> {
    /// Creates a conservative contract in which every transition can occur on either outcome.
    pub fn new(effects: impl IntoIterator<Item = Effect>) -> Self {
        let possible = bray_base::sorted_unique_shared_slice(effects);

        Self { normal: Arc::clone(&possible), possible }
    }

    /// Records normal transitions, retaining them among all possible transitions.
    pub fn with_normal(mut self, effects: impl IntoIterator<Item = Effect>) -> Self {
        self.normal = bray_base::sorted_unique_shared_slice(effects);

        if self.normal == self.possible {
            self.normal = Arc::clone(&self.possible);
        } else if self.normal.iter().any(|effect| self.possible.binary_search(effect).is_err()) {
            // The outcomes own descriptors and share their immutable identities and paths.
            self.possible = bray_base::sorted_unique_shared_slice(
                self.possible.iter().chain(self.normal.iter()).cloned(),
            );
        }

        self
    }

    /// Joins possible execution paths without changing their outcome guards.
    pub fn merge(&mut self, incoming: &Self) -> bool {
        if self == incoming { return false; }

        let merged = Self::new(self.possible.iter().chain(incoming.possible.iter()).cloned())
            .with_normal(self.normal.iter().chain(incoming.normal.iter()).cloned());

        let changed = *self != merged;

        *self = merged;

        changed
    }

    /// Translates identities while preserving the outcome of every transition.
    pub fn try_map<Mapped: Ord + Clone, Error>(
        &self,
        mut map: impl FnMut(&Effect) -> Result<Mapped, Error>,
    ) -> Result<DependencyEffectOutcomes<Mapped>, Error> {
        self.try_flat_map(|effect| map(effect).map(std::iter::once))
    }

    /// Binds each transition once, retaining its outcome guards on every resolved alternative.
    pub fn try_flat_map<Mapped: Ord + Clone, Produced: IntoIterator<Item = Mapped>, Error>(
        &self,
        mut map: impl FnMut(&Effect) -> Result<Produced, Error>,
    ) -> Result<DependencyEffectOutcomes<Mapped>, Error> {
        let mut possible = Vec::with_capacity(self.possible.len());
        let mut normal = Vec::with_capacity(self.normal.len());

        for effect in &*self.possible {
            let on_normal_path = self.normal.binary_search(effect).is_ok();

            for mapped in map(effect)? {
                if on_normal_path {
                    // Translate each identity once, then share it between the outcomes that use it.
                    normal.push(mapped.clone());
                }

                possible.push(mapped);
            }
        }

        Ok(DependencyEffectOutcomes::new(possible).with_normal(normal))
    }
}

impl<Effect> DependencyEffectOutcomes<Effect> {
    /// Transitions that may occur before any completion, including a failing prefix.
    pub fn possible(&self) -> &[Effect] { &self.possible }

    /// Transitions that may occur on a path reaching normal completion.
    pub fn normal(&self) -> &[Effect] { &self.normal }
}

impl<Subject, Projection, Type> DependencyEffectOutcomes<DependencyStorageEffect<Subject, Projection, Type>> {
    /// Whether normal execution changes no storage beyond admitted input ownership.
    pub fn preserves_storage_on_normal_return(&self) -> bool {
        self.normal.iter().all(|effect| matches!(effect, DependencyStorageEffect::Direct {
            kind: DependencyMutationKind::Consume, ..
        }))
    }
}

/// The storage transition a callable may perform at an invocation or deferred phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyMutationKind<Type = super::TypeId> {
    /// Mutate protected atomic payload without replacing any containing value.
    Atomic,
    /// Publish an address or references reachable through the named storage.
    Escape,
    /// Replace contents while retaining the storage's initialized lifetime.
    Write,
    /// Install a value of the stated type, including its owned graceful-cleanup obligations.
    Replace(Type),
    /// Install an absent payload while retaining initialized containing storage.
    Clear,
    /// Dispose a value at this location, whose layout is the stated type.
    Dispose(Type),
    /// Conditional effects of disposing this type on a region with no corresponding layout claim.
    Cleanup(Type),
    /// Remove initialized contents without retiring the storage.
    Consume,
    /// End the storage lifetime and revoke dependencies on that lifetime.
    Retire,
}

impl<Type> DependencyMutationKind<Type> {
    /// Removes the layout claim of a mutation known only through its footprint.
    pub fn region_kind(self) -> Self {
        match self {
            Self::Dispose(ty) => Self::Cleanup(ty),
            other => other,
        }
    }
}

/// A possible mutation expressed over entry values, separately from their reference targets.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyStorageEffect<
    Subject = DependencySubject,
    Projection = DependencyProjection,
    Type = super::TypeId,
> {
    /// Protected atomic payloads may change through unresolved aliases.
    Atomic,
    /// Mutate externally reachable storage without exposing invocation-local inputs.
    Ambient,
    /// No finite target set has been established.
    Unknown,
    /// Addresses reachable through any invocation input may be retained externally.
    Escape,
    /// A known transition whose affected region has no finite entry-relative description.
    UnboundedMutation {
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
        /// Original checked access type, independent of the unknown region.
        access_type: Option<Type>,
        /// Only externally reachable storage can be affected.
        ambient: bool,
    },
    /// Mutate the value slot named by the formal subject.
    Direct {
        /// Whether the complete route supplies the evaluated operand as well as its footprint.
        addressed: bool,
        /// Entry value slot.
        subject: Subject,
        /// Original checked access type, independent of the region that bounds aliases.
        /// Forwarding a raw access through a borrowed owner preserves this type.
        access_type: Option<Type>,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
    /// Mutate storage reached by the reference value held in an entry slot.
    Indirect {
        /// Whether the complete route supplies the evaluated operand as well as its footprint.
        addressed: bool,
        /// Slot containing the entry reference. Reassigning this slot does not change that identity.
        reference: Subject,
        /// Projection positions at which execution reads another stored descriptor.
        /// Equal positions denote successive pointers without an intervening component.
        reads: Arc<[u32]>,
        /// Checked components selected between reads and after the final read.
        projections: Arc<[Projection]>,
        /// Original checked access type, independent of the region that bounds aliases.
        /// Forwarding a raw access through a borrowed owner preserves this type.
        access_type: Option<Type>,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
}

impl<Subject, Projection, Type> DependencyStorageEffect<Subject, Projection, Type> {
    /// Keeps this route as a footprint after losing the actual operand.
    pub fn into_region(self) -> Self {
        match self {
            Self::Direct { subject, kind, access_type, .. } => Self::Direct {
                subject, addressed: false, kind: kind.region_kind(), access_type,
            },
            Self::Indirect { reference, reads, projections, kind, access_type, .. } => Self::Indirect {
                reference, reads, projections, addressed: false, kind: kind.region_kind(), access_type,
            },
            effect => effect,
        }
    }

    /// Whether the route can be executed in order and its precision admits the requested layout.
    pub fn is_valid_route(&self) -> bool {
        match self {
            Self::Direct { addressed: false, kind: DependencyMutationKind::Dispose(_), .. }
            | Self::Indirect { addressed: false, kind: DependencyMutationKind::Dispose(_), .. } => false,
            Self::Indirect { reads, projections, .. } => reads.is_sorted()
                && reads.last().is_none_or(|index| (*index as usize) <= projections.len()),
            _ => true,
        }
    }

    /// Entry slot whose value or reference bounds this effect.
    pub fn subject(&self) -> Option<&Subject> {
        match self {
            Self::Direct { subject, .. } => Some(subject),
            Self::Indirect { reference, .. } => Some(reference),
            _ => None,
        }
    }

    /// Types required to retain and interpret this effect across semantic boundaries.
    pub fn referenced_types(&self) -> impl Iterator<Item = &Type> {
        match self {
            Self::Direct { access_type, kind, .. }
            | Self::Indirect { access_type, kind, .. }
            | Self::UnboundedMutation { access_type, kind, .. } => [
                access_type.as_ref(),
                match kind {
                    DependencyMutationKind::Replace(ty) | DependencyMutationKind::Dispose(ty) | DependencyMutationKind::Cleanup(ty) => Some(ty),
                    _ => None,
                },
            ],
            Self::Atomic | Self::Ambient | Self::Unknown | Self::Escape => [None, None],
        }.into_iter().flatten()
    }

    /// Conservative effects when the callable can have an unknown captured environment.
    pub const fn opaque_call() -> [Self; 2] {
        [Self::Unknown, Self::Escape]
    }

    /// Conservative effects when all inputs are explicit and no captured environment exists.
    pub const fn capture_free_call() -> [Self; 2] {
        [Self::Ambient, Self::Escape]
    }

    /// Describes a transition whose particular storage target is unresolved.
    /// Atomic updates and address escape retain their distinct meanings.
    pub fn unresolved(kind: DependencyMutationKind<Type>, access_type: Option<Type>) -> Self {
        match kind {
            DependencyMutationKind::Atomic => Self::Atomic,
            DependencyMutationKind::Escape => Self::Escape,
            DependencyMutationKind::Write
            | DependencyMutationKind::Replace(_)
            | DependencyMutationKind::Dispose(_)
            | DependencyMutationKind::Cleanup(_)
            | DependencyMutationKind::Clear
            | DependencyMutationKind::Consume
            | DependencyMutationKind::Retire => Self::UnboundedMutation { kind, access_type, ambient: false },
        }
    }
}

impl<Type> DependencyMutationKind<Type> {
    /// Whether contents change without ending the containing storage lifetime.
    pub const fn is_write(&self) -> bool {
        matches!(self, Self::Write | Self::Replace(_) | Self::Clear)
    }

    /// Translates the payload type while preserving the storage transition.
    pub fn try_map_type<Other, Error>(
        self,
        map: impl FnOnce(Type) -> Result<Other, Error>,
    ) -> Result<DependencyMutationKind<Other>, Error> {
        Ok(match self {
            Self::Atomic => DependencyMutationKind::Atomic,
            Self::Escape => DependencyMutationKind::Escape,
            Self::Write => DependencyMutationKind::Write,
            Self::Clear => DependencyMutationKind::Clear,
            Self::Replace(ty) => DependencyMutationKind::Replace(map(ty)?),
            Self::Dispose(ty) => DependencyMutationKind::Dispose(map(ty)?),
            Self::Cleanup(ty) => DependencyMutationKind::Cleanup(map(ty)?),
            Self::Consume => DependencyMutationKind::Consume,
            Self::Retire => DependencyMutationKind::Retire,
        })
    }
}

impl<Subject: Ord, Projection: Ord, Type: Ord> DependencyStorageEffect<Subject, Projection, Type> {
    /// Normalizes possible effects while retaining known transitions beside opaque effects.
    pub fn normalize(effects: impl IntoIterator<Item = Self>) -> Arc<[Self]> {
        let mut effects = effects.into_iter().collect::<Vec<_>>();

        if effects.iter().any(|effect| matches!(effect, Self::Atomic)) {
            effects.retain(|effect| {
                !matches!(
                    effect,
                    Self::Direct {
                        kind: DependencyMutationKind::Atomic,
                        ..
                    } | Self::Indirect {
                        kind: DependencyMutationKind::Atomic,
                        ..
                    }
                )
            });
        }

        bray_base::sorted_unique_shared_slice(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyMutationKind, DependencyStorageEffect, DependencySubject};

    #[test]
    fn mapping_and_joining_transitions_preserves_normal_and_abnormal_outcomes() {
        let left = super::DependencyEffectOutcomes::new([1, 2]).with_normal([1]);
        let right = super::DependencyEffectOutcomes::new([2, 3]).with_normal([3]);

        let mut next = 10;

        let mut joined = left.try_map(|_| {
            next += 1;

            Ok::<_, std::convert::Infallible>(next)
        })
            .expect("fixture translation is total");

        let right = right.try_map(|effect| Ok::<_, std::convert::Infallible>(effect + 10))
            .expect("fixture translation is total");

        assert!(joined.merge(&right));
        assert_eq!(joined, super::DependencyEffectOutcomes::new([11, 12, 13]).with_normal([11, 13]));
        assert!(!joined.merge(&right), "revisiting the same program point must stabilize");
    }
    #[test]
    fn unresolved_atomic_effect_absorbs_only_narrower_atomic_effects() {
        let subject = DependencySubject::root(super::super::DependencySubjectRoot::Receiver);

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct { addressed: true,
            subject: subject.clone(),
            kind: DependencyMutationKind::Write,
            access_type: None,
        };

        let atomic = DependencyStorageEffect::Direct { addressed: true,
            subject,
            kind: DependencyMutationKind::Atomic,
            access_type: None,
        };

        assert_eq!(
            DependencyStorageEffect::normalize([
                atomic,
                write.clone(),
                DependencyStorageEffect::Atomic,
            ])
            .as_ref(),
            &[DependencyStorageEffect::Atomic, write]
        );
    }

    #[test]
    fn absent_summary_is_not_a_proof_of_no_mutation() {
        let missing = super::super::DependencyContractTemplateData::new([]);
        let unchanged = missing.clone().with_effects([]);

        assert_eq!(
            missing.effects(),
            &super::DependencyStorageEffect::opaque_call()
        );

        assert!(unchanged.effects().is_empty());

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct { addressed: true,
            subject: DependencySubject::root(super::super::DependencySubjectRoot::Receiver),
            kind: DependencyMutationKind::Write,
            access_type: None,
        };

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), write.clone()]).as_ref(),
            std::slice::from_ref(&write)
        );

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), DependencyStorageEffect::Unknown])
                .as_ref(),
            &[DependencyStorageEffect::Unknown, write]
        );
    }
    #[test]
    fn unresolved_targets_preserve_known_transitions() {
        type Effect = DependencyStorageEffect<(), (), ()>;

        for kind in [
            DependencyMutationKind::Atomic,
            DependencyMutationKind::Escape,
            DependencyMutationKind::Write,
            DependencyMutationKind::Replace(()),
            DependencyMutationKind::Dispose(()),
            DependencyMutationKind::Cleanup(()),
            DependencyMutationKind::Clear,
            DependencyMutationKind::Consume,
            DependencyMutationKind::Retire,
        ] {
            let expected = match kind {
                DependencyMutationKind::Atomic => Effect::Atomic,
                DependencyMutationKind::Escape => Effect::Escape,
                _ => Effect::UnboundedMutation { kind, access_type: Some(()), ambient: false },
            };

            assert_eq!(Effect::unresolved(kind, Some(())), expected);
        }
    }
}
