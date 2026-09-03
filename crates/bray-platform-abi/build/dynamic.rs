#[path = "dynamic/contract.rs"]
mod contract;
#[path = "dynamic/core.rs"]
mod core;

pub(super) use core::build;
