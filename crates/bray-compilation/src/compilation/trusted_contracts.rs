mod inputs;
mod invocation;
mod witnesses;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod host_tests;

#[cfg(test)]
pub(in crate::compilation) use host_tests::{
    production_host_compilation, production_host_compilation_for_target,
};

#[cfg(test)]
mod sync;
