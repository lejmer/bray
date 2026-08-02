mod codegen;
mod dependency;
mod entry;
mod query;
mod realization;
mod specialization;
mod specialization_identity;
mod visibility;

pub use codegen::{NativeProductFactError, NativeProductFacts};
#[cfg(test)]
pub(in crate::compilation) use realization::{generated_frame_symbol_name, generated_symbol_name};
