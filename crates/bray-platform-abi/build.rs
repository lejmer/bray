#[path = "build/core.rs"]
mod core;
#[path = "build/dynamic.rs"]
mod dynamic;

pub(crate) use core::main;
