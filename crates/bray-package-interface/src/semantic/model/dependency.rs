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
    /// Subject and exact trait application whose witness survives type erasure.
    ImplementationRequirement(super::InterfaceTypeId, super::InterfaceTraitApplicationId),
    /// Product-static declaration identity.
    ProductStatic(InterfaceSymbolReference),
    /// Exact-thread static declaration identity.
    ExactThreadStatic(InterfaceSymbolReference),
    /// One selected static specialization and its ownership domain.
    StaticInstance(super::InterfaceStaticReference, bray_symbols::StaticStorageDuration),
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
    /// A region in an unspecified array or slice element, without an exact address.
    AnyElement,
    /// The value reached through a raw data address, without access or ownership.
    AddressTarget,
    /// Present nullable value.
    NullableValue,
    /// Named union payload field.
    UnionPayloadField(InterfaceSymbolReference),
    /// Target reached through owned indirection.
    OwnedTarget,
    /// Inline protected storage. This projection establishes neither initialization nor access authority.
    UninitPayload,
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
    /// Retains report dependencies only for captures with cleanup work.
    pub fn cleanup_dependencies(
        ty: crate::InterfaceTypeId,
        requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::CleanupDependencies {
                ty,
                requirements: sorted_unique_shared_slice(requirements),
            },
        }
    }

    /// Requires the selected type's remaining cleanup to execute synchronously.
    pub const fn synchronous_cleanup(ty: crate::InterfaceTypeId) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::SynchronousCleanup(ty),
        }
    }

    /// Requires the selected type's pending finalizer body to admit its callees.
    pub const fn finalizer_body_cleanup(ty: crate::InterfaceTypeId) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::FinalizerBodyCleanup(ty),
        }
    }

    /// Retains cleanup in one selected declaration-owned runtime default.
    pub const fn default_cleanup(
        provider: InterfaceSymbolReference,
        substitution: super::InterfaceGenericSubstitutionId,
        contextual_self: Option<(InterfaceSymbolReference, crate::InterfaceTypeId)>,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::DefaultCleanup {
                provider,
                substitution,
                contextual_self,
            },
        }
    }

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
        output: bray_symbols::DependencyCallOutput,
    ) -> Self {
        Self {
            value: InterfaceDependencyRequirementValue::ResultCall {
                output,
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
    /// The selected type's remaining cleanup must execute synchronously.
    SynchronousCleanup(crate::InterfaceTypeId),
    /// A pending finalizer body must admit its synchronous callees.
    FinalizerBodyCleanup(crate::InterfaceTypeId),
    /// Report dependencies conditional on the capture type's cleanup work.
    CleanupDependencies {
        /// Selected capture type.
        ty: crate::InterfaceTypeId,
        /// Nested report requirements.
        requirements: Arc<[InterfaceDependencyRequirement]>,
    },
    /// Cleanup performed by one selected runtime-default provider.
    DefaultCleanup {
        /// Exact parameter or field provider.
        provider: InterfaceSymbolReference,
        /// Enclosing declaration's selected generic arguments.
        substitution: super::InterfaceGenericSubstitutionId,
        /// Lexical Self context and its selected subject, when present.
        contextual_self: Option<(InterfaceSymbolReference, crate::InterfaceTypeId)>,
    },
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
        /// The selected value or owned report output.
        output: bray_symbols::DependencyCallOutput,
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

/// Selected callable, signature and witnesses in the package interface.
pub type InterfaceCapturedCallable = bray_symbols::CapturedCallable<crate::InterfaceCallableInstanceId, crate::InterfaceTypeId,
    (crate::InterfaceTypeId, crate::InterfaceTraitApplicationId), crate::InterfaceImplementationInstanceId>;

/// Portable effect with interface-local subject, projection and type identities.
pub type InterfaceStorageEffect = bray_symbols::DependencyStorageEffect<
    InterfaceDependencySubject, InterfaceDependencyProjection, crate::InterfaceTypeId,
>;

/// Guarded normal values and ownership using interface-local identities.
pub type InterfaceNormalOutcome = bray_symbols::CapturedValueOutcome<InterfaceDependencySubject, InterfaceConstantTermId,
    InterfaceDependencyProjection, super::InterfaceStaticReference, InterfaceConstantTermId, super::InterfaceConstantProjection, InterfaceCapturedCallable, crate::InterfaceSymbolReference, InterfaceDependencyRequirement, InterfaceStorageEffect, InterfaceDependencySubject>;

/// One abnormal outcome using interface-local identities.
pub type InterfaceFailureOutcome = bray_symbols::CapturedFailureOutcome<InterfaceConstantTermId,
    Arc<[InterfaceDependencyRequirement]>, InterfaceStorageEffect>;

/// One normalized portable dependency contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependencyContract {
    pub(crate) captured_values: Arc<[InterfaceNormalOutcome]>,
    pub(crate) observed_result: Option<super::InterfaceConstantTermId>,
    pub(crate) synchronous_cleanup: Arc<[InterfaceDependencyRequirement]>,
    pub(crate) normal_completion: bool,
    pub(crate) requirements: Arc<[InterfaceDependencyRequirement]>,
    pub(crate) failure_outcomes: Option<Arc<[InterfaceFailureOutcome]>>,
    pub(crate) effects: Arc<[InterfaceStorageEffect]>,
}

impl InterfaceDependencyContract {
    /// Records actual whole-value bindings produced by normal completion.
    pub fn with_captured_values(mut self, values: impl IntoIterator<Item = InterfaceNormalOutcome>) -> Self {
        self.captured_values = sorted_unique_shared_slice(values);

        self
    }

    /// Records the value observation without introducing storage identity or authority.
    pub fn with_observed_result(mut self, result: Option<super::InterfaceConstantTermId>) -> Self {
        self.observed_result = result;

        self
    }

    /// Creates a dependency contract in canonical requirement order.
    pub fn new(requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            captured_values: Arc::from([]),
            failure_outcomes: None,
            synchronous_cleanup: Arc::new([]),
            normal_completion: true,
            observed_result: None,
            effects: sorted_unique_shared_slice(InterfaceStorageEffect::opaque_call()),
        }
    }

    /// Records dependencies of reports accompanying normal completion.
    /// Presence with no dependencies still denotes a possible report.
    pub fn with_completion_reports(
        mut self,
        requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        let report = sorted_unique_shared_slice(requirements);

        if self.captured_values.is_empty() {
            self.captured_values = [InterfaceNormalOutcome::new(None, [])
                .with_effects(self.effects.iter().cloned()).with_report(Some(report))].into();
        } else {
            self.captured_values = sorted_unique_shared_slice(self.captured_values.iter().cloned()
                .map(|row| row.with_report(Some(Arc::clone(&report)))));
        }

        self
    }

    /// Describes a possible report from a callable whose failure prefix is opaque.
    pub fn with_abnormal_reports(
        self,
        requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        self.with_failure_outcomes(Some([InterfaceFailureOutcome {
            kind: None,
            guard: None,
            report: Some(sorted_unique_shared_slice(requirements)),
            effects: None,
        }]))
    }

    /// Attaches the alternatives that can reach abnormal completion.
    pub fn with_failure_outcomes(mut self, outcomes: Option<impl IntoIterator<Item = InterfaceFailureOutcome>>) -> Self {
        self.failure_outcomes = outcomes.map(sorted_unique_shared_slice);

        self
    }

    pub(crate) fn all_requirements(&self) -> impl Iterator<Item = &InterfaceDependencyRequirement> {
        self.requirements
            .iter()
            .chain(
                self.captured_values.iter().flat_map(|row| row.report.iter().flat_map(|requirements| requirements.iter())),
            )
            .chain(
                self.failure_reports().flatten(),
            )
            .chain(self.synchronous_cleanup.iter())
    }

    pub(crate) fn failure_reports(&self) -> impl Iterator<Item = &[InterfaceDependencyRequirement]> {
        self.failure_outcomes.as_deref().into_iter().flatten().filter_map(|row| row.report.as_deref())
    }

    /// Records whether execution can complete normally.
    pub fn with_normal_completion(mut self, possible: bool) -> Self {
        self.normal_completion = possible;

        self
    }

    pub(crate) fn normal_effects(&self) -> impl Iterator<Item = &InterfaceStorageEffect> {
        self.effects.iter().filter(|_| self.normal_completion && self.captured_values.is_empty())
            .chain(self.captured_values.iter().filter(|_| self.normal_completion).flat_map(|row| row.effects.iter()))
    }

    /// Records cleanup that must be synchronous after generic substitution.
    pub fn with_synchronous_cleanup(
        mut self,
        requirements: impl IntoIterator<Item = InterfaceDependencyRequirement>,
    ) -> Self {
        self.synchronous_cleanup = sorted_unique_shared_slice(requirements);

        self
    }

    /// Returns deferred synchronous cleanup obligations.
    pub fn synchronous_cleanup(&self) -> &[InterfaceDependencyRequirement] {
        &self.synchronous_cleanup
    }

    /// Attaches the conservative transition envelope across all outcomes.
    pub fn with_effects(mut self, effects: impl IntoIterator<Item = InterfaceStorageEffect>) -> Self {
        self.effects = sorted_unique_shared_slice(effects);

        self
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
