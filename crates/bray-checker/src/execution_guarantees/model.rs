use bray_bound_tree::{AnyBoundNodeId, BoundCallableTarget};
use bray_source::SourceSpan;

/// An independent property of ordinary callable execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionProperty {
    /// Execution has no runtime effects, including cleanup effects.
    Pure,
    /// Execution and cleanup terminate normally on valid inputs.
    Total,
}

impl ExecutionProperty {
    /// Resolves a language execution property name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pure" => Some(Self::Pure),
            "total" => Some(Self::Total),
            _ => None,
        }
    }

    /// Returns the property's source spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Total => "total",
        }
    }
}

/// A declaration creates an obligation without certifying its implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeclaredExecutionProperty {
    /// The promised behavior.
    pub property: ExecutionProperty,
    /// The property name in the declaration.
    pub source: SourceSpan,
}

/// Unconditional obligations and their source clauses, before implementation checking.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionDeclaration {
    pub(crate) properties: Vec<DeclaredExecutionProperty>,
    pub(crate) has_requirements: bool,
    pub(crate) clauses: Vec<bray_declarations::SyntaxAnchor>,
}

impl ExecutionDeclaration {
    /// Returns the independent properties declared on this callable.
    pub fn properties(&self) -> &[DeclaredExecutionProperty] {
        &self.properties
    }

    /// Whether invocations need an entry-precondition proof beyond this unconditional certificate.
    pub const fn has_requirements(&self) -> bool {
        self.has_requirements
    }

    /// Returns the exact unconditional clauses covered by these obligations.
    pub fn clauses(&self) -> &[bray_declarations::SyntaxAnchor] {
        &self.clauses
    }
}

/// A selected implementation and the exact property required from it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionDependency {
    /// The statically or dynamically selected callable.
    pub target: BoundCallableTarget,
    /// The independently required execution property.
    pub property: ExecutionProperty,
    /// The source operation that selected this dependency.
    pub node: AnyBoundNodeId,
}

/// Locally checked behavior whose dependencies still require certification.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCandidate {
    pub(crate) failure: Option<SourceSpan>,
    pub(crate) dependencies: Vec<ExecutionDependency>,
}

impl ExecutionCandidate {
    /// Returns the first operation preventing a local proof, if any.
    pub const fn failure(&self) -> Option<SourceSpan> {
        self.failure
    }

    /// Returns selected dependencies and the operations that require them.
    pub fn dependencies(&self) -> &[ExecutionDependency] {
        &self.dependencies
    }
}

/// Certified properties and the opaque assertions required by their proofs.
/// Foreign assertions retain their source identity and never represent checked Bray bodies.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCertification {
    /// Properties whose complete dependency graphs passed checking.
    pub properties: std::collections::BTreeSet<ExecutionProperty>,
    /// Foreign declarations used by each successful property proof.
    pub foreign_assertions:
        std::collections::BTreeSet<(bray_declarations::SyntaxAnchor, ExecutionProperty)>,
}
