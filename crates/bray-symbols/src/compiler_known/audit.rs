use bray_compiler_known::{
    AvailabilityRule, CompilerKnownDeclarationId, CompilerKnownScopeId, CompilerKnownValueId,
    ImplementationHook, RepresentationRole,
};

use crate::{AnySymbolId, SymbolGraph};

/// One target-availability profile exercised by catalog validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilerKnownTargetProfile {
    /// Only universally available catalog entries participate.
    Portable,
    /// Every closed target capability is available.
    Complete,
    /// One optional capability and universally available entries participate.
    Capability(AvailabilityRule),
}

/// Deterministic summary of one complete compiler-known catalog audit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompilerKnownCatalogAuditReport {
    pub(super) scopes: usize,
    pub(super) declarations: usize,
    pub(super) values: usize,
    pub(super) representation_roles: usize,
    pub(super) implementation_roles: usize,
    pub(super) completion_units: usize,
    pub(super) completion_facts: usize,
    pub(super) target_profiles: usize,
}

impl CompilerKnownCatalogAuditReport {
    /// Returns the number of stable compiler-known scopes audited.
    pub const fn scopes(self) -> usize {
        self.scopes
    }

    /// Returns the number of compiler-known declaration identities audited.
    pub const fn declarations(self) -> usize {
        self.declarations
    }

    /// Returns the number of compiler-known special values audited.
    pub const fn values(self) -> usize {
        self.values
    }

    /// Returns the number of representation-role bindings audited.
    pub const fn representation_roles(self) -> usize {
        self.representation_roles
    }

    /// Returns the number of implementation-role bindings audited.
    pub const fn implementation_roles(self) -> usize {
        self.implementation_roles
    }

    /// Returns the number of symbol units reached by completion.
    pub const fn completion_units(self) -> usize {
        self.completion_units
    }

    /// Returns the number of semantic fact requests forced by completion.
    pub const fn completion_facts(self) -> usize {
        self.completion_facts
    }

    /// Returns the number of target profiles audited.
    pub const fn target_profiles(self) -> usize {
        self.target_profiles
    }
}

/// A validated compiler-known catalog audit and semantic-fact forcer.
pub struct CompilerKnownCatalogAudit<'graph> {
    pub(super) graph: &'graph SymbolGraph,
    pub(super) report: CompilerKnownCatalogAuditReport,
}

/// A violated invariant in generated compiler-known semantic catalog data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilerKnownCatalogAuditError {
    /// The stable scope index has the wrong cardinality.
    StableScopeCount,
    /// The stable declaration index has the wrong cardinality.
    StableDeclarationCount,
    /// One scope is missing or inconsistent.
    InvalidScope(CompilerKnownScopeId),
    /// One declaration is missing or inconsistent.
    InvalidDeclaration(CompilerKnownDeclarationId),
    /// One declaration has the wrong immediate semantic owner.
    InvalidOwner(CompilerKnownDeclarationId),
    /// One generated representation role does not resolve to its catalog target.
    InvalidRepresentationRole(RepresentationRole),
    /// One generated implementation hook does not resolve to its catalog declarations.
    InvalidImplementationRole(ImplementationHook),
    /// One target view has the wrong declaration availability.
    InvalidTargetDeclaration {
        /// The audited target profile.
        profile: CompilerKnownTargetProfile,
        /// The declaration whose availability is inconsistent.
        declaration: CompilerKnownDeclarationId,
    },
    /// One target view has the wrong special-value availability.
    InvalidTargetValue {
        /// The audited target profile.
        profile: CompilerKnownTargetProfile,
        /// The special value whose availability is inconsistent.
        value: CompilerKnownValueId,
    },
    /// One target view has the wrong representation-role availability.
    InvalidTargetRepresentationRole {
        /// The audited target profile.
        profile: CompilerKnownTargetProfile,
        /// The role whose target is inconsistent.
        role: RepresentationRole,
    },
    /// One target view has the wrong implementation-role availability.
    InvalidTargetImplementationRole {
        /// The audited target profile.
        profile: CompilerKnownTargetProfile,
        /// The hook whose targets are inconsistent.
        hook: ImplementationHook,
    },
    /// Compiler-known completion did not reach every declaration exactly once.
    IncompleteCompletion,
    /// The compiler-known completion plan could not be constructed.
    InvalidCompletionPlan,
    /// One planned semantic fact does not resolve to generated declaration data.
    InvalidCompletionFact {
        /// The symbol owning the invalid request.
        symbol: AnySymbolId,
    },
}

impl std::fmt::Display for CompilerKnownCatalogAuditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CompilerKnownCatalogAuditError {}
