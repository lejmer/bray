mod callable;
#[cfg(test)]
mod catalog;
mod emission;
mod inspection;
mod interface;
mod native;
mod project;
mod selection;
mod semantics;
mod source;
mod target;
mod value;

#[cfg(test)]
pub(super) use catalog::USER_FACING_SOURCES;
pub(crate) use source::{format_source_location, format_source_span};
pub(crate) use value::format_value;

#[cfg(test)]
mod tests;
