use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    DependencyContractTemplateId, GenericOwnerId, GenericSubstitutionId, SymbolOrdinal,
    TraitApplicationId, TrustedCapabilitySymbolId, TypeId,
};

/// The result of attempting to prove one semantic predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProofOutcome {
    /// Available semantic facts establish the predicate.
    Proven,
    /// Available semantic facts establish that the predicate is false.
    Disproven,
    /// Available semantic facts are insufficient to decide the predicate.
    Unknown,
    /// Earlier recovery prevents a sound proof.
    Recovered,
}

impl ProofOutcome {
    /// Combines ordered obligations using logical conjunction.
    pub const fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Disproven, _) | (_, Self::Disproven) => Self::Disproven,
            (Self::Recovered, _) | (_, Self::Recovered) => Self::Recovered,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Proven, Self::Proven) => Self::Proven,
        }
    }
}

/// The exact generic declaration instance whose constraints must hold.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericConstraintObligationKey {
    owner: GenericOwnerId,
    substitution: GenericSubstitutionId,
}

impl GenericConstraintObligationKey {
    /// Creates an obligation for one exact generic declaration instance.
    pub const fn new(owner: GenericOwnerId, substitution: GenericSubstitutionId) -> Self {
        Self {
            owner,
            substitution,
        }
    }

    /// Returns the constrained generic declaration.
    pub const fn owner(self) -> GenericOwnerId {
        self.owner
    }

    /// Returns the exact arguments used to instantiate the declaration.
    pub const fn substitution(self) -> GenericSubstitutionId {
        self.substitution
    }
}

/// A checked source-independent semantic predicate summary.
///
/// The full source-shaped checked expression remains owned by `bray-bound-tree`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PredicateSemanticSummary {
    dependency_contract: DependencyContractTemplateId,
}

impl PredicateSemanticSummary {
    /// Creates a checked predicate summary.
    pub const fn new(dependency_contract: DependencyContractTemplateId) -> Self {
        Self {
            dependency_contract,
        }
    }

    /// Returns the portable dependencies of the predicate expression.
    pub const fn dependency_contract(self) -> DependencyContractTemplateId {
        self.dependency_contract
    }
}

/// Source-independent meaning of one checked generic constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedConstraintKind {
    /// A constant predicate that must evaluate to true.
    Predicate(PredicateSemanticSummary),
    /// A subject type that must satisfy an exact applied trait.
    TraitSatisfaction {
        /// The implementation-eligible subject type.
        subject: TypeId,
        /// The required applied trait.
        application: TraitApplicationId,
    },
}

/// One checked generic constraint in declaration order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedConstraint {
    ordinal: SymbolOrdinal,
    kind: CheckedConstraintKind,
}

impl CheckedConstraint {
    /// Creates a checked generic constraint.
    pub const fn new(ordinal: SymbolOrdinal, predicate: PredicateSemanticSummary) -> Self {
        Self {
            ordinal,
            kind: CheckedConstraintKind::Predicate(predicate),
        }
    }

    /// Creates a checked trait-satisfaction constraint.
    pub const fn trait_satisfaction(
        ordinal: SymbolOrdinal,
        subject: TypeId,
        application: TraitApplicationId,
    ) -> Self {
        Self {
            ordinal,
            kind: CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            },
        }
    }

    /// Returns the stable declaration-order position within the owning constraint list.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the checked predicate meaning.
    pub const fn kind(self) -> CheckedConstraintKind {
        self.kind
    }

    /// Returns predicate meaning when this is a constant predicate constraint.
    pub const fn predicate(self) -> Option<PredicateSemanticSummary> {
        match self.kind {
            CheckedConstraintKind::Predicate(predicate) => Some(predicate),
            CheckedConstraintKind::TraitSatisfaction { .. } => None,
        }
    }

    /// Returns the subject and application when this is a trait-satisfaction constraint.
    pub const fn trait_satisfaction_requirement(self) -> Option<(TypeId, TraitApplicationId)> {
        match self.kind {
            CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } => Some((subject, application)),
            CheckedConstraintKind::Predicate(_) => None,
        }
    }
}

/// Ordered checked constraints owned by one generic declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericConstraintSet {
    constraints: Arc<[CheckedConstraint]>,
}

/// One generic constraint that supplies a concrete implementation during specialization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericConstraintDispatch {
    owner: crate::GenericOwnerId,
    ordinal: crate::SymbolOrdinal,
}

impl GenericConstraintDispatch {
    /// Creates a dispatch reference to one declared generic constraint.
    pub const fn new(owner: crate::GenericOwnerId, ordinal: crate::SymbolOrdinal) -> Self {
        Self { owner, ordinal }
    }

    /// Returns the generic declaration owning the constraint.
    pub const fn owner(self) -> crate::GenericOwnerId {
        self.owner
    }

    /// Returns the constraint's declaration-order position.
    pub const fn ordinal(self) -> crate::SymbolOrdinal {
        self.ordinal
    }
}

