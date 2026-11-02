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
    pub(crate) failure: Option<SourceSpan>,
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
    pub const fn failure(&self) -> Option<SourceSpan> {
        match self.failure {
            Some(failure) => Some(failure),
            None => self.input_mutation,
        }
    }

    /// Returns an operation that escapes the supported deterministic observation frame.
    pub const fn observation_failure(&self) -> Option<SourceSpan> {
        match self.failure {
            Some(failure) => Some(failure),
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

/// Values captured at a selected call's entry, before callee mutation can occur.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ExecutionCallEvidence {
    pub(crate) implementation_witnesses: std::sync::Arc<[bray_symbols::ImplementationInstanceId]>,
    /// Caller-owned carriers admitted at invocation. Valid clauses remain in the
    /// live state; these identities prevent another carrier from lending provenance.
    pub(crate) witness_inputs: std::collections::BTreeSet<super::ExecutionCondition>,
    /// Copy origins while their observations remain unchanged. These are value
    /// correlations, never language equality predicates or ownership authority.
    pub(crate) copy_sources:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    /// Noncopying owned inputs, in receiver-first checked signature order.
    pub(crate) owned_inputs: std::sync::Arc<[bray_symbols::SymbolOrdinal]>,
    /// Access dependencies captured before consuming inputs, without owning authority.
    pub(crate) addresses:
        std::collections::BTreeMap<super::ExecutionPlace, super::AddressDependencies>,
    pub(crate) anonymous_target: Option<bray_bound_tree::BoundUnitKey>,
    pub(crate) trusted_boundary: bool,
    pub(crate) pending_execution: bool,
    pub(crate) arguments:
        std::collections::BTreeMap<super::ExecutionPlace, super::ExecutionCondition>,
    pub(crate) assumptions: std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
    pub(crate) trusted_assumptions: std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
}

impl ExecutionCallEvidence {
    /// Actual values moved into the receiver-first noncopying input slots.
    pub(crate) fn owned_arguments(&self) -> impl Iterator<Item = super::ExecutionCondition> + '_ {
        self.owned_inputs.iter().filter_map(|ordinal| {
            super::ExecutionPlace::argument(*ordinal).value_in(&self.arguments)
        })
    }

    /// Project current ordinary knowledge into immutable copied input identities.
    pub(crate) fn captured_facts(
        &self,
        ordinary: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
    ) -> std::collections::BTreeSet<(super::ExecutionCondition, bool)> {
        use super::ExecutionCondition as Condition;

        let mut bindings = std::collections::BTreeMap::new();
        let mut facts = std::collections::BTreeSet::new();

        for (place, source) in &self.copy_sources {
            let Some(captured) = place.value_in(&self.arguments) else {
                continue;
            };

            bindings.insert(source.clone(), captured.clone());

            bindings.insert(
                source.owner_identity().clone(),
                captured.owner_identity().clone(),
            );
        }

        if bindings.is_empty() {
            return facts;
        }

        for (condition, holds) in ordinary {
            if condition.contains_trusted() || condition.observes_live(None) {
                continue;
            }

            let captured = condition.with_snapshots(&bindings);

            if captured != *condition {
                captured.assume(*holds, &mut facts);
            }
        }

        for (source, captured) in bindings {
            if let Some(holds) = source.prove(ordinary, &mut { Condition::WORK_LIMIT }) {
                captured.assume(holds, &mut facts);
            }
        }

        facts
    }

    /// Captures only constant argument values, with no trusted authority or runtime execution lane.
    pub fn from_constants(
        values: &bray_symbols::SemanticValueStore,
        arguments: &[bray_symbols::ConstantValueId],
    ) -> Self {
        Self {
            arguments: arguments
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        super::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                            u32::try_from(index)
                                .expect("constant call argument index fits its signature"),
                        )),
                        super::ExecutionCondition::from_constant(values, *value),
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    /// Retains the implementation choices selected for this invocation.
    pub fn with_implementation_witnesses(
        mut self,
        witnesses: impl IntoIterator<Item = bray_symbols::ImplementationInstanceId>,
    ) -> Self {
        self.implementation_witnesses = witnesses.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Returns the selected invocation's implementation choices.
    pub fn implementation_witnesses(&self) -> &[bray_symbols::ImplementationInstanceId] {
        &self.implementation_witnesses
    }

    /// Instantiates call identities in captured values without changing the reusable entry snapshot.
    pub fn try_map_conditions<E>(
        &self,
        mut map: impl FnMut(&super::ExecutionCondition) -> Result<super::ExecutionCondition, E>,
    ) -> Result<Self, E> {
        let mut mapped = Self {
            implementation_witnesses: std::sync::Arc::clone(&self.implementation_witnesses),
            // Proof queries own their snapshot while analysis retains the original entry evidence.
            returned: self.returned.clone(),
            anonymous_target: self.anonymous_target.clone(),
            trusted_boundary: self.trusted_boundary,
            pending_execution: self.pending_execution,
            ..Self::default()
        };

        mapped.arguments = self
            .arguments
            .iter()
            .map(|(place, condition)| Ok((place.clone(), map(condition)?)))
            .collect::<Result<_, E>>()?;

        mapped.assumptions = self
            .assumptions
            .iter()
            .map(|(condition, value)| Ok((map(condition)?, *value)))
            .collect::<Result<_, E>>()?;

        mapped.trusted_assumptions = self
            .trusted_assumptions
            .iter()
            .map(|(condition, value)| Ok((map(condition)?, *value)))
            .collect::<Result<_, E>>()?;

        Ok(mapped)
    }

    /// Exact local callable body established by the entry value flow, if known.
    pub fn anonymous_target(&self) -> Option<&bray_bound_tree::BoundUnitKey> {
        self.anonymous_target.as_ref()
    }

    pub(crate) fn intersect(&mut self, other: &Self) -> bool {
        assert_eq!(
            self.implementation_witnesses, other.implementation_witnesses,
            "a selected invocation retains its implementation choices across control flow"
        );
        assert_eq!(
            self.owned_inputs, other.owned_inputs,
            "one checked invocation retains its input ownership modes"
        );

        let before = (
            self.anonymous_target.is_some(),
            self.trusted_boundary,
            self.pending_execution,
            self.assumptions.len(),
            self.trusted_assumptions.len(),
            self.arguments.len(),
            self.copy_sources.len(),
            self.witness_inputs.len(),
        );

        let mut addresses_changed = false;

        for (input, addresses) in &other.addresses {
            let retained = self.addresses.entry(input.clone()).or_default();

            for (address, owners) in addresses {
                addresses_changed |= !retained.contains_key(address);

                let retained = retained.entry(address.clone()).or_default();
                let before = retained.len();

                retained.extend(owners.iter().cloned());
                addresses_changed |= retained.len() != before;
            }
        }

        if self.anonymous_target != other.anonymous_target {
            self.anonymous_target = None;
        }

        self.trusted_boundary &= other.trusted_boundary;
        self.pending_execution |= other.pending_execution;

        self.assumptions
            .retain(|condition| other.assumptions.contains(condition));

        self.trusted_assumptions
            .retain(|condition| other.trusted_assumptions.contains(condition));

        self.arguments
            .retain(|place, value| other.arguments.get(place) == Some(value));

        self.copy_sources
            .retain(|place, value| other.copy_sources.get(place) == Some(value));

        self.witness_inputs
            .retain(|owner| other.witness_inputs.contains(owner));

        addresses_changed
            || before
                != (
                    self.anonymous_target.is_some(),
                    self.trusted_boundary,
                    self.pending_execution,
                    self.assumptions.len(),
                    self.trusted_assumptions.len(),
                    self.arguments.len(),
                    self.copy_sources.len(),
                    self.witness_inputs.len(),
                )
    }

    /// Whether every callee-entry condition follows from the captured argument values.
    pub fn proves<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
    ) -> bool {
        self.proves_from(conditions, &self.assumptions, &[])
    }

    /// Whether live trusted evidence supplies every selected caller obligation.
    pub fn proves_trusted(&self, conditions: &[super::ExecutionCondition]) -> bool {
        self.proves_from(conditions, &self.trusted_assumptions, &[])
    }

    /// Checks a declaration's own inputs against captured receiver-first arguments.
    pub fn proves_trusted_with_inputs(
        &self,
        conditions: &[super::ExecutionCondition],
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> bool {
        self.proves_from(conditions, &self.trusted_assumptions, inputs)
    }

    fn proves_from<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
        assumptions: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> bool {
        let mut conditions = conditions.into_iter().peekable();

        if conditions.peek().is_none() {
            return true;
        }

        let equalities = super::ExecutionCondition::equalities(&self.assumptions, None);

        let known = assumptions
            .iter()
            .map(|(condition, value)| (condition.with_equalities(&equalities), *value))
            .collect();

        let known = super::trusted_condition_closure(known, &self.assumptions);

        conditions.all(|condition| {
            let condition = condition.substitute(
                &|input| {
                    input
                        .value_in(&self.arguments)
                        .or_else(|| {
                            let ordinal = inputs
                                .iter()
                                .position(|reference| Some(*reference) == input.reference())?;

                            let mut argument =
                                super::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                                    u32::try_from(ordinal)
                                        .expect("callable parameter ordinal fits"),
                                ));

                            // The captured argument retains this immutable component path.
                            argument.projections = std::sync::Arc::clone(&input.projections);

                            argument.value_in(&self.arguments)
                        })
                        .or_else(|| {
                            input
                                .constant_term()
                                .map(|_| super::ExecutionCondition::Input(input.clone()))
                        })
                        .or_else(|| match input.reference() {
                            Some(bray_bound_tree::BoundReferenceTarget::Surface(
                                bray_symbols::AnySymbolId::GenericConstParameter(_),
                            )) => {
                                // Generic substitution has already mapped this immutable compile-time input into the caller.
                                Some(super::ExecutionCondition::Input(input.clone()))
                            }
                            _ => None,
                        })
                        .unwrap_or(super::ExecutionCondition::Unknown)
                },
                &super::ExecutionCondition::Unknown,
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            );

            let condition = condition.with_equalities(&equalities);

            condition.prove_mixed(&known, &self.assumptions, &mut {
                super::ExecutionCondition::WORK_LIMIT
            }) == Some(true)
        })
    }
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
