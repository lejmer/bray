use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_symbols::{BorrowKind, LifecycleObligationKind, SymbolOrdinal};

use super::{InterfaceConstantTermId, InterfaceImplementationInstanceId};
use crate::InterfaceSymbolReference;

/// Source-independent formal root used by a dependency contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceDependencySubjectRoot {
    /// Callable receiver.
    Receiver,
    /// Callable parameter ordinal.
    Parameter(SymbolOrdinal),
    /// Callable result.
    Result,
    /// Scoped capability ordinal.
    ScopedCapability(SymbolOrdinal),
    /// Selected implementation witness.
    ImplementationWitness(InterfaceImplementationInstanceId),
    /// Product-static declaration identity.
    ProductStatic(InterfaceSymbolReference),
    /// Exact-thread static declaration identity.
    ExactThreadStatic(InterfaceSymbolReference),
}

/// One source-independent dependency-subject projection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceDependencyProjection {
    /// Named product field.
    ProductField(InterfaceSymbolReference),
    /// Tuple element ordinal.
    TupleElement(SymbolOrdinal),
    /// Array or slice element selected by a checked term.
    Element(InterfaceConstantTermId),
    /// Present nullable value.
    NullableValue,
    /// Named union payload field.
    UnionPayloadField(InterfaceSymbolReference),
    /// Target reached through owned indirection.
    OwnedTarget,
}

/// One formal dependency subject and its ordered projections.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependencySubject {
    pub(crate) root: InterfaceDependencySubjectRoot,
    pub(crate) projections: Arc<[InterfaceDependencyProjection]>,
}

impl InterfaceDependencySubject {
    /// Creates one formal dependency subject.
    pub fn new(
        root: InterfaceDependencySubjectRoot,
        projections: impl IntoIterator<Item = InterfaceDependencyProjection>,
    ) -> Self {
        Self {
            root,
            projections: projections.into_iter().collect(),
        }
    }
}

/// Semantic guard around nested dependency requirements.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceDependencyGuard {
    /// Nullable subject is present.
    NullablePresent(InterfaceDependencySubject),
    /// Union subject has the exact active variant.
    ActiveUnionVariant {
        /// Guarded subject.
        subject: InterfaceDependencySubject,
        /// Required active variant.
        variant: InterfaceSymbolReference,
    },
}

/// One portable dependency requirement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependencyRequirement {
    pub(crate) value: InterfaceDependencyRequirementValue,
}

impl InterfaceDependencyRequirement {
    /// Creates one direct portable dependency requirement.
    pub const fn new(
        subject: InterfaceDependencySubject,
        kind: InterfaceDependencyRequirementKind,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::Direct { subject, kind },
        }
    }

    /// Creates a guarded portable dependency requirement.
    pub fn guarded(
        guard: InterfaceDependencyGuard,
        requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::Guarded {
                guard,
                requirements: sorted_unique_shared_slice(requirements),
            },
        }
    }
}

/// Direct or guarded portable dependency requirement payload.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceDependencyRequirementValue {
    /// Unconditional requirement.
    Direct {
        /// Formal subject.
        subject: InterfaceDependencySubject,
        /// Required state.
        kind: InterfaceDependencyRequirementKind,
    },
    /// Requirements enabled by a semantic guard.
    Guarded {
        /// Enabling guard.
        guard: InterfaceDependencyGuard,
        /// Nested requirements.
        requirements: Arc<[InterfaceDependencyRequirement]>,
    },
}

/// Portable semantic state required by a dependency contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceDependencyRequirementKind {
    /// Dependencies carried by the transferred value.
    ValueDependencies,
    /// Reached storage remains alive.
    StorageAlive,
    /// Reached storage remains initialized.
    StorageInitialized,
    /// Exclusive mutation authority remains active.
    ExclusiveMutationAuthority,
    /// Borrow capability remains active.
    BorrowCapabilityActive(BorrowKind),
    /// A scoped capability remains live.
    ScopedCapabilityLive,
    /// A lifecycle obligation remains attached.
    LifecycleObligation(LifecycleObligationKind),
}

/// One normalized portable dependency contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependencyContract {
    pub(crate) requirements: Arc<[InterfaceDependencyRequirement]>,
}

impl InterfaceDependencyContract {
    /// Creates a dependency contract in canonical requirement order.
    pub fn new(requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::SymbolOrdinal;

    use super::{
        InterfaceDependencyContract, InterfaceDependencyGuard, InterfaceDependencyRequirement,
        InterfaceDependencyRequirementKind, InterfaceDependencyRequirementValue,
        InterfaceDependencySubject, InterfaceDependencySubjectRoot,
    };

    fn requirement(
        ordinal: u32,
        kind: InterfaceDependencyRequirementKind,
    ) -> InterfaceDependencyRequirement {
        InterfaceDependencyRequirement::new(
            InterfaceDependencySubject::new(
                InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(ordinal)),
                [],
            ),
            kind,
        )
    }

    #[test]
    fn dependency_contract_canonicalizes_requirements() {
        let first = requirement(0, InterfaceDependencyRequirementKind::StorageAlive);
        let second = requirement(1, InterfaceDependencyRequirementKind::StorageInitialized);
        let contract = InterfaceDependencyContract::new([second.clone(), first.clone(), second]);

        assert_eq!(
            &*contract.requirements,
            &[
                first,
                requirement(1, InterfaceDependencyRequirementKind::StorageInitialized),
            ]
        );
    }

    #[test]
    fn guarded_requirement_canonicalizes_nested_requirements() {
        let subject = InterfaceDependencySubject::new(
            InterfaceDependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            [],
        );

        let first = requirement(0, InterfaceDependencyRequirementKind::StorageAlive);
        let second = requirement(1, InterfaceDependencyRequirementKind::StorageInitialized);

        let guarded = InterfaceDependencyRequirement::guarded(
            InterfaceDependencyGuard::NullablePresent(subject),
            [second.clone(), first.clone(), second],
        );

        let InterfaceDependencyRequirementValue::Guarded { requirements, .. } = guarded.value
        else {
            panic!("guarded constructor must produce a guarded requirement");
        };

        assert_eq!(
            &*requirements,
            &[
                first,
                requirement(1, InterfaceDependencyRequirementKind::StorageInitialized),
            ]
        );
    }
}
