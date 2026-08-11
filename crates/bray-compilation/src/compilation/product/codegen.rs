mod error;
mod facts;
mod host;
mod implementation;
mod link;

pub use error::NativeProductFactError;
pub(in crate::compilation) use error::codegen_fact_failure_kind;
pub use facts::NativeProductFacts;
pub(in crate::compilation) use host::demanded_runtime_capabilities;
