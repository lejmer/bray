//! LLVM implementation of Bray's code generation contract.

#![forbid(unsafe_code)]

mod backend;
mod initialization;
mod machine;
mod mapping;
mod translation;

pub use backend::LlvmCodeGenerator;
