use bray_bound_tree::{AnyBoundNodeId, BoundCallableTarget};
use bray_source::SourceSpan;

use bray_symbols::ExecutionProperty;

/// A declaration creates an obligation without certifying its implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeclaredExecutionProperty {
    /// The promised behavior.
    pub property: ExecutionProperty,
    /// The property name in the declaration.
    pub source: SourceSpan,
}

/// Execution obligations and their source clauses, before implementation checking.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionDeclaration {
    pub(crate) domains: Vec<ExecutionDomain>,
    pub(crate) requirements: Vec<bray_declarations::SyntaxAnchor>,
    pub(crate) clauses: Vec<bray_declarations::SyntaxAnchor>,
}

impl ExecutionDeclaration {
    /// Returns all independent entry domains, including the unguarded domain.
    pub fn domains(&self) -> &[ExecutionDomain] {
        &self.domains
    }

    /// Returns the common invocation requirements.
    pub fn requirements(&self) -> &[bray_declarations::SyntaxAnchor] {
        &self.requirements
    }

    /// Returns the independent properties declared on this callable.
    pub fn properties(&self) -> &[DeclaredExecutionProperty] {
        self.domains
            .iter()
            .find(|domain| domain.guards.is_empty())
            .map_or(&[], |domain| domain.properties.as_slice())
    }

    /// Whether invocations need an entry-precondition proof beyond this unconditional certificate.
    pub const fn has_requirements(&self) -> bool {
        !self.requirements.is_empty()
    }

    /// Returns the exact execution clauses covered by these obligations.
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
    pub(crate) completion_dependencies: Vec<ExecutionCompletionDependency>,
    pub(crate) calls: std::collections::BTreeMap<AnyBoundNodeId, ExecutionCallEvidence>,
    pub(crate) failure: Option<SourceSpan>,
    pub(crate) dependencies: Vec<ExecutionDependency>,
}

impl ExecutionCandidate {
    /// Returns completion predicates whose proof is required by this candidate.
    pub fn completion_dependencies(&self) -> &[ExecutionCompletionDependency] {
        &self.completion_dependencies
    }

    /// Returns captured entry evidence for a selected call operation.
    pub fn call_evidence(&self, node: AnyBoundNodeId) -> Option<&ExecutionCallEvidence> {
        self.calls.get(&node)
    }

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
    /// Selected obligations used by successful proofs, retaining source and generic identities.
    pub dependencies: std::collections::BTreeSet<(
        bray_declarations::SyntaxAnchor,
        ExecutionObligation,
        BoundCallableTarget,
        ExecutionObligation,
    )>,
    /// Certified properties restricted to their declared entry domains.
    pub guarded_properties: std::collections::BTreeSet<(SourceSpan, ExecutionProperty)>,
    /// Certified predicates on normal completion.
    pub postconditions: std::collections::BTreeSet<SourceSpan>,
    /// Properties whose complete dependency graphs passed checking.
    pub properties: std::collections::BTreeSet<ExecutionProperty>,
    /// Foreign declarations used by each successful property proof.
    pub foreign_assertions:
        std::collections::BTreeSet<(bray_declarations::SyntaxAnchor, ExecutionProperty)>,
}

/// Guarantees that apply together under a conjunction of execution-entry guards.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionDomain {
    /// Entry guards, including enclosing groups.
    pub guards: Vec<bray_declarations::SyntaxAnchor>,
    /// Independent execution promises in this group.
    pub properties: Vec<DeclaredExecutionProperty>,
    /// Normal-completion predicates in this group.
    pub postconditions: Vec<bray_declarations::SyntaxAnchor>,
}

/// Values captured at a selected call's entry, before callee mutation can occur.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCallEvidence {
    pub(crate) arguments:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    pub(crate) assumptions: std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
}

impl ExecutionCallEvidence {
    /// Whether every callee-entry condition follows from the captured argument values.
    pub fn proves(&self, conditions: &[super::ExecutionCondition]) -> bool {
        conditions.iter().all(|condition| {
            let condition = condition.substitute(
                &|input| {
                    input
                        .value_in(&self.arguments)
                        .unwrap_or(super::ExecutionCondition::Unknown)
                },
                &super::ExecutionCondition::Unknown,
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            );

            condition.prove(&self.assumptions, &mut {
                crate::ExecutionCondition::WORK_LIMIT
            }) == Some(true)
        })
    }
}

/// Local execution candidates keyed by property and guarded declaration, when present.
pub type ExecutionCandidates = std::collections::BTreeMap<ExecutionObligation, ExecutionCandidate>;

/// The exact declared promise required by a proof node.
pub type ExecutionObligation = bray_symbols::CallableExecutionObligation<super::ExecutionClauseId>;

/// An entry domain and its declared normal-completion predicates at one selected call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionCompletionContract {
    /// Required conditions, including the group's entry guards.
    pub entry: Vec<super::ExecutionCondition>,
    /// Predicates to establish from the actual callee body before callers may use them.
    pub postconditions: Vec<(super::ExecutionCondition, super::ExecutionClauseId)>,
}

/// A selected normal-completion predicate whose implementation still needs checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionCompletionDependency {
    /// The selected callable.
    pub target: BoundCallableTarget,
    /// The exact normal-completion clause.
    pub source: super::ExecutionClauseId,
    /// The calling expression.
    pub node: AnyBoundNodeId,
}
