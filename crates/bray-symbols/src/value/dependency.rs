use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    StaticSymbolId, StructFieldSymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

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
    /// Storage owned by the evaluation whose returned contract is being inferred.
    EvaluationStorage,
    /// A scoped declaration capability by stable ordinal.
    ScopedCapability(SymbolOrdinal),
    /// A required selected implementation witness.
    ImplementationWitness(ImplementationInstanceId),
    /// Storage owned by one product-static instance.
    ProductStatic(StaticSymbolId),
    /// Storage owned by one exact native-thread static instance.
    ExactThreadStatic(StaticSymbolId),
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
    /// The dependencies carried by the value must remain valid after it transfers.
    ValueDependencies,
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
            requirements: sorted_unique_shared_slice(requirements),
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
    /// Finite equations for dependencies carried around a local control-flow cycle.
    FixedPoint {
        /// Equation values, addressed by variable ordinal.
        definitions: Arc<[Arc<[DependencyRequirement]>]>,
        /// Dependencies selected from the equation group.
        result: Arc<[DependencyRequirement]>,
    },
    /// A reference to an enclosing finite equation group.
    Variable {
        /// Number of intervening equation groups.
        depth: u32,
        /// Equation ordinal in the selected group.
        ordinal: SymbolOrdinal,
    },
    /// An unconditional requirement on one formal subject.
    Direct {
        /// Formal subject carrying the dependency.
        subject: DependencySubject,
        /// Exact required semantic state.
        kind: DependencyRequirementKind,
    },
    /// Requirements active only while a semantic guard holds.
    Guarded(GuardedDependencyRequirement),
    /// A deferred callable result, optionally awaiting implementation selection.
    ResultCall {
        /// The declaration and its generic arguments.
        callable: super::CallableInstanceId,
        /// The subject and trait application selecting an implementation, when needed.
        requirement: Option<crate::ImplementationRequirementKey>,
        /// Dependencies of each actual input in the enclosing contract.
        inputs: Arc<[DependencyCallInput]>,
    },
}

/// The storage and value dependencies supplied to one symbolic result call.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyCallInput {
    root: DependencySubjectRoot,
    values: Arc<[DependencyRequirement]>,
    storage: Arc<[DependencyRequirement]>,
}

impl DependencyCallInput {
    /// Creates an input with independently normalized value and storage requirements.
    pub fn new(
        root: DependencySubjectRoot,
        values: impl IntoIterator<Item = DependencyRequirement>,
        storage: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self {
            root,
            values: sorted_unique_shared_slice(values),
            storage: sorted_unique_shared_slice(storage),
        }
    }

    /// Returns the receiver or parameter being supplied.
    pub const fn root(&self) -> DependencySubjectRoot {
        self.root
    }

    /// Returns dependencies carried by the supplied value.
    pub fn values(&self) -> &[DependencyRequirement] {
        &self.values
    }

    /// Returns dependencies keeping the supplied storage alive.
    pub fn storage(&self) -> &[DependencyRequirement] {
        &self.storage
    }
}

impl DependencyRequirement {
    /// Creates a finite group of dependency equations.
    pub fn fixed_point(
        definitions: impl IntoIterator<Item = impl IntoIterator<Item = DependencyRequirement>>,
        result: impl IntoIterator<Item = DependencyRequirement>,
    ) -> Self {
        Self::FixedPoint {
            definitions: bray_base::shared_slice(
                definitions.into_iter().map(sorted_unique_shared_slice),
            ),
            result: sorted_unique_shared_slice(result),
        }
    }

    /// Refers to an enclosing dependency equation.
    pub const fn variable(depth: u32, ordinal: SymbolOrdinal) -> Self {
        Self::Variable { depth, ordinal }
    }

    /// Creates a deferred result relation with normalized inputs.
    pub fn result_call(
        callable: super::CallableInstanceId,
        requirement: Option<crate::ImplementationRequirementKey>,
        inputs: impl IntoIterator<Item = DependencyCallInput>,
    ) -> Self {
        Self::ResultCall {
            callable,
            requirement,
            inputs: sorted_unique_shared_slice(inputs),
        }
    }

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
            requirements: sorted_unique_shared_slice(requirements),
        }
    }

    /// Returns the normalized requirements.
    pub fn requirements(&self) -> &[DependencyRequirement] {
        &self.requirements
    }
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
