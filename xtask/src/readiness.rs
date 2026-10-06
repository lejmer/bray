mod codegen;
mod command;
mod dependencies;
mod emission;
mod linker;
mod lowering;
mod memory;
mod native_execution;
mod semantic;
mod workspace;

pub(crate) use command::run;
