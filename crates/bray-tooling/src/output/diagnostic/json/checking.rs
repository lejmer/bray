mod analysis;
mod candidate;
mod contract_mismatch;
mod overload;
mod selection;
mod trait_mismatch;

pub(super) use analysis::{
    DiagnosticPatternCoverageJson, DiagnosticStorageAccessJson, array_generator_problem_json,
    callback_state_problem_json, copy_contract_problem_json, layout_problem_json,
    propagation_problem_json, refinement_capacity_json, union_tag_problem_json,
};
pub(super) use overload::{
    DiagnosticCallableOverloadProblemJson, DiagnosticImplementationOverloadProblemJson,
};
pub(super) use selection::{
    DiagnosticSelectionCandidatesJson, DiagnosticSelectionRejectionsJson,
};
pub(super) use trait_mismatch::DiagnosticTraitFulfillmentMismatchJson;
