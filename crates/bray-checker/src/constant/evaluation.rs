mod call;
mod engine;
mod flow;
mod pattern;
mod result;
mod selected;
mod shape;
mod support;

pub(crate) use engine::{
    check_constant_term, evaluate_constant, evaluate_constant_with_references,
};
pub use result::EvaluatedConstant;
pub(super) use support::EvaluationFailure;
