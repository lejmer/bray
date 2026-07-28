mod json;
mod source_map;
#[cfg(test)]
pub(crate) mod test_support;
mod text;
mod writer;

pub(crate) use json::{DiagnosticJson, diagnostic_jsons};
pub(crate) use writer::{write_driver_output, write_driver_output_error};
