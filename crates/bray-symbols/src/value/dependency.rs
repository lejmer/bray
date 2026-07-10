use std::sync::Arc;

use bray_base::shared_slice;

use crate::{StructFieldSymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId, UnionVariantSymbolId};

use super::{BorrowKind, ConstantTermId, ImplementationInstanceId};

/// A formal source-independent root referenced by a dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencySubjectRoot {
    /// The callable receiver.
    Receiver,
    /// A callable parameter by stable declaration ordinal.
    Parameter(SymbolOrdinal),
    /// The callable result.
    Result,
    /// A scoped declaration capability by stable ordinal.
    ScopedCapability(SymbolOrdinal),
    /// A required selected implementation witness.
    ImplementationWitness(ImplementationInstanceId),
}

/// One source-independent projection from a formal dependency subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyProjection {
    /// A named product field.
    ProductField(StructFieldSymbolId),
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// An array or slice element selected by a checked constant term.
    Element(ConstantTermId),
    /// The present value of a nullable subject.
    NullableValue,
    /// A named union payload field.
    UnionPayloadField(UnionPayloadFieldSymbolId),
    /// The target owned through an indirection layer.
    OwnedTarget,
}

/// A formal root plus its ordered source-independent projection path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencySubject {
    root: DependencySubjectRoot,
    projections: Arc<[DependencyProjection]>,
}

impl DependencySubject {
    /// Creates a formal dependency subject.
    pub fn new(
        root: DependencySubjectRoot,
        projections: impl IntoIterator<Item = DependencyProjection>,
    ) -> Self {
        Self {
            root,
            projections: shared_slice(projections),
        }
    }

    /// Creates a formal subject without projections.
    pub fn root(root: DependencySubjectRoot) -> Self {
        Self::new(root, [])
    }

    /// Returns the formal root.
    pub const fn subject_root(&self) -> DependencySubjectRoot {
        self.root
    }

    /// Returns the ordered projection path.
    pub fn projections(&self) -> &[DependencyProjection] {
        &self.projections
    }
}

/// A portable lifecycle obligation retained by a dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LifecycleObligationKind {
    /// Destruction must remain attached.
    Destruction,
    /// Finalization must remain attached.
    Finalization,
    /// Cancellation must remain attached.
    Cancellation,
    /// Joining must remain attached.
    Joining,
}

/// The exact requirement attached to one formal dependency subject.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyRequirementKind {
    /// Reached storage must remain alive.
    StorageAlive,
    /// Reached storage must remain initialized.
    StorageInitialized,
    /// A borrow capability of the exact kind must remain active.
    BorrowCapabilityActive(BorrowKind),
    /// Exclusive mutation authority must remain active.
    ExclusiveMutationAuthority,
    /// A scoped capability must remain live.
    ScopedCapabilityLive,
    /// A lifecycle obligation must remain attached.
    LifecycleObligation(LifecycleObligationKind),
}

/// A semantic condition guarding nested dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyGuard {
    /// The nullable subject is present.
    NullablePresent(DependencySubject),
    /// The union subject has the exact active variant.
    ActiveUnionVariant {
        /// The guarded union subject.
        subject: DependencySubject,
        /// The required active variant.
        variant: UnionVariantSymbolId,
    },
}

/// A normalized guarded set of dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GuardedDependencyRequirement {
    guard: DependencyGuard,
    requirements: Arc<[DependencyRequirement]>,
}

impl GuardedDependencyRequirement {
    /// Creates a guarded requirement after deterministic sorting and deduplication.
    pub fn new(
        guard: DependencyGuard,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            guard,
            requirements: normalize_requirements(requirements),
        }
    }

    /// Returns the semantic guard.
    pub const fn guard(&self) -> &DependencyGuard {
        &self.guard
    }

    /// Returns the normalized nested requirements.
    pub fn requirements(&self) -> &[DependencyRequirement] {
        &self.requirements
    }
}

/// One direct or guarded portable dependency requirement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyRequirement {
    /// An unconditional requirement on one formal subject.
    Direct {
        /// Formal subject carrying the dependency.
        subject: DependencySubject,
        /// Exact required semantic state.
        kind: DependencyRequirementKind,
    },
    /// Requirements active only while a semantic guard holds.
    Guarded(GuardedDependencyRequirement),
}

impl DependencyRequirement {
    /// Creates an unconditional requirement.
    pub fn direct(subject: DependencySubject, kind: DependencyRequirementKind) -> Self {
        Self::Direct { subject, kind }
    }

    /// Creates normalized guarded requirements.
    pub fn guarded(
        guard: DependencyGuard,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::Guarded(GuardedDependencyRequirement::new(guard, requirements))
    }
}

/// A normalized source-independent dependency-contract template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyContractTemplateData {
    requirements: Arc<[DependencyRequirement]>,
}

impl DependencyContractTemplateData {
    /// Creates a template after deterministic sorting and deduplication.
    pub fn new(requirements: impl IntoIterator<Item = DependencyRequirement>) -> Self {
        Self {
            requirements: normalize_requirements(requirements),
        }
    }

    /// Returns the normalized requirements.
    pub fn requirements(&self) -> &[DependencyRequirement] {
        &self.requirements
    }
}

fn normalize_requirements(
    requirements: impl IntoIterator<Item = DependencyRequirement>,
) -> Arc<[DependencyRequirement]> {
    let mut requirements: Vec<_> = requirements.into_iter().collect();
    requirements.sort_unstable();
    requirements.dedup();
    shared_slice(requirements)
}

#[cfg(test)]
mod tests {
    use crate::SymbolOrdinal;

    use super::{
        DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
        DependencySubject, DependencySubjectRoot,
    };

    #[test]
    fn dependency_templates_are_normalized() {
        let subject =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));
        let requirement =
            DependencyRequirement::direct(subject, DependencyRequirementKind::StorageAlive);
        let template =
            DependencyContractTemplateData::new([requirement.clone(), requirement.clone()]);

        assert_eq!(template.requirements(), &[requirement]);
    }
}
