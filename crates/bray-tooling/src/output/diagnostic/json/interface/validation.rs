mod failure;
mod problem;

pub(in crate::output::diagnostic::json) use failure::interface_validation_failure_json;
pub(in crate::output::diagnostic::json) use problem::{
    interface_semantic_problem_json, interface_symbol_graph_problem_json, problem,
    problem_array_length, problem_count_u64, problem_text, problem_type, problem_types,
};
