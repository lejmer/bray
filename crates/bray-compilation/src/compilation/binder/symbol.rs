mod cache;
mod completion;
mod compute;
mod contract;
mod environment;
mod imported;
mod surface;
mod template;

#[cfg(test)]
mod test_support;

pub(in crate::compilation) use cache::CompilationSymbolFacts;
pub(in crate::compilation) use environment::{
    type_binder, type_scope, visible_generic_const_parameters,
};
pub(in crate::compilation) use imported::imported_implementation;
