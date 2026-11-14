use super::evidence::ExecutionCallEvidence;
use bray_bound_tree::{AnyBoundNodeId, BoundCallableTarget};
use bray_source::SourceSpan;

use bray_symbols::ExecutionProperty;

/// Certification failure retaining the source identity and specific known cause.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionVerificationFailure<Source> {
    /// Source of the failed operation or dependency.
    pub source: Source,
    /// Why the requested behavior could not be established.
    pub cause: bray_diagnostics::DiagnosticExecutionProofFailure,
    /// The unproven contract with its actual values, resolved only on failure.
    pub condition: Option<std::sync::Arc<bray_diagnostics::DiagnosticContractObligation>>,
}

impl<Source> ExecutionVerificationFailure<Source> {
    /// Changes the source representation without discarding the proof's diagnostic context.
    pub fn map_source<T>(self, map: impl FnOnce(Source) -> T) -> ExecutionVerificationFailure<T> {
        ExecutionVerificationFailure { source: map(self.source), cause: self.cause, condition: self.condition }
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

/// An execution dependency retains the callable or declaration-owned default it invokes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionDependencyTarget {
    /// A selected callable implementation.
    Callable(BoundCallableTarget),
    /// A selected abstract method retains the exact enclosing witness route.
    Requirement {
        /// Abstract declaration and its checked generic application.
        callable: bray_symbols::CallableInstanceData,
        /// Existing constraint or trait-default witness route.
        dispatch: bray_symbols::TraitConstraintDispatch,
    },
    /// An omitted argument or construction field's provider.
    Default(bray_symbols::DefaultValueProvider),
}

impl ExecutionDependencyTarget {
    pub(crate) fn selected(call: &bray_bound_tree::SelectedCall) -> Self {
        let target = call
            .resolution()
            .declaration_source()
            .map(BoundCallableTarget::Declaration)
            .unwrap_or(call.target());

        if let Some(dispatch) = call.resolution().trait_dispatch() {
            let BoundCallableTarget::Declaration(callable) = target else {
                panic!("selected abstract invocation retains its declaration");
            };

            Self::Requirement { callable, dispatch }
        } else {
            Self::Callable(target)
        }
    }

    /// Returns the selected callable independently of its witness route.
    pub const fn callable(self) -> Option<BoundCallableTarget> {
        match self {
            Self::Callable(target) => Some(target),
            Self::Requirement { callable, .. } => Some(BoundCallableTarget::Declaration(callable)),
            Self::Default(_) => None,
        }
    }
}

impl From<BoundCallableTarget> for ExecutionDependencyTarget {
    fn from(target: BoundCallableTarget) -> Self {
        Self::Callable(target)
    }
}

/// A selected implementation and the exact property required from it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionDependency {
    /// The selected callable or default provider.
    pub target: ExecutionDependencyTarget,
    /// The independently required checked behavior.
    pub obligation: ExecutionObligation,
    /// The exact source operation or implicit invocation that selected this dependency.
    pub occurrence: bray_bound_tree::BoundExecutionSite,
}

/// Locally checked behavior whose dependencies still require certification.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCandidate {
    pub(crate) inert_cleanup:
        std::collections::BTreeMap<(AnyBoundNodeId, bray_bound_tree::StorageAccessId), bool>,
    pub(crate) disposal: std::collections::BTreeMap<
        (
            bray_bound_tree::BoundExecutionSite,
            bray_symbols::CallableInstanceData,
        ),
        ExecutionCallEvidence,
    >,
    pub(crate) cleanup: std::collections::BTreeMap<
        (
            AnyBoundNodeId,
            bray_bound_tree::StorageAccessId,
            usize,
            bray_symbols::TypeAssociatedLifecycleSlot,
            bool,
        ),
        ExecutionCleanup,
    >,
    pub(crate) result_variant: Option<bray_symbols::UnionVariantSymbolId>,
    pub(crate) completion_dependencies: Vec<ExecutionCompletionDependency>,
    pub(crate) calls:
        std::collections::BTreeMap<bray_bound_tree::BoundExecutionSite, ExecutionCallEvidence>,
    pub(crate) failure: Option<ExecutionVerificationFailure<SourceSpan>>,
    pub(crate) constant_failure: Option<SourceSpan>,
    pub(crate) observation_failure: Option<SourceSpan>,
    pub(crate) finite_iteration: bool,
    pub(crate) input_mutation: Option<SourceSpan>,
    pub(crate) integer_maxima: std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, u64>,
    pub(crate) reachable_nodes: std::collections::BTreeSet<AnyBoundNodeId>,
    pub(crate) dependencies: Vec<ExecutionDependency>,
}

impl ExecutionCandidate {
    /// Returns the receiver evidence for an implicit lifecycle invocation, or the explicit call evidence.
    pub fn dependency_evidence(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        target: BoundCallableTarget,
    ) -> Option<&ExecutionCallEvidence> {
        if let BoundCallableTarget::Declaration(callable) = target
            && let Some(evidence) = self.disposal.get(&(occurrence, callable))
        {
            return Some(evidence);
        }

        self.call_evidence(occurrence)
    }

    /// Returns selected lifecycle actions with their receiver facts. The key retains the
    /// exit node, owner, selected part and whether this is a normal ownership end.
    /// Each entry still requires execution and completion-dependency certification.
    pub fn cleanup_candidates(
        &self,
    ) -> impl Iterator<
        Item = (
            (
                AnyBoundNodeId,
                bray_bound_tree::StorageAccessId,
                usize,
                bray_symbols::TypeAssociatedLifecycleSlot,
                bool,
            ),
            &ExecutionCleanup,
        ),
    > {
        self.cleanup.iter().map(|(site, cleanup)| (*site, cleanup))
    }

