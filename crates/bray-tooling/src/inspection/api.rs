use bray_compilation::Compilation;
use bray_diagnostics::{
    DiagnosticBoundInspectionFailure, DiagnosticDeclarationInspectionFailure,
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticInspectionFailure,
    DiagnosticInspectionFailureDetail, DiagnosticInspectionOutputFormat,
    DiagnosticInspectionTarget, DiagnosticLoweredInspectionFailure,
    DiagnosticSourceInspectionFailure, DiagnosticSymbolInspectionFailure,
    DiagnosticSyntaxInspectionFailure, DiagnosticTokenInspectionFailure,
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
) -> DiagnosticSourceInspectionFailure {
    match error {
        SourceInspectionRenderError::Capacity { resource, actual } => {
            DiagnosticSourceInspectionFailure::Detail(capacity_detail(resource, actual))
        }
        SourceInspectionRenderError::SourceIndex => DiagnosticSourceInspectionFailure::SourceIndex,
        SourceInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticSourceInspectionFailure::Detail(source_overflow_detail(error))
        }
        SourceInspectionRenderError::Json(report) => DiagnosticSourceInspectionFailure::Detail(
            report_detail("source_inspection_json", report),
        ),
    }
}

fn token_inspection_failure(error: TokenInspectionRenderError) -> DiagnosticTokenInspectionFailure {
    match error {
        TokenInspectionRenderError::SourceIndex => DiagnosticTokenInspectionFailure::SourceIndex,
        TokenInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticTokenInspectionFailure::Detail(source_overflow_detail(error))
        }
        TokenInspectionRenderError::TokenText => DiagnosticTokenInspectionFailure::TokenText,
        TokenInspectionRenderError::TriviaText => DiagnosticTokenInspectionFailure::TriviaText,
        TokenInspectionRenderError::Json(report) => {
            DiagnosticTokenInspectionFailure::Detail(report_detail("token_inspection_json", report))
        }
    }
}

fn syntax_inspection_failure(
    error: SyntaxInspectionRenderError,
) -> DiagnosticSyntaxInspectionFailure {
    match error {
        SyntaxInspectionRenderError::SourceIndex => DiagnosticSyntaxInspectionFailure::SourceIndex,
        SyntaxInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticSyntaxInspectionFailure::Detail(source_overflow_detail(error))
        }
        SyntaxInspectionRenderError::SourceMismatch => {
            DiagnosticSyntaxInspectionFailure::SourceMismatch
        }
        SyntaxInspectionRenderError::TokenText => DiagnosticSyntaxInspectionFailure::TokenText,
        SyntaxInspectionRenderError::TriviaText => DiagnosticSyntaxInspectionFailure::TriviaText,
        SyntaxInspectionRenderError::TreeStructure => {
            DiagnosticSyntaxInspectionFailure::TreeStructure
        }
        SyntaxInspectionRenderError::Json(report) => DiagnosticSyntaxInspectionFailure::Detail(
            report_detail("syntax_inspection_json", report),
        ),
    }
}

fn declaration_inspection_failure(
    error: DeclarationInspectionRenderError,
) -> DiagnosticDeclarationInspectionFailure {
    match error {
        DeclarationInspectionRenderError::Container => {
            DiagnosticDeclarationInspectionFailure::Container
        }
        DeclarationInspectionRenderError::Declaration => {
            DiagnosticDeclarationInspectionFailure::Declaration
        }
        DeclarationInspectionRenderError::ModulePart => {
            DiagnosticDeclarationInspectionFailure::ModulePart
        }
        DeclarationInspectionRenderError::Source => DiagnosticDeclarationInspectionFailure::Source,
        DeclarationInspectionRenderError::SourceIndex => {
            DiagnosticDeclarationInspectionFailure::SourceIndex
        }
        DeclarationInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticDeclarationInspectionFailure::Detail(source_overflow_detail(error))
        }
        DeclarationInspectionRenderError::Json(report) => {
            DiagnosticDeclarationInspectionFailure::Detail(report_detail(
                "declaration_inspection_json",
                report,
            ))
        }
    }
}

