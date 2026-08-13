//! Shared raw-memory validation and export support for native platform ABI components.

#![deny(unsafe_code)]

mod boundary;
mod error;
mod handle;
mod process;
mod region;
mod transfer;

pub use error::platform_io_error;
pub use handle::PROCESS_HANDLE_TAG;
pub use process::startup_working_directory;
pub use region::{MemoryRegion, disjoint, mutually_disjoint};
pub use transfer::{destination_slice, publish_transfer_count, source_slice, validate_transfer};
