mod constant;
mod dependency;
mod header;

pub(in crate::compilation) use header::match_implementation_subject;
pub(super) use header::{ImplementationMatchError, match_implementation_header};
