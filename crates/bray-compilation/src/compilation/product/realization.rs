mod aggregate;
mod contextual_self;
mod debug;
mod known;
mod lifecycle;
mod names;
mod native_statics;
mod operations;
mod runtime_signatures;
mod runtime_source;
mod signatures;
mod standard;
mod statics;
mod storage;
mod support;
mod symbols;
mod types;
mod value_mappings;

pub(in crate::compilation::product) use contextual_self::{
    codegen_instance_contextual_self, substitute_contextual_self,
    substitute_contextual_self_in_application, substitute_contextual_self_in_substitution,
};
pub(in crate::compilation) use names::{
    generated_frame_symbol_name, generated_identity, generated_symbol_name,
};
pub(in crate::compilation::product) use storage::ProductStaticHostEntry;
pub(super) use support::closed_array_length;
