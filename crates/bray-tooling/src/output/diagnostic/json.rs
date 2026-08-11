mod argument;
mod checking;
mod emission;
mod foreign;
mod interface;
mod project;
mod report;
#[cfg(test)]
mod tests;
mod value;

pub(in crate::output::diagnostic::json) use argument::DiagnosticArgValueJson;
pub(in crate::output::diagnostic::json) use checking::{
    DiagnosticCallableOverloadProblemJson, DiagnosticImplementationOverloadProblemJson,
    DiagnosticPatternCoverageJson, DiagnosticSelectionCandidatesJson,
    DiagnosticSelectionRejectionsJson, DiagnosticStorageAccessJson,
    DiagnosticTraitFulfillmentMismatchJson, array_generator_problem_json,
    callback_state_problem_json, copy_contract_problem_json, layout_problem_json,
    propagation_problem_json, refinement_capacity_json, union_tag_problem_json,
};
pub(in crate::output::diagnostic::json) use emission::DiagnosticEmissionFailureJson;
pub(in crate::output::diagnostic::json) use foreign::{
    DiagnosticProblemJson, native_link_directive_problem_json,
    native_symbol_directive_problem_json, platform_service_signature_problem_json,
};
pub(in crate::output::diagnostic::json) use interface::{
    DiagnosticInterfaceSymbolIdentityJson, interface_semantic_problem_json,
    interface_symbol_graph_problem_json, problem, problem_array_length, problem_count_u64,
    problem_text, problem_type, problem_types,
};
pub(in crate::output::diagnostic::json) use project::{
    DiagnosticExternalToolExitJson, DiagnosticLinkerDriverIdentityJson,
    DiagnosticProjectCommandFailureJson, DiagnosticProjectDependencyCycleMemberJson,
    DiagnosticProjectSelectionJson, DiagnosticSourceInputJson,
    DiagnosticUnsupportedEmissionReasonJson,
};
pub(super) use report::write_json_diagnostic_groups;
pub(crate) use report::write_json_diagnostics;
#[cfg(feature = "analysis")]
pub(crate) use report::{DiagnosticJson, diagnostic_jsons};
pub(in crate::output::diagnostic::json) use value::{
    DiagnosticArtifactDigestJson, DiagnosticLinkRequirementJson, DiagnosticOutputSinkJson,
    DiagnosticRuntimeAbiVersionJson, DiagnosticTypeJson, SourceSpanJson,
};
