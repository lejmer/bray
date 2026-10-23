use bray_compilation::Compilation;
use bray_diagnostics::{
    DiagnosticBoundInspectionFailure, DiagnosticFailureField, DiagnosticFailureValue,
    DiagnosticInspectionFailure, DiagnosticInspectionFailureDetail,
    DiagnosticInspectionOutputFormat, DiagnosticInspectionTarget,
    DiagnosticLoweredInspectionFailure, DiagnosticSymbolInspectionFailure,
};

use crate::{InspectionTarget, OutputFormat};

use super::bound::BoundInspectionRenderError;
use super::declaration::DeclarationInspectionRenderError;
use super::lowered::LoweredInspectionRenderError;
use super::source::SourceInspectionRenderError;
use super::support::InspectionOutput;
use super::symbol::SymbolInspectionRenderError;
use super::syntax::SyntaxInspectionRenderError;
use super::token::TokenInspectionRenderError;

/// Failure to construct a requested compiler inspection report.
pub type InspectionError = DiagnosticInspectionFailure;

/// Renders loaded source snapshots.
pub fn render_source_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<String, InspectionError> {
    super::source::render_source_inspection(compilation, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Source {
            format: diagnostic_format(output_format),
            cause: source_inspection_failure(error),
        }
    })
}

/// Renders lexical tokens for loaded source snapshots.
pub fn render_token_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::token::render_token_inspection(compilation, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Token {
            format: diagnostic_format(output_format),
            cause: token_inspection_failure(error),
        }
    })
}

/// Renders parsed syntax trees for loaded source snapshots.
pub fn render_syntax_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::syntax::render_syntax_inspection(compilation, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Syntax {
            format: diagnostic_format(output_format),
            cause: syntax_inspection_failure(error),
        }
    })
}

/// Renders discovered declarations for loaded source snapshots.
pub fn render_declaration_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::declaration::render_declaration_inspection(compilation, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Declaration {
            format: diagnostic_format(output_format),
            cause: declaration_inspection_failure(error),
        }
    })
}

/// Renders the compilation-wide symbol graph.
pub fn render_symbol_inspection(
    compilation: &Compilation,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::symbol::render_symbol_inspection(compilation, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Symbol {
            format: diagnostic_format(output_format),
            cause: symbol_inspection_failure(error),
        }
    })
}

/// Renders selected source-correlated bound units.
pub fn render_bound_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::bound::render_bound_inspection(compilation, target, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Bound {
            target: diagnostic_target(target),
            format: diagnostic_format(output_format),
            cause: bound_inspection_failure(error),
        }
    })
}

/// Renders selected lowered units.
pub fn render_lowered_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
) -> Result<InspectionOutput, InspectionError> {
    super::lowered::render_lowered_inspection(compilation, target, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Lowered {
            target: diagnostic_target(target),
            format: diagnostic_format(output_format),
            cause: lowered_inspection_failure(error),
        }
    })
}

/// Renders selected lowered units as MIR notation.
pub fn render_mir_inspection(
    compilation: &Compilation,
    target: InspectionTarget,
    output_format: OutputFormat,
    include_source: bool,
) -> Result<InspectionOutput, InspectionError> {
    super::lowered::render_mir_inspection(compilation, target, output_format, include_source)
        .map_err(|error| DiagnosticInspectionFailure::Mir {
            target: diagnostic_target(target),
            format: diagnostic_format(output_format),
            cause: lowered_inspection_failure(error),
        })
}

const fn diagnostic_format(format: OutputFormat) -> DiagnosticInspectionOutputFormat {
    match format {
        OutputFormat::Text => DiagnosticInspectionOutputFormat::Text,
        OutputFormat::Json => DiagnosticInspectionOutputFormat::Json,
    }
}

const fn diagnostic_target(target: InspectionTarget) -> DiagnosticInspectionTarget {
    DiagnosticInspectionTarget {
        source_id: target.source_id(),
        position: match target.position() {
            Some(position) => Some(position.bytes()),
            None => None,
        },
    }
}

fn source_inspection_failure(
    error: SourceInspectionRenderError,
) -> DiagnosticInspectionFailureDetail {
    match error {
        SourceInspectionRenderError::Capacity { resource, actual } => {
            capacity_detail(resource, actual)
        }
        SourceInspectionRenderError::SourceIndexOverflow(error) => source_overflow_detail(error),
        SourceInspectionRenderError::Json(report) => {
            report_detail("source_inspection_json", report)
        }
    }
}

fn token_inspection_failure(
    error: TokenInspectionRenderError,
) -> DiagnosticInspectionFailureDetail {
    match error {
        TokenInspectionRenderError::SourceIndexOverflow(error) => source_overflow_detail(error),
        TokenInspectionRenderError::Json(report) => report_detail("token_inspection_json", report),
    }
}

