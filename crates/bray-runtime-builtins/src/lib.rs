//! Link-isolated compiler-provided memory, string, and character operations.

#![deny(unsafe_code)]

#[macro_use]
mod export;
#[cfg(any(feature = "memory", feature = "string"))]
mod allocation;
#[cfg(feature = "character")]
mod character;
#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "string")]
mod string;
