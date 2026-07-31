//! Shared compiler command tooling used by the loose-file and project drivers.

#![forbid(unsafe_code)]

mod inspection;
mod model;
mod output;
mod product;
mod source;
mod status;
#[cfg(test)]
mod test_support;

pub use inspection::{
    InspectionError, InspectionOutput, format_semantic_type, render_bound_inspection,
    render_declaration_inspection, render_lowered_inspection,
    render_mir_inspection, render_source_inspection,
    render_symbol_inspection, render_syntax_inspection,
    render_token_inspection,
};
pub use model::{InspectionTarget, OutputFormat};
pub use output::{
    clap_styles, render_styled_text, write_diagnostic_groups,
    write_diagnostics,
};
pub use product::{
    baseline_output_name, baseline_target_outputs, load_compilation,
    load_llvm_compilation, native_linker, package_interface_export_request,
    project_interface_path, project_output_directory, selected_target,
};
pub use source::{
    SourceInputError, compilation_request_from_file_arguments,
    source_inputs_from_file_arguments,
};
pub use status::exit_code_from_diagnostics;
