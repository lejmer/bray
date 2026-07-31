mod json;
mod source_map;
#[cfg(test)]
pub(crate) mod test_support;
mod text;
mod writer;

#[cfg(feature = "analysis")]
pub(crate) use json::{DiagnosticJson, diagnostic_jsons};
pub use writer::{write_diagnostic_groups, write_diagnostics};
