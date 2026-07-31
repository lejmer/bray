use bray_compilation::Compilation;

use crate::{InspectionTarget, OutputFormat};

use super::support::InspectionOutput;

/// Failure to construct a requested compiler inspection report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectionError;

/// Renders loaded source snapshots.
pub fn render_source_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<String, InspectionError> {
    super::source::render_source_inspection(compilation, output_format).map_err(|_| InspectionError)
}

/// Renders lexical tokens for loaded source snapshots.
pub fn render_token_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::token::render_token_inspection(compilation, output_format).map_err(|_| InspectionError)
}

/// Renders parsed syntax trees for loaded source snapshots.
pub fn render_syntax_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::syntax::render_syntax_inspection(compilation, output_format).map_err(|_| InspectionError)
}

/// Renders discovered declarations for loaded source snapshots.
pub fn render_declaration_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::declaration::render_declaration_inspection(compilation, output_format)
        .map_err(|_| InspectionError)
}

/// Renders the compilation-wide symbol graph.
pub fn render_symbol_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::symbol::render_symbol_inspection(compilation, output_format).map_err(|_| InspectionError)
}

/// Renders selected source-correlated bound units.
pub fn render_bound_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::bound::render_bound_inspection(compilation, target, output_format)
        .map_err(|_| InspectionError)
}

/// Renders selected lowered units.
pub fn render_lowered_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::lowered::render_lowered_inspection(compilation, target, output_format)
        .map_err(|_| InspectionError)
}

/// Renders selected lowered units as MIR notation.
pub fn render_mir_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::lowered::render_mir_inspection(compilation, target, output_format)
        .map_err(|_| InspectionError)
}
