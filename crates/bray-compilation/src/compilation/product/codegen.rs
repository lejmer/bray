mod error;
mod facts;
mod host;
mod implementation;
mod link;

pub use error::NativeProductFactError;
pub use facts::NativeProductFacts;
pub(in crate::compilation) use host::demanded_runtime_capabilities;
