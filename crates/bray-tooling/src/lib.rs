//! Shared compiler command tooling used by the loose-file and project drivers.

#![forbid(unsafe_code)]

#[cfg(feature = "analysis")]
mod inspection;
mod model;
mod output;
#[cfg(feature = "analysis")]
mod product;
#[cfg(feature = "compiler")]
mod runtime;
#[cfg(feature = "analysis")]
mod source;
mod status;
#[cfg(test)]
mod test_support;

#[cfg(feature = "analysis")]
pub use inspection::{
    InspectionError, InspectionOutput, format_semantic_type, render_bound_inspection,
    render_declaration_inspection, render_lowered_inspection, render_mir_inspection,
    render_source_inspection, render_symbol_inspection, render_syntax_inspection,
    render_token_inspection,
};
pub use model::{InspectionTarget, OutputFormat};
pub use output::{clap_styles, render_styled_text, write_diagnostic_groups, write_diagnostics};
#[cfg(feature = "analysis")]
pub use product::{
    load_compilation, package_interface_export_request, project_interface_path,
    project_output_directory, selected_target,
};
#[cfg(feature = "compiler")]
pub use product::{load_llvm_compilation, native_linker};
#[cfg(feature = "compiler")]
pub use runtime::load_runtime_artifact;
#[cfg(feature = "analysis")]
pub use source::{
    SourceInputError, compilation_request_from_file_arguments, source_inputs_from_file_arguments,
};
pub use status::exit_code_from_diagnostics;
