mod query;
mod static_initializer;
mod support;

pub(in crate::compilation) use support::{
    collect_constant_references, collect_constant_references_from, constant_definition_id,
};
