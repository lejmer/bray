mod call;
mod contract;
mod definition;
mod embedded;
#[cfg(test)]
mod layout_tests;

pub(in crate::compilation) use call::{
    CompilationConstantCallResolver, CompilationConstantTemplateResolver,
};
pub(in crate::compilation) use definition::{
    collect_constant_references, collect_constant_references_from, constant_definition_id,
};