    /// Returns the union variant constructed on every reachable normal exit in this domain.
    pub const fn result_variant(&self) -> Option<bray_symbols::UnionVariantSymbolId> {
        self.result_variant
    }

    /// Returns completion predicates whose proof is required by this candidate.
    pub fn completion_dependencies(&self) -> &[ExecutionCompletionDependency] {
        &self.completion_dependencies
    }

    /// Returns the actual invocation occurrences retained by this analysis.
    pub fn invocations(&self) -> impl Iterator<Item = bray_bound_tree::BoundExecutionSite> + '_ {
        self.calls.keys().copied()
    }

    /// Returns captured entry evidence for a selected call operation.
    pub fn call_evidence(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> Option<&ExecutionCallEvidence> {
        self.calls.get(&occurrence)
    }

    /// Returns the first operation preventing a local proof, if any.
    pub fn failure(&self) -> Option<ExecutionVerificationFailure<SourceSpan>> {
        // Failure context is immutable and shared with the candidate's certificate consumers.
        match &self.failure {
            Some(failure) => Some(failure.clone()),
            None => match self.input_mutation {
                Some(source) => Some(ExecutionVerificationFailure { condition: None,
                    source, cause: bray_diagnostics::DiagnosticExecutionProofFailure::InputMutation,
                }),
                None => None,
            },
        }
    }

    /// Returns an operation that escapes the supported deterministic observation frame.
    pub const fn observation_failure(&self) -> Option<SourceSpan> {
        match &self.failure {
            Some(failure) => Some(failure.source),
            None => self.observation_failure,
        }
    }

    /// Whether every successful step has a checked finite progression proof.
    pub const fn finite_iteration(&self) -> bool {
        self.finite_iteration
    }

    /// Returns the first reachable operation forbidden in constant evaluation.
    pub const fn constant_failure(&self) -> Option<SourceSpan> {
        self.constant_failure
    }

    /// Returns nonnegative integer upper bounds established at their actual expression occurrences.
    pub fn integer_maxima(
        &self,
    ) -> &std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, u64> {
        &self.integer_maxima
    }

    /// Whether this node executes in the checked constant-evaluation entry domain.
    pub fn is_reachable(&self, node: AnyBoundNodeId) -> bool {
        self.reachable_nodes.contains(&node)
    }

    /// Returns selected dependencies and the operations that require them.
    pub fn dependencies(&self) -> &[ExecutionDependency] {
        &self.dependencies
    }
}

/// Selected lifecycle execution and receiver evidence at one implicit cleanup boundary.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionCleanup {
    /// The selected, substituted lifecycle callable.
    pub callable: bray_symbols::CallableInstanceData,
    /// Its completion result, before asynchronous wrapping.
    pub result: bray_symbols::TypeId,
    /// Whether invocation can complete without driving a future.
    pub synchronous: bool,
    /// Whether the implicit receiver tail executes in the consuming caller.
    pub caller_continuation: bool,
    /// Whether this action finalizes the complete owner rather than a represented part.
    pub whole: bool,
    /// Facts available immediately before this invocation.
    pub evidence: ExecutionCallEvidence,
}

/// Certified properties and the opaque assertions required by their proofs.
/// Foreign assertions retain their source identity and never represent checked Bray bodies.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCertification {
    /// Selected obligations used by successful proofs, retaining source and generic identities.
    pub dependencies: std::collections::BTreeSet<(
        bray_declarations::SyntaxAnchor,
        ExecutionObligation,
        ExecutionDependencyTarget,
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

/// Local execution candidates keyed by property and guarded declaration, when present.
pub type ExecutionCandidates =
    std::collections::BTreeMap<ExecutionObligation, std::sync::Arc<ExecutionCandidate>>;

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

#[cfg(test)]
mod tests {
    use super::ExecutionCallEvidence;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::StorageProjection;
    use bray_symbols::SymbolOrdinal;
    use std::sync::Arc;

    #[test]
    fn input_join_loses_contents_without_losing_the_common_owner() {
        let ordinal = SymbolOrdinal::new(0);
        let input = ExecutionPlace::argument(ordinal);
        let owner = ExecutionCondition::Input(input.clone());
        let field = StorageProjection::TupleElement(ordinal);

        let snapshot = |value| ExecutionCondition::constructed(owner.clone(), [(field, ExecutionCondition::Boolean(value))].into(),
        );

        for borrowed in [false, true] {
            let value = |value| if borrowed { snapshot(value).borrowed() } else { snapshot(value) };

            let mut entry = ExecutionCallEvidence {
                arguments: [(input.clone(), value(true))].into(),
                owned_inputs: if borrowed { Arc::from([]) } else { Arc::from([ordinal]) },
                ..ExecutionCallEvidence::default()
            };

            let incoming = ExecutionCallEvidence {
                arguments: [(input.clone(), value(false))].into(),
                owned_inputs: Arc::clone(&entry.owned_inputs),
                ..ExecutionCallEvidence::default()
            };

            assert!(entry.intersect(&incoming, &Default::default()));
            assert_eq!(entry.arguments[&input].owner_identity(), value(true).owner_identity());

            let component = entry.arguments[&input].clone().into_referent().project(field);

            assert_eq!(component.prove(&entry.assumptions, &mut { ExecutionCondition::WORK_LIMIT }), None);
            assert_eq!(entry.owned_arguments().count(), usize::from(!borrowed));
            assert!(!entry.intersect(&incoming, &Default::default()));
        }
    }
}
