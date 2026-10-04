#![forbid(unsafe_code)]

mod bundle;
mod command;
mod compiler_known;
mod composition;
mod dependency_audit;
mod diagnostic_output;
mod input_identity;
mod json;
mod link_map;
mod native_archive;
mod native_package;
mod native_product;
mod native_symbols;
mod native_test_report;
mod native_toolchain;
mod package_interface;
mod path;
mod performance;
mod preparation_tools;
mod progress;
mod readiness;
mod runtime_artifact;
mod source_format;
mod standard_library;
mod style;
mod text;
mod windows_crt;
mod workspace;

pub use command::run;
pub use native_archive::rust_native_link_argument;
pub use runtime_artifact::build_bootstrap;
