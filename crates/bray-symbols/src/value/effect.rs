use std::sync::Arc;

use super::{DependencyProjection, DependencySubject};

/// The storage transition a callable may perform at an invocation or deferred phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyMutationKind {
    /// Replace contents while retaining the storage's initialized lifetime.
    Write,
    /// Remove initialized contents without retiring the storage.
    Consume,
    /// End the storage lifetime and revoke dependencies on that lifetime.
    Retire,
}

/// A possible mutation expressed over entry values, separately from their reference targets.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyStorageEffect<Subject = DependencySubject, Projection = DependencyProjection> {
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

impl<Subject: Ord, Projection: Ord> DependencyStorageEffect<Subject, Projection> {
    /// Normalizes the finite may set, with unknown absorbing narrower effects.
    pub fn normalize(effects: impl IntoIterator<Item = Self>) -> Arc<[Self]> {
        let effects = effects.into_iter().collect::<Vec<_>>();

        if effects.iter().any(|effect| matches!(effect, Self::Unknown)) {
            return Arc::new([Self::Unknown]);
        }

        bray_base::sorted_unique_shared_slice(effects)
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyMutationKind, DependencyStorageEffect, DependencySubject};
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
