mod aggregate;
mod lifecycle;
mod names;
mod operations;
mod signatures;
mod statics;
mod support;
mod symbols;
mod types;

pub(in crate::compilation) use names::{generated_frame_symbol_name, generated_symbol_name};
pub(super) use support::closed_array_length;