fn symbol_inspection_failure(
    error: SymbolInspectionRenderError,
) -> DiagnosticSymbolInspectionFailure {
    match error {
        SymbolInspectionRenderError::Declaration => DiagnosticSymbolInspectionFailure::Declaration,
        SymbolInspectionRenderError::Evaluation(error) => {
            DiagnosticSymbolInspectionFailure::Evaluation(error.diagnostic_evaluation_failure())
        }
        SymbolInspectionRenderError::Json(report) => DiagnosticSymbolInspectionFailure::Detail(
            report_detail("symbol_inspection_json", report),
        ),
        SymbolInspectionRenderError::Source(error) => source_error_detail(error).map_or(
            DiagnosticSymbolInspectionFailure::Source,
            DiagnosticSymbolInspectionFailure::Detail,
        ),
        SymbolInspectionRenderError::SourceIndex => DiagnosticSymbolInspectionFailure::SourceIndex,
        SymbolInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticSymbolInspectionFailure::Detail(source_overflow_detail(error))
        }
        SymbolInspectionRenderError::Symbol => DiagnosticSymbolInspectionFailure::Symbol,
        SymbolInspectionRenderError::SymbolCycle => DiagnosticSymbolInspectionFailure::SymbolCycle,
        SymbolInspectionRenderError::Type(error) => type_error_detail(error).map_or(
            DiagnosticSymbolInspectionFailure::Type,
            DiagnosticSymbolInspectionFailure::Detail,
        ),
        SymbolInspectionRenderError::UnsupportedRelationship => {
            DiagnosticSymbolInspectionFailure::UnsupportedRelationship
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
        BoundInspectionRenderError::MissingNode => DiagnosticBoundInspectionFailure::MissingNode,
        BoundInspectionRenderError::Source(error) => source_error_detail(error).map_or(
            DiagnosticBoundInspectionFailure::Source,
            DiagnosticBoundInspectionFailure::Detail,
        ),
        BoundInspectionRenderError::SourceIndex => DiagnosticBoundInspectionFailure::SourceIndex,
        BoundInspectionRenderError::SourceIndexOverflow(error) => {
            DiagnosticBoundInspectionFailure::Detail(source_overflow_detail(error))
        }
        BoundInspectionRenderError::Symbol => DiagnosticBoundInspectionFailure::Symbol,
        BoundInspectionRenderError::Type(error) => type_error_detail(error).map_or(
            DiagnosticBoundInspectionFailure::Type,
            DiagnosticBoundInspectionFailure::Detail,
        ),
        BoundInspectionRenderError::Selection(error) => selection_error_detail(error).map_or(
            DiagnosticBoundInspectionFailure::Selection,
            DiagnosticBoundInspectionFailure::Detail,
        ),
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
        LoweredInspectionRenderError::Model(error) => mir_model_error_detail(error).map_or(
            DiagnosticLoweredInspectionFailure::Model,
            DiagnosticLoweredInspectionFailure::Detail,
        ),
        LoweredInspectionRenderError::Source(error) => source_error_detail(error).map_or(
            DiagnosticLoweredInspectionFailure::Source,
            DiagnosticLoweredInspectionFailure::Detail,
        ),
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

fn capacity_detail(resource: &'static str, actual: usize) -> DiagnosticInspectionFailureDetail {
    inspection_detail(
        "inspection_resource_limit",
        [
            text_field("resource", resource),
            text_field("actual", actual.to_string()),
        ],
    )
}

fn source_error_detail(
    error: super::support::InspectionSourceError,
) -> Option<DiagnosticInspectionFailureDetail> {
    match error {
        super::support::InspectionSourceError::Source
        | super::support::InspectionSourceError::SourceIndex => None,
        super::support::InspectionSourceError::SourceIndexOverflow(error) => {
            Some(source_overflow_detail(error))
        }
    }
}

fn type_error_detail(
    error: super::types::TypeInspectionError,
) -> Option<DiagnosticInspectionFailureDetail> {
    match error {
        super::types::TypeInspectionError::Depth => None,
        super::types::TypeInspectionError::SemanticValue(error) => {
            Some(semantic_value_detail(error))
        }
    }
}

fn selection_error_detail(
    error: super::bound::SelectionInspectionError,
) -> Option<DiagnosticInspectionFailureDetail> {
    match error {
        super::bound::SelectionInspectionError::InvalidConstraintDispatch
        | super::bound::SelectionInspectionError::Local => None,
        super::bound::SelectionInspectionError::SemanticValue(error) => {
            Some(semantic_value_detail(error))
        }
        super::bound::SelectionInspectionError::Type(error) => type_error_detail(error),
    }
}

fn mir_model_error_detail(
    error: super::lowered::MirInspectionModelError,
) -> Option<DiagnosticInspectionFailureDetail> {
    use super::lowered::MirInspectionModelError as Error;

    match error {
        Error::InvalidGeneratedLifecycle => Some(inspection_detail(
            "mir_inspection_invalid_generated_lifecycle",
            [],
        )),
        Error::MissingOperation => Some(inspection_detail("mir_inspection_missing_operation", [])),
        Error::MissingSymbol => Some(inspection_detail("mir_inspection_missing_symbol", [])),
        Error::Source(error) => source_error_detail(error),
        Error::Type(error) => type_error_detail(error),
    }
}

fn semantic_value_detail(
    error: bray_symbols::SemanticValueStoreError,
) -> DiagnosticInspectionFailureDetail {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    let failure = bray_compilation::diagnostic_semantic_value_failure(error);
    let mut context = vec![text_field("cause", failure.as_str())];

    match failure {
        Failure::ForeignId {
            expected_store,
            actual_store,
        } => {
            context.push(text_field("expected_store", expected_store.to_string()));
            context.push(text_field("actual_store", actual_store.to_string()));
        }
        Failure::UnknownId { kind } | Failure::CapacityExhausted { kind } => {
            context.push(text_field("value_kind", kind));
        }
        Failure::GenericOwnerMismatch {
            expected_kind,
            expected,
            actual_kind,
            actual,
        } => {
            context.push(text_field("expected_kind", expected_kind));
            context.push(text_field("expected", expected.to_string()));
            context.push(text_field("actual_kind", actual_kind));
            context.push(text_field("actual", actual.to_string()));
        }
        Failure::InvalidDependencyVariable { depth, ordinal } => {
            context.push(text_field("depth", depth.to_string()));
            context.push(text_field("ordinal", ordinal.to_string()));
        }
        Failure::OpenSubstitution => {}
    }

    DiagnosticInspectionFailureDetail::new("inspection_semantic_value", context)
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