impl GenericConstraintSet {
    /// Creates a checked constraint set in canonical declaration order.
    pub fn new(constraints: impl IntoIterator<Item = CheckedConstraint>) -> Self {
        Self {
            constraints: shared_slice(constraints),
        }
    }

    /// Returns checked constraints in declaration order.
    pub fn constraints(&self) -> &[CheckedConstraint] {
        &self.constraints
    }
}

/// The semantic role of one checked callable contract predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableContractClauseKind {
    /// A precondition required at the call boundary.
    Requires,
    /// A postcondition established by successful completion.
    Ensures,
    /// A static predicate required while selecting or instantiating the callable.
    Static,
}

/// The checked meaning carried by one callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableContractClauseValue {
    /// A predicate expression checked in the clause's contract context.
    Predicate(PredicateSemanticSummary),
    /// A subject type that must satisfy an exact applied trait.
    TraitSatisfaction {
        /// The implementation-eligible subject type.
        subject: TypeId,
        /// The required applied trait.
        application: TraitApplicationId,
    },
}

/// One checked callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableContractClause {
    ordinal: SymbolOrdinal,
    kind: CallableContractClauseKind,
    value: CallableContractClauseValue,
}

impl CallableContractClause {
    /// Creates one checked callable contract clause.
    pub const fn new(
        ordinal: SymbolOrdinal,
        kind: CallableContractClauseKind,
        predicate: PredicateSemanticSummary,
    ) -> Self {
        Self {
            ordinal,
            kind,
            value: CallableContractClauseValue::Predicate(predicate),
        }
    }

    /// Creates one checked static trait-satisfaction constraint.
    pub const fn trait_satisfaction(
        ordinal: SymbolOrdinal,
        subject: TypeId,
        application: TraitApplicationId,
    ) -> Self {
        Self {
            ordinal,
            kind: CallableContractClauseKind::Static,
            value: CallableContractClauseValue::TraitSatisfaction {
                subject,
                application,
            },
        }
    }

    /// Returns the stable declaration-order position within the owning contract list.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the semantic clause role.
    pub const fn kind(self) -> CallableContractClauseKind {
        self.kind
    }

    /// Returns the checked clause meaning.
    pub const fn value(self) -> CallableContractClauseValue {
        self.value
    }

    /// Returns predicate meaning when this clause contains a predicate expression.
    pub const fn predicate(self) -> Option<PredicateSemanticSummary> {
        match self.value {
            CallableContractClauseValue::Predicate(predicate) => Some(predicate),
            CallableContractClauseValue::TraitSatisfaction { .. } => None,
        }
    }

    /// Returns the subject and application for a trait-satisfaction constraint.
    pub const fn trait_satisfaction_requirement(self) -> Option<(TypeId, TraitApplicationId)> {
        match self.value {
            CallableContractClauseValue::TraitSatisfaction {
                subject,
                application,
            } => Some((subject, application)),
            CallableContractClauseValue::Predicate(_) => None,
        }
    }
}

/// One trusted capability named by a callable `uses` clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TrustedCapabilityRequirement {
    ordinal: SymbolOrdinal,
    capability: TrustedCapabilitySymbolId,
}

impl TrustedCapabilityRequirement {
    /// Creates a checked trusted-capability requirement.
    pub const fn new(ordinal: SymbolOrdinal, capability: TrustedCapabilitySymbolId) -> Self {
        Self {
            ordinal,
            capability,
        }
    }

    /// Returns the stable declaration-order position within the owning capability list.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the exact resolved capability declaration.
    pub const fn capability(self) -> TrustedCapabilitySymbolId {
        self.capability
    }
}

/// Checked callable contracts, trusted obligations, and portable dependencies.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableContractSet {
    invocation_preconditions: Arc<[CallableContractClause]>,
    static_constraints: Arc<[CallableContractClause]>,
    normal_completion_postconditions: Arc<[CallableContractClause]>,
    phase_behaviors: super::CallablePhaseBehaviors,
}

impl CallableContractSet {
    /// Creates a checked callable contract with explicit invocation and completion phases.
    pub fn new(
        clauses: impl IntoIterator<Item = CallableContractClause>,
        invocation_behavior: super::CallablePhaseBehavior,
        deferred_execution_behavior: Option<super::CallablePhaseBehavior>,
    ) -> Self {
        let mut invocation_preconditions = Vec::new();
        let mut static_constraints = Vec::new();
        let mut normal_completion_postconditions = Vec::new();

        for clause in clauses {
            match clause.kind() {
                CallableContractClauseKind::Requires => invocation_preconditions.push(clause),
                CallableContractClauseKind::Ensures => {
                    normal_completion_postconditions.push(clause);
                }
                CallableContractClauseKind::Static => static_constraints.push(clause),
            }
        }

        Self {
            invocation_preconditions: shared_slice(invocation_preconditions),
            static_constraints: shared_slice(static_constraints),
            normal_completion_postconditions: shared_slice(normal_completion_postconditions),
            phase_behaviors: match deferred_execution_behavior {
                Some(deferred) => {
                    super::CallablePhaseBehaviors::asynchronous(invocation_behavior, deferred)
                }
                None => super::CallablePhaseBehaviors::synchronous(invocation_behavior),
            },
        }
    }

