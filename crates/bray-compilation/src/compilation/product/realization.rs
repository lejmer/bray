mod aggregate;
mod lifecycle;
mod names;
mod operations;
mod signatures;
mod statics;
mod support;
mod symbols;
mod types;

pub(in crate::compilation) use names::{
    generated_frame_symbol_name, generated_identity, generated_symbol_name,
};
pub(in crate::compilation::product) use statics::ProductStaticHostEntry;
pub(super) use support::closed_array_length;
