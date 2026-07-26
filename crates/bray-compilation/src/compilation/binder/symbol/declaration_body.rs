#[macro_use]
mod macros;
mod default;
mod dependency;
mod lookup;
mod predicate;
mod shared;

pub(super) use shared::checked_source_predicate_sequence;