    /// Returns preconditions checked before callable invocation.
    pub fn invocation_preconditions(&self) -> &[CallableContractClause] {
        &self.invocation_preconditions
    }

    /// Returns constraints checked while selecting or instantiating the callable.
    pub fn static_constraints(&self) -> &[CallableContractClause] {
        &self.static_constraints
    }

    /// Returns facts published exclusively after normal completion.
    pub fn normal_completion_postconditions(&self) -> &[CallableContractClause] {
        &self.normal_completion_postconditions
    }

    /// Returns behavior incurred while invoking the callable.
    pub fn invocation_behavior(&self) -> &super::CallablePhaseBehavior {
        self.phase_behaviors.invocation()
    }

    /// Returns behavior retained by an async future until direct await or task execution.
    ///
    /// Starting the future transfers this contract unchanged to the produced task.
    pub fn deferred_execution_behavior(&self) -> Option<&super::CallablePhaseBehavior> {
        self.phase_behaviors.deferred_execution()
    }

    /// Returns behavior for every callable execution phase.
    pub const fn phase_behaviors(&self) -> &super::CallablePhaseBehaviors {
        &self.phase_behaviors
    }
}

/// A checked semantic predicate definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PredicateDefinition {
    semantic: PredicateSemanticSummary,
}

impl PredicateDefinition {
    /// Creates a checked semantic predicate definition.
    pub const fn new(semantic: PredicateSemanticSummary) -> Self {
        Self { semantic }
    }

    /// Returns the checked predicate meaning.
    pub const fn semantic(self) -> PredicateSemanticSummary {
        self.semantic
    }
}

/// Marks an invalid predicate definition whose diagnostics belong to the fact result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ErrorPredicateDefinition;

/// The checked declaration state of a predicate symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PredicateDefinitionState<T> {
    /// A checked semantic definition is available.
    Defined(T),
    /// A trait member requires a definition from an implementation.
    Required,
    /// The trusted declaration intentionally hides its implementation.
    OpaqueTrusted,
    /// Checking failed and diagnostics are retained by the fact result.
    Error(ErrorPredicateDefinition),
}

#[cfg(test)]
mod tests {
    use super::{
        CallableContractClause, CallableContractClauseKind, CallableContractSet,
        GenericConstraintSet, PredicateDefinitionState, PredicateSemanticSummary, ProofOutcome,
    };
    use crate::{
        CallablePhaseBehavior, CurrentRunCancellation, DependencyContractTemplateData,
        SemanticValueStore, SymbolOrdinal,
    };

    #[test]
    fn empty_constraint_sets_are_stable() {
        let constraints = GenericConstraintSet::new([]);

        assert!(constraints.constraints().is_empty());
    }

    #[test]
    fn predicate_proofs_combine_as_conjunctions() {
        assert_eq!(
            ProofOutcome::Proven.and(ProofOutcome::Proven),
            ProofOutcome::Proven
        );

        assert_eq!(
            ProofOutcome::Proven.and(ProofOutcome::Unknown),
            ProofOutcome::Unknown
        );

        assert_eq!(
            ProofOutcome::Unknown.and(ProofOutcome::Recovered),
            ProofOutcome::Recovered
        );

        assert_eq!(
            ProofOutcome::Recovered.and(ProofOutcome::Disproven),
            ProofOutcome::Disproven
        );
    }

    #[test]
    fn predicate_fact_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CallableContractSet>();
        assert_send_sync::<PredicateDefinitionState<super::PredicateDefinition>>();
    }

    #[test]
    fn callable_contracts_separate_invocation_and_normal_completion() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store must be available");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let predicate = PredicateSemanticSummary::new(dependencies);

        let behavior = CallablePhaseBehavior::new(
            [],
            [],
            [],
            [],
            [],
            dependencies,
            CurrentRunCancellation::MayEnter,
        );

        let contract = CallableContractSet::new(
            [
                CallableContractClause::new(
                    SymbolOrdinal::new(0),
                    CallableContractClauseKind::Requires,
                    predicate,
                ),
                CallableContractClause::new(
                    SymbolOrdinal::new(1),
                    CallableContractClauseKind::Ensures,
                    predicate,
                ),
            ],
            CallablePhaseBehavior::empty(dependencies),
            Some(behavior),
        );

        assert_eq!(contract.invocation_preconditions().len(), 1);
        assert_eq!(contract.normal_completion_postconditions().len(), 1);

        assert_eq!(
            contract
                .deferred_execution_behavior()
                .map(CallablePhaseBehavior::current_run_cancellation),
            Some(CurrentRunCancellation::MayEnter)
        );
    }
}
