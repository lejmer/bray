use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{CallableAbi, CallableContractClauseKind, SymbolOrdinal};

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
}

/// Checked contracts and trusted capabilities for one callable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableContract {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) clauses: Arc<[InterfaceCallableContractClause]>,
    pub(crate) trusted_capabilities: Arc<[InterfaceSymbolReference]>,
    pub(crate) dependency_contract: InterfaceDependencyContractId,
}

impl InterfaceCallableContract {
    /// Creates a complete callable contract surface.
    pub fn new(
        owner: InterfaceSymbolReference,
        clauses: impl IntoIterator<Item = InterfaceCallableContractClause>,
        trusted_capabilities: impl IntoIterator<Item = InterfaceSymbolReference>,
        dependency_contract: InterfaceDependencyContractId,
    ) -> Self {
        Self {
            owner,
            clauses: clauses.into_iter().collect(),
            trusted_capabilities: trusted_capabilities.into_iter().collect(),
            dependency_contract,
        }
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
    pub(crate) fact: InterfaceSymbolReference,
    pub(crate) value: InterfaceConstantValueId,
}

impl InterfaceTargetFactDependency {
    /// Creates one exact target-fact requirement.
    pub const fn new(fact: InterfaceSymbolReference, value: InterfaceConstantValueId) -> Self {
        Self { fact, value }
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

/// Symbol-owned semantic fact category addressable through the lazy fact directory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSemanticFactKind {
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

/// One stable lazy fact-directory entry.
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
