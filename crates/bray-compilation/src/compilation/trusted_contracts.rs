mod inputs;
mod invocation;
mod witnesses;

#[cfg(test)]
mod contract_tests;

#[cfg(test)]
pub(in crate::compilation) use contract_tests::{
    production_host_compilation, production_host_compilation_for_target,
};
