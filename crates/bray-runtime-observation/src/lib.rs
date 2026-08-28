//! Link-isolated compiler performance-observation hooks.

#![deny(unsafe_code)]

#[cfg(feature = "performance-observation")]
#[macro_use]
mod export;
#[cfg(feature = "performance-observation")]
mod performance;
