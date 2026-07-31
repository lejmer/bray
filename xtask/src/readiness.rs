mod codegen;
mod command;
mod emission;
mod linker;
mod lowering;
mod memory;
mod native_execution;
mod semantic;
mod workspace;

pub(crate) use command::run;
