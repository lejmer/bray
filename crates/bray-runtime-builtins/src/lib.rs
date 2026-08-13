//! Link-isolated compiler-provided memory, string, and character operations.

#![deny(unsafe_code)]

#[cfg(any(feature = "character", feature = "memory", feature = "string"))]
#[macro_use]
mod export;
#[cfg(any(feature = "memory", feature = "string"))]
mod allocation;
#[cfg(feature = "character")]
mod character;
#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "performance-observation")]
mod performance;
#[cfg(feature = "string")]
mod string;
