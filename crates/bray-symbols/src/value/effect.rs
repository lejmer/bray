use std::sync::Arc;

use super::{DependencyProjection, DependencySubject};

/// The storage transition a callable may perform at an invocation or deferred phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyMutationKind {
    /// Mutate protected atomic payload without replacing any containing value.
    Atomic,
    /// Replace contents while retaining the storage's initialized lifetime.
    Write,
    /// Remove initialized contents without retiring the storage.
    Consume,
    /// End the storage lifetime and revoke dependencies on that lifetime.
    Retire,
}

/// A possible mutation expressed over entry values, separately from their reference targets.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyStorageEffect<Subject = DependencySubject, Projection = DependencyProjection, Type = super::TypeId> {
    /// Opaque effects of disposing this type. No effects occur when the type needs no cleanup.
    Cleanup(Type),
    /// Protected atomic payloads may change through unresolved aliases.
    Atomic,
    /// No finite target set has been established.
    Unknown,
    /// Mutate the value slot named by the formal subject.
    Direct {
        /// Entry value slot.
        subject: Subject,
        /// Possible transition.
        kind: DependencyMutationKind,
    },
    /// Mutate storage reached by the reference value held in an entry slot.
    Indirect {
        /// Slot containing the entry reference. Reassigning this slot does not change that identity.
        reference: Subject,
        /// Checked projections after dereferencing the entry reference.
        projections: Arc<[Projection]>,
        /// Possible transition.
        kind: DependencyMutationKind,
    },
}

impl<Subject: Ord, Projection: Ord, Type: Ord> DependencyStorageEffect<Subject, Projection, Type> {
    /// Normalizes the finite may set, with unknown absorbing narrower effects.
    pub fn normalize(effects: impl IntoIterator<Item = Self>) -> Arc<[Self]> {
        let mut effects = effects.into_iter().collect::<Vec<_>>();

        if effects.iter().any(|effect| matches!(effect, Self::Unknown)) {
            return Arc::new([Self::Unknown]);
        }

        if effects.iter().any(|effect| matches!(effect, Self::Atomic)) {
            effects.retain(|effect| !matches!(effect,
                Self::Direct { kind: DependencyMutationKind::Atomic, .. }
                | Self::Indirect { kind: DependencyMutationKind::Atomic, .. }));
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
            subject: subject.clone(), kind: DependencyMutationKind::Write,
        };

        let atomic = DependencyStorageEffect::Direct {
            subject, kind: DependencyMutationKind::Atomic,
        };

        assert_eq!(DependencyStorageEffect::normalize([
            atomic, write.clone(), DependencyStorageEffect::Atomic,
        ]).as_ref(), &[DependencyStorageEffect::Atomic, write]);
    }

    #[test]
    fn absent_summary_is_not_a_proof_of_no_mutation() {
        let missing = super::super::DependencyContractTemplateData::new([]);
        let unchanged = missing.clone().with_effects([]);

        assert_eq!(missing.effects(), &[DependencyStorageEffect::Unknown]);
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
            DependencyStorageEffect::normalize([write, DependencyStorageEffect::Unknown]).as_ref(),
            &[DependencyStorageEffect::Unknown]
        );
    }
}
