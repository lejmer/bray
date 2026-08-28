//! Stable native entry adapters for semantic reference-runtime components.

#![deny(unsafe_code)]

#[cfg(any(
    feature = "callback",
    feature = "host",
    feature = "scheduler",
    feature = "cancellation",
    feature = "event",
    feature = "test-host"
))]
#[macro_use]
mod export;
#[cfg(feature = "callback")]
mod callback;
#[cfg(feature = "cancellation")]
mod cancellation;
#[cfg(feature = "event")]
mod event;
#[cfg(feature = "host")]
mod host;
#[cfg(feature = "scheduler")]
mod scheduler;
#[cfg(feature = "test-host")]
mod test_entry;
#[cfg(feature = "test-host")]
mod test_host;
