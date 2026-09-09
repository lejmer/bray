use std::sync::Arc;

use bray_base::{NonEmptySharedStr, sorted_unique_shared_slice};
use bray_package_interface_model::InterfaceSemanticRecordKind;
use bray_runtime_interface::{ProtectedAsyncFrameId, RuntimeRequirements};
use bray_symbols::{
    CallableAbi, CallableContractClauseKind, CurrentRunCancellation, LifecycleObligationKind,
    SymbolOrdinal,
};

use super::{
    InterfaceConstantValueId, InterfaceDependencyContractId, InterfaceTraitApplicationId,
    InterfaceTypeId,
};
use crate::InterfaceSymbolReference;

/// Source-independent meaning of one checked predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfacePredicateSummary {
    pub(crate) dependency_contract: InterfaceDependencyContractId,
    pub(crate) condition: Option<super::InterfaceConstantTermId>,
}

impl InterfacePredicateSummary {
    /// Creates one predicate summary.
    pub const fn new(dependency_contract: InterfaceDependencyContractId) -> Self {
        Self {
            dependency_contract,
            condition: None,
        }
    }

    /// Retains symbolic predicate meaning, not a proof of truth.
    pub const fn with_condition(
        mut self,
        condition: Option<super::InterfaceConstantTermId>,
    ) -> Self {
        self.condition = condition;

        self
    }

    /// Returns the retained predicate expression when available.
    pub const fn condition(self) -> Option<super::InterfaceConstantTermId> {
        self.condition
    }

    /// Returns the predicate expression's portable dependency contract.
    pub const fn dependency_contract(self) -> InterfaceDependencyContractId {
        self.dependency_contract
    }
}

/// Source-independent meaning of one exported generic constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceConstraintKind {
    /// A constant predicate that must evaluate to true.
    Predicate(InterfacePredicateSummary),
    /// A subject type that must satisfy an exact applied trait.
    TraitSatisfaction {
        /// The implementation-eligible subject type.
        subject: InterfaceTypeId,
        /// The required applied trait.
        application: InterfaceTraitApplicationId,
    },
    /// Two types that must have the same semantic identity.
    TypeEquality {
        /// The left type.
        left: InterfaceTypeId,
        /// The right type.
        right: InterfaceTypeId,
    },
}

/// One generic constraint attached to an exported declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceConstraint {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) ordinal: SymbolOrdinal,
    pub(crate) kind: InterfaceConstraintKind,
}

impl InterfaceConstraint {
    /// Creates one checked generic constraint.
    pub const fn new(
        owner: InterfaceSymbolReference,
        ordinal: SymbolOrdinal,
        predicate: InterfacePredicateSummary,
    ) -> Self {
        Self {
            owner,
            ordinal,
            kind: InterfaceConstraintKind::Predicate(predicate),
        }
    }

    /// Creates one checked trait-satisfaction constraint.
    pub const fn trait_satisfaction(
        owner: InterfaceSymbolReference,
        ordinal: SymbolOrdinal,
        subject: InterfaceTypeId,
        application: InterfaceTraitApplicationId,
    ) -> Self {
        Self {
            owner,
            ordinal,
            kind: InterfaceConstraintKind::TraitSatisfaction {
                subject,
                application,
            },
        }
    }

    /// Creates one checked type-equality constraint.
    pub const fn type_equality(
        owner: InterfaceSymbolReference,
        ordinal: SymbolOrdinal,
        left: InterfaceTypeId,
        right: InterfaceTypeId,
    ) -> Self {
        Self {
            owner,
            ordinal,
            kind: InterfaceConstraintKind::TypeEquality { left, right },
        }
    }

    /// Returns the source-independent constraint meaning.
    pub const fn kind(&self) -> InterfaceConstraintKind {
        self.kind
    }
}

/// The checked meaning carried by one callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceCallableContractClauseValue {
    /// A predicate expression checked in the clause's contract context.
    Predicate(InterfacePredicateSummary),
    /// A subject type that must satisfy an exact applied trait.
    TraitSatisfaction {
        /// The implementation-eligible subject type.
        subject: InterfaceTypeId,
        /// The required applied trait.
        application: InterfaceTraitApplicationId,
    },
}

/// One checked callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableContractClause {
    pub(crate) ordinal: SymbolOrdinal,
    pub(crate) kind: CallableContractClauseKind,
    pub(crate) value: InterfaceCallableContractClauseValue,
    pub(crate) guard: Option<SymbolOrdinal>,
}

