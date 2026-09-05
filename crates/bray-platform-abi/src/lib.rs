//! Link integration for independently packaged temporal and dynamic-library providers.

#![deny(unsafe_code)]

#[cfg(feature = "temporal")]
mod temporal_link;
