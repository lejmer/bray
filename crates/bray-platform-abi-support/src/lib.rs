//! Shared raw-memory validation and export support for native platform ABI components.

#![forbid(unsafe_code)]

mod boundary;
mod region;

pub use region::{MemoryRegion, disjoint, mutually_disjoint};