impl InterfaceCallableContractClause {
    pub(crate) fn try_map_ids<E>(
        mut self,
        mut dependency: impl FnMut(
            InterfaceDependencyContractId,
        ) -> Result<InterfaceDependencyContractId, E>,
        mut term: impl FnMut(
            super::InterfaceConstantTermId,
        ) -> Result<super::InterfaceConstantTermId, E>,
        mut ty: impl FnMut(InterfaceTypeId) -> Result<InterfaceTypeId, E>,
        mut application: impl FnMut(
            InterfaceTraitApplicationId,
        ) -> Result<InterfaceTraitApplicationId, E>,
    ) -> Result<Self, E> {
        self.value = match self.value {
            InterfaceCallableContractClauseValue::Predicate(predicate) => {
                InterfaceCallableContractClauseValue::Predicate(
                    InterfacePredicateSummary::new(dependency(predicate.dependency_contract)?)
                        .with_condition(predicate.condition.map(&mut term).transpose()?),
                )
            }
            InterfaceCallableContractClauseValue::TraitSatisfaction {
                subject,
                application: trait_application,
            } => InterfaceCallableContractClauseValue::TraitSatisfaction {
                subject: ty(subject)?,
                application: application(trait_application)?,
            },
        };

        Ok(self)
    }

    /// Creates one checked callable contract clause.
    pub const fn new(
        ordinal: SymbolOrdinal,
        kind: CallableContractClauseKind,
        predicate: InterfacePredicateSummary,
    ) -> Self {
        Self {
            ordinal,
            kind,
            value: InterfaceCallableContractClauseValue::Predicate(predicate),
            guard: None,
        }
    }

    /// Creates one checked static trait-satisfaction constraint.
    pub const fn trait_satisfaction(
        ordinal: SymbolOrdinal,
        subject: InterfaceTypeId,
        application: InterfaceTraitApplicationId,
    ) -> Self {
        Self {
            ordinal,
            kind: CallableContractClauseKind::Static,
            guard: None,
            value: InterfaceCallableContractClauseValue::TraitSatisfaction {
                subject,
                application,
            },
        }
    }

    /// Attaches an execution-entry guard without making its promise unconditional.
    pub const fn with_guard(mut self, guard: Option<SymbolOrdinal>) -> Self {
        self.guard = guard;

        self
    }

    /// Returns the immediate guard's declaration ordinal, if conditional.
    pub const fn guard(self) -> Option<SymbolOrdinal> {
        self.guard
    }

    /// Returns the clause's stable declaration ordinal.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the clause role.
    pub const fn kind(self) -> CallableContractClauseKind {
        self.kind
    }

    /// Returns the checked clause meaning.
    pub const fn value(self) -> InterfaceCallableContractClauseValue {
        self.value
    }

    /// Returns predicate meaning when this clause contains a predicate expression.
    pub const fn predicate(self) -> Option<InterfacePredicateSummary> {
        match self.value {
            InterfaceCallableContractClauseValue::Predicate(predicate) => Some(predicate),
            InterfaceCallableContractClauseValue::TraitSatisfaction { .. } => None,
        }
    }

    /// Returns the subject and application for a trait-satisfaction constraint.
    pub const fn trait_satisfaction_requirement(
        self,
    ) -> Option<(InterfaceTypeId, InterfaceTraitApplicationId)> {
        match self.value {
            InterfaceCallableContractClauseValue::TraitSatisfaction {
                subject,
                application,
            } => Some((subject, application)),
            InterfaceCallableContractClauseValue::Predicate(_) => None,
        }
    }
}

/// Durable checked behavior for one callable contract phase.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceTrustedCapabilityRequirement {
    pub(crate) ordinal: SymbolOrdinal,
    pub(crate) capability: InterfaceSymbolReference,
}

impl InterfaceTrustedCapabilityRequirement {
    /// Creates one trusted capability requirement in declaration order.
    pub const fn new(ordinal: SymbolOrdinal, capability: InterfaceSymbolReference) -> Self {
        Self {
            ordinal,
            capability,
        }
    }

    /// Returns the requirement's declaration ordinal.
    pub const fn ordinal(&self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the required capability declaration.
    pub const fn capability(&self) -> &InterfaceSymbolReference {
        &self.capability
    }
}

/// Durable checked behavior for one callable contract phase.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallablePhaseBehavior {
    pub(crate) effects: Arc<[InterfaceSymbolReference]>,
    pub(crate) capabilities: Arc<[InterfaceSymbolReference]>,
    pub(crate) trusted_capabilities: Arc<[InterfaceTrustedCapabilityRequirement]>,
    pub(crate) execution_requirements: Arc<[InterfaceSymbolReference]>,
    pub(crate) lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    pub(crate) dependency_contract: InterfaceDependencyContractId,
    pub(crate) current_run_cancellation: CurrentRunCancellation,
}

