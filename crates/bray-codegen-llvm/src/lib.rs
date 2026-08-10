//! LLVM implementation of Bray's code generation contract.

#![forbid(unsafe_code)]

mod backend;
mod environment;
mod initialization;
mod installation;
mod machine;
mod mapping;
mod native;
mod optimization;
mod serialization;
mod session;
mod translation;

pub use backend::LlvmCodeGenerator;
pub use environment::LLVM_PREFIX_ENVIRONMENT_VARIABLE;
pub use installation::COMPILED_LLVM_PREFIX;
