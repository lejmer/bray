mod coherence;
mod result;
mod state;
mod subject;

pub(in crate::pattern) use result::effective_pattern_kind;
pub(crate) use state::check_patterns;
pub(in crate::pattern) use state::{PatternChecker, available_dependency};