impl InterfaceCallablePhaseBehavior {
    /// Creates normalized behavior for one invocation or deferred execution phase.
    pub fn new(
        effects: impl IntoIterator<Item = InterfaceSymbolReference>,
        capabilities: impl IntoIterator<Item = InterfaceSymbolReference>,
        trusted_capabilities: impl IntoIterator<Item = InterfaceTrustedCapabilityRequirement>,
        execution_requirements: impl IntoIterator<Item = InterfaceSymbolReference>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: InterfaceDependencyContractId,
        current_run_cancellation: CurrentRunCancellation,
    ) -> Self {
        Self {
            effects: sorted_unique_shared_slice(effects),
            capabilities: sorted_unique_shared_slice(capabilities),
            trusted_capabilities: trusted_capabilities.into_iter().collect(),
            execution_requirements: sorted_unique_shared_slice(execution_requirements),
            lifecycle_obligations: sorted_unique_shared_slice(lifecycle_obligations),
            dependency_contract,
            current_run_cancellation,
        }
    }

    /// Returns checked effects in canonical semantic order.
    pub fn effects(&self) -> &[InterfaceSymbolReference] {
        &self.effects
    }

    /// Returns checked capabilities in canonical semantic order.
    pub fn capabilities(&self) -> &[InterfaceSymbolReference] {
        &self.capabilities
    }

    /// Returns trusted capability requirements in declaration order.
    pub fn trusted_capabilities(&self) -> &[InterfaceTrustedCapabilityRequirement] {
        &self.trusted_capabilities
    }

    /// Returns execution-lane requirements in canonical semantic order.
    pub fn execution_requirements(&self) -> &[InterfaceSymbolReference] {
        &self.execution_requirements
    }

    /// Returns retained lifecycle obligations in canonical semantic order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns the phase's portable dependency contract.
    pub const fn dependency_contract(&self) -> InterfaceDependencyContractId {
        self.dependency_contract
    }

    /// Returns whether this phase can enter current-run cancellation.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }
}

impl bray_symbols::CallableConditionClause for InterfaceCallableContractClause {
    fn kind(&self) -> CallableContractClauseKind {
        self.kind
    }

    fn guard(&self) -> Option<SymbolOrdinal> {
        self.guard
    }
}

/// Checked phase-separated contracts for one callable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableContract {
    pub(crate) owner: InterfaceSymbolReference,
    conditions: bray_symbols::CallableConditionSet<InterfaceCallableContractClause>,
    pub(crate) invocation_behavior: InterfaceCallablePhaseBehavior,
    pub(crate) deferred_execution_behavior: Option<InterfaceCallablePhaseBehavior>,
    pub(crate) evidence:
        Arc<[bray_symbols::CallableContractEvidence<super::InterfaceCallableEvidenceTarget>]>,
}

impl InterfaceCallableContract {
    /// Creates a complete phase-separated callable contract.
    pub fn new(
        owner: InterfaceSymbolReference,
        clauses: impl IntoIterator<Item = InterfaceCallableContractClause>,
        invocation_behavior: InterfaceCallablePhaseBehavior,
        deferred_execution_behavior: Option<InterfaceCallablePhaseBehavior>,
    ) -> Self {
        Self {
            owner,
            conditions: bray_symbols::CallableConditionSet::new(clauses),
            invocation_behavior,
            deferred_execution_behavior,
            evidence: Arc::from([]),
        }
    }

    /// Attaches implementation evidence to the declared contract in promise order.
    pub fn with_evidence(
        mut self,
        evidence: impl IntoIterator<
            Item = bray_symbols::CallableContractEvidence<super::InterfaceCallableEvidenceTarget>,
        >,
    ) -> Self {
        let mut evidence = evidence.into_iter().collect::<Vec<_>>();
        evidence.sort_by_key(bray_symbols::CallableContractEvidence::obligation);
        self.evidence = evidence.into();

        self
    }

    /// Returns supplied promises, their proof authority, and their dependencies.
    pub fn evidence(
        &self,
    ) -> &[bray_symbols::CallableContractEvidence<super::InterfaceCallableEvidenceTarget>] {
        &self.evidence
    }

    /// Returns the callable that owns this contract.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns behavior incurred while invoking the callable.
    pub const fn invocation_behavior(&self) -> &InterfaceCallablePhaseBehavior {
        &self.invocation_behavior
    }

    /// Returns body behavior retained by a future and transferred to a started task.
    pub const fn deferred_execution_behavior(&self) -> Option<&InterfaceCallablePhaseBehavior> {
        self.deferred_execution_behavior.as_ref()
    }
}

impl bray_symbols::CallableConditions for InterfaceCallableContract {
    type Clause = InterfaceCallableContractClause;

    fn conditions(&self) -> &bray_symbols::CallableConditionSet<Self::Clause> {
        &self.conditions
    }

    fn conditions_mut(&mut self) -> &mut bray_symbols::CallableConditionSet<Self::Clause> {
        &mut self.conditions
    }
}

