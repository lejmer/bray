use std::sync::Arc;

use bray_base::{NonEmptySharedStr, sorted_unique_shared_slice};
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
}

impl InterfacePredicateSummary {
    /// Creates one predicate summary.
    pub const fn new(dependency_contract: InterfaceDependencyContractId) -> Self {
        Self {
            dependency_contract,
        }
    }

    /// Returns the predicate expression's portable dependency contract.
    pub const fn dependency_contract(self) -> InterfaceDependencyContractId {
        self.dependency_contract
    }
}

/// One generic constraint attached to an exported declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceConstraint {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) ordinal: SymbolOrdinal,
    pub(crate) predicate: InterfacePredicateSummary,
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
            predicate,
        }
    }
}

/// One checked callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableContractClause {
    pub(crate) ordinal: SymbolOrdinal,
    pub(crate) kind: CallableContractClauseKind,
    pub(crate) predicate: InterfacePredicateSummary,
}

impl InterfaceCallableContractClause {
    /// Creates one checked callable contract clause.
    pub const fn new(
        ordinal: SymbolOrdinal,
        kind: CallableContractClauseKind,
        predicate: InterfacePredicateSummary,
    ) -> Self {
        Self {
            ordinal,
            kind,
            predicate,
        }
    }

    /// Returns the clause's stable declaration ordinal.
    pub const fn ordinal(self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the clause role.
    pub const fn kind(self) -> CallableContractClauseKind {
        self.kind
    }

    /// Returns the checked predicate meaning.
    pub const fn predicate(self) -> InterfacePredicateSummary {
        self.predicate
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

/// Checked phase-separated contracts for one callable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableContract {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) invocation_preconditions: Arc<[InterfaceCallableContractClause]>,
    pub(crate) static_constraints: Arc<[InterfaceCallableContractClause]>,
    pub(crate) normal_completion_postconditions: Arc<[InterfaceCallableContractClause]>,
    pub(crate) invocation_behavior: InterfaceCallablePhaseBehavior,
    pub(crate) deferred_execution_behavior: Option<InterfaceCallablePhaseBehavior>,
}

impl InterfaceCallableContract {
    /// Creates a complete phase-separated callable contract surface.
    pub fn new(
        owner: InterfaceSymbolReference,
        clauses: impl IntoIterator<Item = InterfaceCallableContractClause>,
        invocation_behavior: InterfaceCallablePhaseBehavior,
        deferred_execution_behavior: Option<InterfaceCallablePhaseBehavior>,
    ) -> Self {
        let mut invocation_preconditions = Vec::new();
        let mut static_constraints = Vec::new();
        let mut normal_completion_postconditions = Vec::new();

        for clause in clauses {
            match clause.kind {
                CallableContractClauseKind::Requires => invocation_preconditions.push(clause),
                CallableContractClauseKind::Ensures => {
                    normal_completion_postconditions.push(clause);
                }
                CallableContractClauseKind::Static => static_constraints.push(clause),
            }
        }

        Self {
            owner,
            invocation_preconditions: invocation_preconditions.into(),
            static_constraints: static_constraints.into(),
            normal_completion_postconditions: normal_completion_postconditions.into(),
            invocation_behavior,
            deferred_execution_behavior,
        }
    }

    /// Returns the callable that owns this contract.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns preconditions checked before invocation.
    pub fn invocation_preconditions(&self) -> &[InterfaceCallableContractClause] {
        &self.invocation_preconditions
    }

    /// Returns constraints checked during selection or instantiation.
    pub fn static_constraints(&self) -> &[InterfaceCallableContractClause] {
        &self.static_constraints
    }

    /// Returns postconditions published only after normal completion.
    pub fn normal_completion_postconditions(&self) -> &[InterfaceCallableContractClause] {
        &self.normal_completion_postconditions
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

/// One target fact and exact value required by an exported semantic fact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceTargetFactDependency {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) fact: InterfaceSymbolReference,
    pub(crate) value: InterfaceConstantValueId,
}

impl InterfaceTargetFactDependency {
    /// Creates one exact target-fact requirement.
    pub const fn new(
        owner: InterfaceSymbolReference,
        fact: InterfaceSymbolReference,
        value: InterfaceConstantValueId,
    ) -> Self {
        Self { owner, fact, value }
    }

    /// Returns the semantic fact that consumes this requirement.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the required target-fact declaration.
    pub const fn fact(&self) -> &InterfaceSymbolReference {
        &self.fact
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

/// Symbol-owned semantic fact category addressable through the fact directory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSemanticFactKind {
    /// Complete source-independent callable signature template.
    CallableSignature,
    /// Ordered generic declaration template.
    GenericDeclaration,
    /// Callable parameter default-template presence.
    CallableParameterDefault,
    /// Validated predicate definition form.
    PredicateDefinition,
    /// Checked generic constraint.
    GenericConstraint,
    /// Complete callable contract set.
    CallableContracts,
    /// Source-independent checked declaration-owned template.
    DeclarationTemplate,
    /// Public implementation subject and applied trait.
    Implementation,
    /// Required target fact value.
    TargetFact,
    /// Required callable ABI.
    Abi,
}

/// One stable fact-directory entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceSemanticFactEntry {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) kind: InterfaceSemanticFactKind,
    pub(crate) section: crate::InterfaceSectionTag,
    pub(crate) record: u32,
}

impl InterfaceSemanticFactEntry {
    /// Returns the exact symbol that owns this fact.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the stable semantic fact category.
    pub const fn kind(&self) -> InterfaceSemanticFactKind {
        self.kind
    }

    /// Returns the section containing the fact payload.
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
