//! Shared raw-memory validation and export support for temporal and runtime ABI components.

#![deny(unsafe_code)]

mod boundary;
mod error;
mod region;
mod transfer;

pub use error::platform_io_error;
pub use region::{MemoryRegion, disjoint, mutually_disjoint};
pub use transfer::{destination_slice, publish_transfer_count, source_slice, validate_transfer};

#[doc(hidden)]
pub use bray_runtime_abi::{PlatformAbiValue, platform_signature_matches};