/// Public implementation surface required by downstream selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceImplementationRecord {
    pub(crate) implementation: InterfaceSymbolReference,
    pub(crate) subject: InterfaceTypeId,
    pub(crate) trait_application: Option<InterfaceTraitApplicationId>,
}

impl InterfaceImplementationRecord {
    /// Creates one public implementation record.
    pub const fn new(
        implementation: InterfaceSymbolReference,
        subject: InterfaceTypeId,
        trait_application: Option<InterfaceTraitApplicationId>,
    ) -> Self {
        Self {
            implementation,
            subject,
            trait_application,
        }
    }
}

/// Canonical implementation candidates for one coherence key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCoherenceRecord {
    pub(crate) subject: InterfaceTypeId,
    pub(crate) trait_application: InterfaceTraitApplicationId,
    pub(crate) implementations: Arc<[InterfaceSymbolReference]>,
}

impl InterfaceCoherenceRecord {
    /// Creates one canonical coherence candidate set.
    pub fn new(
        subject: InterfaceTypeId,
        trait_application: InterfaceTraitApplicationId,
        implementations: impl IntoIterator<Item = InterfaceSymbolReference>,
    ) -> Self {
        Self {
            subject,
            trait_application,
            implementations: implementations.into_iter().collect(),
        }
    }
}

/// One target property and exact value required by an exported semantic record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceTargetPropertyDependency {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) property: InterfaceSymbolReference,
    pub(crate) value: InterfaceConstantValueId,
}

impl InterfaceTargetPropertyDependency {
    /// Creates one exact target-property requirement.
    pub const fn new(
        owner: InterfaceSymbolReference,
        property: InterfaceSymbolReference,
        value: InterfaceConstantValueId,
    ) -> Self {
        Self {
            owner,
            property,
            value,
        }
    }

    /// Returns the semantic record that consumes this requirement.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the required target-property declaration.
    pub const fn property(&self) -> &InterfaceSymbolReference {
        &self.property
    }

    /// Returns the required canonical value.
    pub const fn value(&self) -> InterfaceConstantValueId {
        self.value
    }
}

/// One callable ABI requirement exposed through the interface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceAbiDependency {
    pub(crate) symbol: InterfaceSymbolReference,
    pub(crate) abi: CallableAbi,
}

/// Portable runtime requirements published by one exported callable or support entity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceRuntimeRequirement {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) frames: Arc<[ProtectedAsyncFrameId]>,
    pub(crate) requirements: RuntimeRequirements,
}

impl InterfaceRuntimeRequirement {
    /// Creates one owner-correlated portable runtime requirement.
    ///
    /// Interface validation rejects exact runtime identities and private ABI roles.
    pub fn new(
        owner: InterfaceSymbolReference,
        frames: impl IntoIterator<Item = ProtectedAsyncFrameId>,
        requirements: RuntimeRequirements,
    ) -> Self {
        Self {
            owner,
            frames: sorted_unique_shared_slice(frames),
            requirements,
        }
    }

    /// Returns the exported semantic owner.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the hidden protected-frame identities in canonical order.
    pub fn frames(&self) -> &[ProtectedAsyncFrameId] {
        &self.frames
    }

    /// Returns target-specific portable runtime compatibility requirements.
    pub const fn requirements(&self) -> &RuntimeRequirements {
        &self.requirements
    }
}

impl InterfaceAbiDependency {
    /// Creates one ABI dependency.
    pub const fn new(symbol: InterfaceSymbolReference, abi: CallableAbi) -> Self {
        Self { symbol, abi }
    }
}

/// Optional non-semantic source correlation for one interface symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceSourceProvenance {
    pub(crate) symbol: InterfaceSymbolReference,
    pub(crate) document: NonEmptySharedStr,
    pub(crate) start: u32,
    pub(crate) end: u32,
}

/// One stable record-directory entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceSemanticRecord {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) kind: InterfaceSemanticRecordKind,
    pub(crate) section: crate::InterfaceSectionTag,
    pub(crate) record: u32,
}

impl InterfaceSemanticRecord {
    /// Returns the exact symbol that owns this record.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the stable semantic record category.
    pub const fn kind(&self) -> InterfaceSemanticRecordKind {
        self.kind
    }

    /// Returns the section containing the record payload.
    pub const fn section(&self) -> crate::InterfaceSectionTag {
        self.section
    }

    /// Returns the record index within the category table.
    pub const fn record(&self) -> u32 {
        self.record
    }
}

impl InterfaceSourceProvenance {
    /// Creates optional source correlation when the range is ordered.
    pub fn try_new(
        symbol: InterfaceSymbolReference,
        document: impl Into<Arc<str>>,
        start: u32,
        end: u32,
    ) -> Option<Self> {
        let document = NonEmptySharedStr::try_new(document)?;

        if start > end {
            return None;
        }

        Some(Self {
            symbol,
            document,
            start,
            end,
        })
    }
}
