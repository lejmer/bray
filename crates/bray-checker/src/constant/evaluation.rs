mod engine;
mod selected;
mod support;

pub(crate) use engine::{check_constant_term, evaluate_constant};
pub(super) use support::EvaluationFailure;
