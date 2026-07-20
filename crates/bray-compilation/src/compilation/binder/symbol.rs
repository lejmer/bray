mod cache;
mod completion;
mod compute;
mod contract;
mod environment;
mod surface;
mod template;

#[cfg(test)]
mod test_support;

pub(in crate::compilation) use cache::CompilationSymbolFacts;
pub(in crate::compilation) use environment::{type_binder, visible_generic_const_parameters};
