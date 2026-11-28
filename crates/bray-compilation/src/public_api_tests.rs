//! Scenarios that drive the compiler only through its public API.
//!
//! They live in the unit-test binary because an integration test target would make Cargo
//! compile the whole library a second time.

mod complex_literals;
mod raw_memory_refinements;
mod real_power;
mod scalar_target;
