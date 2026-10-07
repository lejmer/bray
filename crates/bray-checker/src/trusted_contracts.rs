use std::collections::BTreeMap;

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

/// Checked predicate conditions supplied to one body analysis.
#[derive(Debug, Default)]
pub struct TrustedContractInputs {
    /// Trusted conditions the caller must supply at entry.
    pub requirements: Vec<ExecutionCondition>,
    /// Borrowed or owning subjects whose established entry authority cannot be copied.
    pub required_witnesses: Vec<(ExecutionCondition, crate::ExecutionPlace)>,
    /// Ordinary entry conditions that guard the trusted guarantees.
    pub preconditions: Vec<ExecutionCondition>,
    /// Guarantees a safe body must establish on every normal completion.
    pub guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    /// Predicate contracts on selected calls, retaining declaration identity.
    pub calls: BTreeMap<BoundExpressionId, TrustedCallContract>,
}

/// Trusted requirements and guarantees on one selected call.
#[derive(Debug, Default)]
pub struct TrustedCallContract {
    /// Exact declaration inputs for an implicit selected operation.
    pub arguments: BTreeMap<bray_bound_tree::BoundReferenceTarget, BoundExpressionId>,
    /// Whether this operation establishes guarantees at its normal completion.
    pub completes: bool,
    /// Whether result guarantees are carried by the value rather than checked storage.
    pub result_is_witness: bool,
    /// Trusted predicate subjects paired with their conditional completion guarantee.
    pub witness_subjects: Vec<(ExecutionCondition, ExecutionCondition)>,
    /// Conditions required before invocation.
    pub requirements: Vec<ExecutionCondition>,
    /// Ordinary entry conditions that guard the trusted guarantees.
    pub preconditions: Vec<ExecutionCondition>,
    /// Conditions established on normal completion.
    pub guarantees: Vec<ExecutionCondition>,
    /// Ordinary completion predicates used for path and equality reasoning.
    pub postconditions: Vec<ExecutionCondition>,
}
