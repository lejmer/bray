use std::collections::BTreeMap;

use bray_bound_tree::BoundExecutionSite;

use crate::ExecutionCondition;

/// Checked predicate conditions supplied to one body analysis.
#[derive(Debug, Default)]
pub struct TrustedContractInputs {
    /// Whether checked body evidence may refine returned authority beyond declared promises.
    /// Trusted implementations expose only their declared boundary contract.
    pub infer_result_witnesses: bool,
    /// Receiver-first formal inputs used by portable result summaries.
    pub inputs: Vec<bray_bound_tree::BoundReferenceTarget>,
    /// Exact static instances whose initializer authority is needed by this body.
    pub statics: std::collections::BTreeSet<bray_symbols::StaticReferenceSelection>,
    /// Closed declaration constants used by this body's conditions.
    pub constants: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    /// Checked ordinary predicate bodies in positional argument form.
    pub predicates: BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    /// Trusted conditions the caller must supply at entry.
    pub requirements: Vec<ExecutionCondition>,
    /// Borrowed or owning subjects whose established entry authority cannot be copied.
    pub required_witnesses: Vec<(ExecutionCondition, crate::ExecutionPlace)>,
    /// Ordinary entry conditions that guard the trusted guarantees.
    pub preconditions: Vec<ExecutionCondition>,
    /// Guarantees a safe body must establish on every normal completion.
    pub guarantees: Vec<(ExecutionCondition, bray_source::SourceSpan)>,
    /// Predicate contracts on selected calls, retaining declaration identity.
    pub calls: BTreeMap<BoundExecutionSite, TrustedCallContract>,
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
    /// Whether declared purity preserves inputs after the caller establishes its requirements.
    pub preserves_inputs: bool,
    /// Checked witness carriers paired with their conditional completion guarantee.
    pub witness_subjects: Vec<(ExecutionCondition, ExecutionCondition)>,
    /// Conditions required before invocation.
    pub requirements: Vec<ExecutionCondition>,
    /// Compiler-only compound execution conditions that require ordinary caller evidence.
    pub execution_requirements: Vec<ExecutionCondition>,
    /// Ordinary entry conditions that guard the trusted guarantees.
    pub preconditions: Vec<ExecutionCondition>,
    /// Conditions established on normal completion.
    pub guarantees: Vec<ExecutionCondition>,
    /// Ordinary completion predicates used for path and equality reasoning.
    pub postconditions: Vec<ExecutionCondition>,
}

impl TrustedCallContract {
    pub(crate) fn result_guarantees_name_whole_owner(&self) -> bool {
        let mut subjects = self
            .witness_subjects
            .iter()
            .filter(|(_, subject)| subject.observes_completion_result())
            .peekable();

        subjects.peek().is_some()
            && subjects.all(|(_, subject)| {
                let mut subject = subject;

                while let ExecutionCondition::Borrowed(value) | ExecutionCondition::Present(value) =
                    subject
                {
                    subject = value;
                }

                matches!(subject, ExecutionCondition::Result)
            })
    }
}
