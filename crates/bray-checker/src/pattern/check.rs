mod result;
mod state;
mod subject;

pub(in crate::pattern) use result::effective_pattern_kind;
pub(in crate::pattern) use state::PatternChecker;
pub(crate) use state::check_patterns;
