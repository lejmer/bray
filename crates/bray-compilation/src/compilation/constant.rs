mod call;
mod definition;
mod embedded;

pub(in crate::compilation) use call::{
    CompilationConstantCallResolver, CompilationConstantTemplateResolver,
};
pub(in crate::compilation) use definition::{
    collect_constant_references, collect_constant_references_from, constant_definition_id,
    empty_concrete_substitution,
};