fn syntax_inspection_failure(
    error: SyntaxInspectionRenderError,
) -> DiagnosticInspectionFailureDetail {
    match error {
        SyntaxInspectionRenderError::SourceIndexOverflow(error) => source_overflow_detail(error),
        SyntaxInspectionRenderError::Json(report) => {
            report_detail("syntax_inspection_json", report)
        }
    }
}

fn declaration_inspection_failure(
    error: DeclarationInspectionRenderError,
) -> DiagnosticInspectionFailureDetail {
    match error {
        DeclarationInspectionRenderError::SourceIndexOverflow(error) => {
            source_overflow_detail(error)
        }
        DeclarationInspectionRenderError::Json(report) => {
            report_detail("declaration_inspection_json", report)
        }
    }
}

fn symbol_inspection_failure(
    error: SymbolInspectionRenderError,
) -> DiagnosticSymbolInspectionFailure {
    match error {
        SymbolInspectionRenderError::Evaluation(error) => {
            DiagnosticSymbolInspectionFailure::Evaluation(error.diagnostic_evaluation_failure())
        }
        SymbolInspectionRenderError::Json(report) => DiagnosticSymbolInspectionFailure::Detail(
            report_detail("symbol_inspection_json", report),
        ),
        SymbolInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticSymbolInspectionFailure::Detail(source_overflow_detail(error))
        }
        SymbolInspectionRenderError::Type(error) => {
            DiagnosticSymbolInspectionFailure::Detail(type_error_detail(error))
        }
    }
}

fn bound_inspection_failure(error: BoundInspectionRenderError) -> DiagnosticBoundInspectionFailure {
    match error {
        BoundInspectionRenderError::Evaluation(error) => {
            DiagnosticBoundInspectionFailure::Evaluation(error.diagnostic_evaluation_failure())
        }
        BoundInspectionRenderError::Json(report) => {
            DiagnosticBoundInspectionFailure::Detail(report_detail("bound_inspection_json", report))
        }
        BoundInspectionRenderError::Source => DiagnosticBoundInspectionFailure::Source,
        BoundInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticBoundInspectionFailure::Detail(source_overflow_detail(error))
        }
        BoundInspectionRenderError::Type(error) => {
            DiagnosticBoundInspectionFailure::Detail(type_error_detail(error))
        }
        BoundInspectionRenderError::Capacity { resource, actual } => {
            DiagnosticBoundInspectionFailure::Detail(capacity_detail(resource, actual))
        }
    }
}

fn lowered_inspection_failure(
    error: LoweredInspectionRenderError,
) -> DiagnosticLoweredInspectionFailure {
    match error {
        LoweredInspectionRenderError::Evaluation(error) => {
            DiagnosticLoweredInspectionFailure::Evaluation(error.diagnostic_evaluation_failure())
        }
        LoweredInspectionRenderError::Json(report) => DiagnosticLoweredInspectionFailure::Detail(
            report_detail("lowered_inspection_json", report),
        ),
        LoweredInspectionRenderError::Model(super::lowered::MirInspectionModelError::Type(
            error,
        )) => DiagnosticLoweredInspectionFailure::Detail(type_error_detail(error)),
        LoweredInspectionRenderError::Source => DiagnosticLoweredInspectionFailure::Source,
        LoweredInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticLoweredInspectionFailure::Detail(source_overflow_detail(error))
        }
    }
}

fn report_detail(reason: &'static str, report: String) -> DiagnosticInspectionFailureDetail {
    inspection_detail(reason, [text_field("report", report)])
}

fn source_overflow_detail(
    error: bray_source::TextSizeOverflow,
) -> DiagnosticInspectionFailureDetail {
    capacity_detail("source_text_bytes", error.bytes())
}

fn type_error_detail(
    error: super::types::TypeInspectionError,
) -> DiagnosticInspectionFailureDetail {
    match error {
        super::types::TypeInspectionError::Depth => inspection_detail(
            "inspection_resource_limit",
            [
                text_field("resource", "type_depth"),
                text_field("maximum", super::types::MAXIMUM_TYPE_DEPTH.to_string()),
            ],
        ),
    }
}

fn capacity_detail(resource: &'static str, actual: usize) -> DiagnosticInspectionFailureDetail {
    inspection_detail(
        "inspection_resource_limit",
        [
            text_field("resource", resource),
            text_field("actual", actual.to_string()),
        ],
    )
}

fn inspection_detail<const N: usize>(
    reason: &'static str,
    context: [DiagnosticFailureField; N],
) -> DiagnosticInspectionFailureDetail {
    DiagnosticInspectionFailureDetail::new(reason, context)
}

fn text_field(name: &'static str, value: impl Into<String>) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Text(value.into()))
}
