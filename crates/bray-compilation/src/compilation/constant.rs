mod call;
mod definition;
mod embedded;

pub(in crate::compilation) use definition::{
    collect_constant_references, constant_definition_id, empty_concrete_substitution,
};
