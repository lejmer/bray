mod component;
mod discovery;
mod summary;

pub(in crate::compilation) use component::BodyAnalysis;
pub(in crate::compilation) use summary::{
    anonymous_result_dependencies, default_result_dependencies, expression_result_dependencies,
    source_body_semantics, static_result_dependencies,
};
