//! Deterministic wire encoding and bounded validation for compiled package interfaces.

#![forbid(unsafe_code)]

mod diagnostic;
mod hash;
mod header;
mod limits;
mod section;
mod validation;
mod wire;

pub use diagnostic::InterfaceValidationError;
pub use hash::{InterfaceArtifactHash, InterfaceContentHash, InterfaceSectionHash};
pub use header::{
    CURRENT_FORMAT_REVISION, InterfaceFormatRevision, InterfaceHeader, InterfaceLanguageRevision,
    InterfaceRequiredFlags,
};
pub use limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
pub use section::{InterfaceSectionTag, ValidatedInterfaceSection};
pub use validation::ValidatedPackageInterface;
