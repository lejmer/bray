use std::sync::Arc;

use super::{DependencyProjection, DependencySubject};

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
    /// Remove initialized contents without retiring the storage.
    Consume,
    /// End the storage lifetime and revoke dependencies on that lifetime.
    Retire,
}

/// A possible mutation expressed over entry values, separately from their reference targets.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyStorageEffect<
    Subject = DependencySubject,
    Projection = DependencyProjection,
    Type = super::TypeId,
> {
    /// Opaque effects of disposing this type. No effects occur when the type needs no cleanup.
    Cleanup(Type),
    /// Protected atomic payloads may change through unresolved aliases.
    Atomic,
    /// Mutate externally reachable storage without exposing invocation-local inputs.
    Ambient,
    /// No finite target set has been established.
    Unknown,
    /// Addresses reachable through any invocation input may be retained externally.
    Escape,
    /// Mutate the value slot named by the formal subject.
    Direct {
        /// Entry value slot.
        subject: Subject,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
    /// Mutate storage reached by the reference value held in an entry slot.
    Indirect {
        /// Slot containing the entry reference. Reassigning this slot does not change that identity.
        reference: Subject,
        /// Checked projections after dereferencing the entry reference.
        projections: Arc<[Projection]>,
        /// Possible transition.
        kind: DependencyMutationKind<Type>,
    },
}

impl<Subject, Projection, Type> DependencyStorageEffect<Subject, Projection, Type> {
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
    pub fn unresolved(kind: DependencyMutationKind<Type>) -> Self {
        match kind {
            DependencyMutationKind::Atomic => Self::Atomic,
            DependencyMutationKind::Escape => Self::Escape,
            DependencyMutationKind::Write
            | DependencyMutationKind::Replace(_)
            | DependencyMutationKind::Clear
            | DependencyMutationKind::Consume
            | DependencyMutationKind::Retire => Self::Unknown,
        }
    }
}

impl<Type> DependencyMutationKind<Type> {
    /// Whether contents change without ending the containing storage lifetime.
    pub const fn is_write(&self) -> bool {
        matches!(self, Self::Write | Self::Replace(_) | Self::Clear)
    }

    /// Translates the installed type while preserving the storage transition.
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
    fn unresolved_atomic_effect_absorbs_only_narrower_atomic_effects() {
        let subject = DependencySubject::root(super::super::DependencySubjectRoot::Receiver);

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct {
            subject: subject.clone(),
            kind: DependencyMutationKind::Write,
        };

        let atomic = DependencyStorageEffect::Direct {
            subject,
            kind: DependencyMutationKind::Atomic,
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

        let write: DependencyStorageEffect = DependencyStorageEffect::Direct {
            subject: DependencySubject::root(super::super::DependencySubjectRoot::Receiver),
            kind: DependencyMutationKind::Write,
        };

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), write.clone()]).as_ref(),
            &[write.clone()]
        );

        assert_eq!(
            DependencyStorageEffect::normalize([write.clone(), DependencyStorageEffect::Unknown])
                .as_ref(),
            &[DependencyStorageEffect::Unknown, write]
        );
    }
    #[test]
    fn unresolved_targets_preserve_atomic_and_escape_effects() {
        type Effect = DependencyStorageEffect<(), (), ()>;

        for (kind, expected) in [
            (DependencyMutationKind::Atomic, Effect::Atomic),
            (DependencyMutationKind::Escape, Effect::Escape),
            (DependencyMutationKind::Write, Effect::Unknown),
            (DependencyMutationKind::Replace(()), Effect::Unknown),
            (DependencyMutationKind::Clear, Effect::Unknown),
            (DependencyMutationKind::Consume, Effect::Unknown),
            (DependencyMutationKind::Retire, Effect::Unknown),
        ] {
            assert_eq!(Effect::unresolved(kind), expected);
        }
    }
}
