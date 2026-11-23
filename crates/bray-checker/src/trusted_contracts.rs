use std::collections::BTreeMap;

use bray_bound_tree::BoundExecutionSite;

use crate::ExecutionCondition;

/// Predicate meaning needed while normalizing execution conditions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PredicateConditionInput {
    condition: Option<bray_symbols::ConstantTermId>,
    domain: bray_symbols::PredicateDomain,
}

impl PredicateConditionInput {
    /// Creates one predicate input from its retained meaning and checking authority.
    pub const fn new(
        condition: Option<bray_symbols::ConstantTermId>,
        domain: bray_symbols::PredicateDomain,
    ) -> Self {
        Self { condition, domain }
    }

    /// Returns the predicate body term when the declaration defines one.
    pub const fn condition(self) -> Option<bray_symbols::ConstantTermId> {
        self.condition
    }

    /// Returns the authority required to establish the predicate.
    pub const fn domain(self) -> bray_symbols::PredicateDomain {
        self.domain
    }
}

/// Checked predicate conditions supplied to one body analysis.
#[derive(Debug, Default)]
pub struct TrustedContractInputs {
    /// Whether checked body evidence may refine returned authority beyond declared promises.
    /// Trusted assertions remain separate from evidence established by the body.
    pub infer_result_witnesses: bool,
    /// Receiver-first formal inputs used by portable result summaries.
    pub inputs: Vec<bray_bound_tree::BoundReferenceTarget>,
    /// Exact static instances whose initializer authority is needed by this body.
    pub statics: std::collections::BTreeSet<bray_symbols::StaticReferenceSelection>,
    /// Closed declaration constants used by this body's conditions.
    pub constants: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    /// Checked predicate meanings and opaque declaration authority.
    pub predicates:
        BTreeMap<bray_symbols::PredicateInstanceData, PredicateConditionInput>,
    /// Trusted conditions the caller must supply at entry.
    pub requirements: Vec<ExecutionCondition>,
    /// Borrowed or owning subjects whose established entry authority cannot be copied.
    pub required_witnesses: Vec<(ExecutionCondition, ExecutionCondition, bool)>,
    /// Ordinary entry conditions that guard the trusted guarantees.
    pub preconditions: Vec<ExecutionCondition>,
    /// Whether the declaration itself establishes its guarantees without body proof.
    pub declared_guarantees_are_authoritative: bool,
    /// Guarantees published by the declaration on every normal completion.
    pub declared_guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    /// Declared result owners that carry each established guarantee.
    pub guarantee_witness_subjects: Vec<(ExecutionCondition, ExecutionCondition)>,
    /// Predicate contracts on selected calls, retaining declaration identity.
    pub calls: BTreeMap<BoundExecutionSite, TrustedCallContract>,
}

impl TrustedContractInputs {
    /// The declared guarantees that require proof from this body.
    pub(crate) fn proof_guarantees(&self) -> &[(ExecutionCondition, bray_source::SourceSpan)] {
        if self.declared_guarantees_are_authoritative {
            &[]
        } else {
            &self.declared_guarantees
        }
    }
}

/// Trusted requirements and guarantees on one selected call.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct TrustedCallContract {
    /// Exact declaration inputs for an implicit selected operation.
    pub arguments: BTreeMap<bray_bound_tree::BoundReferenceTarget, BoundExecutionSite>,
    /// Raw storage authority consumed when constructing a linear allocation owner.
    pub transferred_inputs: Vec<bray_bound_tree::BoundReferenceTarget>,
    /// Whether this operation establishes guarantees at its normal completion.
    pub completes: bool,
    /// Whether ordinary value preconditions can fail before callee ownership transfer.
    pub checks_preconditions: bool,
    /// Whether declared purity preserves inputs after the caller establishes its requirements.
    pub preserves_inputs: bool,
    /// Checked witness carriers paired with their conditional completion guarantee.
    pub witness_subjects: Vec<(ExecutionCondition, ExecutionCondition)>,
    /// Typed raw-address subjects, their completion guarantee, and whether they carry owning authority.
    pub address_subjects: Vec<(ExecutionCondition, ExecutionCondition, bool)>,
    /// Conditions required before invocation.
    pub requirements: Vec<ExecutionCondition>,
    /// Compiler-only compound execution conditions that require ordinary caller evidence.
    pub execution_requirements: Vec<ExecutionCondition>,
    /// Checked value and execution entry conditions that guard completion guarantees.
    pub preconditions: Vec<(ExecutionCondition, bray_symbols::PredicateDomain)>,
    /// Conditions established on normal completion.
    pub guarantees: Vec<ExecutionCondition>,
    /// Ordinary completion predicates used for path and equality reasoning.
    pub postconditions: Vec<ExecutionCondition>,
}

impl TrustedCallContract {
    /// Decodes the checked predicates at one invocation boundary and its selected completion phase.
    pub fn try_from_phases<E>(
        values: &bray_symbols::SemanticValueStore,
        symbols: &bray_symbols::AvailableCompilerKnownSymbols,
        invocation: &bray_symbols::CallablePhaseBehavior,
        completion: &bray_symbols::CallablePhaseBehavior,
        completes: bool,
        mut decode: impl FnMut(&bray_symbols::PredicateSemanticSummary) -> Result<ExecutionCondition, E>,
    ) -> Result<Self, E> {
        let mut contract = Self {
            completes,
            checks_preconditions: invocation.has_runtime_requirements(),
            ..Self::default()
        };

        for predicate in invocation.predicate_requirements() {
            let condition = decode(predicate)?;

            if predicate.domain() == bray_symbols::PredicateDomain::Execution
                && predicate.execution_requirement(values, symbols).is_none()
            {
                contract.execution_requirements.push(condition.clone());
            }

            if predicate.is_trusted() { contract.requirements.push(condition); }
            else { contract.preconditions.push((condition, predicate.domain())); }
        }

        for predicate in completion.predicate_guarantees() {
            let condition = decode(predicate)?;

            if predicate.is_trusted() { contract.guarantees.push(condition); }
            else { contract.postconditions.push(condition); }
        }

        Ok(contract)
    }
}
