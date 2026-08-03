//! Compiled package-interface publication.

mod build;
mod error;
mod semantic;
mod template;

pub(in crate::compilation) use build::external_symbol_key;
pub use error::PackageInterfaceExportError;
