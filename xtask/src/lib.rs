#![forbid(unsafe_code)]

#[cfg(feature = "full")]
mod bundle;
mod command;
#[cfg(feature = "full")]
mod compiler_known;
#[cfg(feature = "full")]
mod composition;
#[cfg(feature = "full")]
mod dependency_audit;
#[cfg(feature = "full")]
mod diagnostic_output;
#[cfg(feature = "full")]
mod formatter_reference;
#[cfg(feature = "full")]
mod input_identity;
mod json;
#[cfg(feature = "full")]
mod link_map;
#[cfg(feature = "full")]
mod native_archive;
#[cfg(feature = "full")]
mod native_package;
#[cfg(feature = "full")]
mod native_product;
#[cfg(feature = "full")]
mod native_symbols;
#[cfg(feature = "full")]
mod native_test_report;
#[cfg(feature = "full")]
mod native_toolchain;
#[cfg(feature = "full")]
mod package_interface;
#[cfg(feature = "full")]
mod path;
#[cfg(feature = "full")]
mod performance;
#[cfg(feature = "full")]
mod preparation_tools;
#[cfg(feature = "full")]
mod progress;
#[cfg(feature = "full")]
mod readiness;
#[cfg(feature = "full")]
mod runtime_artifact;
#[cfg(feature = "full")]
mod source_format;
#[cfg(feature = "full")]
mod standard_library;
#[cfg(feature = "full")]
mod style;
mod testing;
#[cfg(feature = "full")]
mod text;
mod validation_process;
#[cfg(feature = "full")]
mod windows_crt;
mod workspace;

pub use command::run;
#[cfg(feature = "full")]
pub use native_archive::rust_native_link_argument;
#[cfg(feature = "full")]
pub use runtime_artifact::build_bootstrap;
