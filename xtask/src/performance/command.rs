mod comparison_build;
mod compiler;
mod execution;
mod identity;
mod measurement;
mod optimization_artifacts;
mod options;
mod output;
mod progress;
mod toolchain;
mod workload_compilation;

pub(crate) use execution::run;
pub(super) use identity::expected_output_digest;
#[cfg(test)]
pub(super) use options::parse_for_test as parse_options_for_test;
pub(in crate::performance) use output::validate as validate_output;
#[cfg(test)]
pub(super) use output::validate_parts as validate_output_parts;
