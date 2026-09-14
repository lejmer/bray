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
    /// Storage owned by the evaluation producing the result.
    EvaluationStorage,
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
    /// Returns the formal root of this subject.
    pub const fn subject_root(&self) -> &InterfaceDependencySubjectRoot {
        &self.root
    }

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
    /// Creates a finite group of dependency equations.
    pub fn fixed_point(
        definitions: impl IntoIterator<Item = impl IntoIterator<Item = InterfaceDependencyRequirement>>,
        result: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::FixedPoint {
                definitions: bray_base::shared_slice(
                    definitions.into_iter().map(sorted_unique_shared_slice),
                ),
                result: sorted_unique_shared_slice(result),
            },
        }
    }

    /// Refers to an enclosing dependency equation.
    pub const fn variable(depth: u32, ordinal: SymbolOrdinal) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::Variable { depth, ordinal },
        }
    }

    /// Creates a deferred result relation with normalized inputs.
    pub fn result_call(
        callable: super::InterfaceCallableInstanceId,
        requirement: Option<(super::InterfaceTypeId, super::InterfaceTraitApplicationId)>,
        inputs: impl IntoIterator<Item = InterfaceDependencyCallInput>,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::ResultCall {
                callable,
                requirement,
                inputs: sorted_unique_shared_slice(inputs),
            },
        }
    }

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
    /// Finite equations for dependencies carried around a local control-flow cycle.
    FixedPoint {
        /// Equation values, addressed by variable ordinal.
        definitions: Arc<[Arc<[InterfaceDependencyRequirement]>]>,
        /// Dependencies selected from the equation group.
        result: Arc<[InterfaceDependencyRequirement]>,
    },
    /// A reference to an enclosing finite equation group.
    Variable {
        /// Number of intervening equation groups.
        depth: u32,
        /// Equation ordinal in the selected group.
        ordinal: SymbolOrdinal,
    },
    /// A deferred callable result, optionally awaiting implementation selection.
    ResultCall {
        /// Callable instance.
        callable: super::InterfaceCallableInstanceId,
        /// Subject and trait application selecting an implementation, when needed.
        requirement: Option<(super::InterfaceTypeId, super::InterfaceTraitApplicationId)>,
        /// Enclosing dependencies mapped to callable inputs.
        inputs: Arc<[InterfaceDependencyCallInput]>,
    },
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

/// One input supplied to an unresolved result relation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependencyCallInput {
    pub(crate) root: InterfaceDependencySubjectRoot,
    pub(crate) values: Arc<[InterfaceDependencyRequirement]>,
    pub(crate) storage: Arc<[InterfaceDependencyRequirement]>,
}

impl InterfaceDependencyCallInput {
    /// Creates the normalized value and storage dependencies for one input.
    pub fn new(
        root: InterfaceDependencySubjectRoot,
        values: impl IntoIterator<Item = InterfaceDependencyRequirement>,
        storage: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        Self {
            root,
            values: sorted_unique_shared_slice(values),
            storage: sorted_unique_shared_slice(storage),
        }
    }
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
