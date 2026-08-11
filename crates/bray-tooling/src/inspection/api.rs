use bray_compilation::Compilation;
use bray_diagnostics::{
    DiagnosticBoundInspectionFailure, DiagnosticDeclarationInspectionFailure,
    DiagnosticInspectionFailure, DiagnosticInspectionOutputFormat, DiagnosticInspectionTarget,
    DiagnosticLoweredInspectionFailure, DiagnosticSourceInspectionFailure,
    DiagnosticSymbolInspectionFailure, DiagnosticSyntaxInspectionFailure,
    DiagnosticTokenInspectionFailure,
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
) -> Result<InspectionOutput, InspectionError> {
    super::lowered::render_mir_inspection(compilation, target, output_format).map_err(|error| {
        DiagnosticInspectionFailure::Mir {
            target: diagnostic_target(target),
            format: diagnostic_format(output_format),
            cause: lowered_inspection_failure(error),
        }
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

const fn source_inspection_failure(
    error: SourceInspectionRenderError,
) -> DiagnosticSourceInspectionFailure {
    match error {
        SourceInspectionRenderError::SourceIndex => DiagnosticSourceInspectionFailure::SourceIndex,
        SourceInspectionRenderError::Json => DiagnosticSourceInspectionFailure::Json,
    }
}

const fn token_inspection_failure(
    error: TokenInspectionRenderError,
) -> DiagnosticTokenInspectionFailure {
    match error {
        TokenInspectionRenderError::SourceIndex => DiagnosticTokenInspectionFailure::SourceIndex,
        TokenInspectionRenderError::TokenText => DiagnosticTokenInspectionFailure::TokenText,
        TokenInspectionRenderError::TriviaText => DiagnosticTokenInspectionFailure::TriviaText,
        TokenInspectionRenderError::Json => DiagnosticTokenInspectionFailure::Json,
    }
}

const fn syntax_inspection_failure(
    error: SyntaxInspectionRenderError,
) -> DiagnosticSyntaxInspectionFailure {
    match error {
        SyntaxInspectionRenderError::SourceIndex => DiagnosticSyntaxInspectionFailure::SourceIndex,
        SyntaxInspectionRenderError::SourceMismatch => {
            DiagnosticSyntaxInspectionFailure::SourceMismatch
        }
        SyntaxInspectionRenderError::TokenText => DiagnosticSyntaxInspectionFailure::TokenText,
        SyntaxInspectionRenderError::TriviaText => DiagnosticSyntaxInspectionFailure::TriviaText,
        SyntaxInspectionRenderError::TreeStructure => {
            DiagnosticSyntaxInspectionFailure::TreeStructure
        }
        SyntaxInspectionRenderError::Json => DiagnosticSyntaxInspectionFailure::Json,
    }
}

const fn declaration_inspection_failure(
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
        DeclarationInspectionRenderError::Json => DiagnosticDeclarationInspectionFailure::Json,
    }
}

const fn symbol_inspection_failure(
    error: SymbolInspectionRenderError,
) -> DiagnosticSymbolInspectionFailure {
    match error {
        SymbolInspectionRenderError::Declaration => DiagnosticSymbolInspectionFailure::Declaration,
        SymbolInspectionRenderError::Graph => DiagnosticSymbolInspectionFailure::Graph,
        SymbolInspectionRenderError::Json => DiagnosticSymbolInspectionFailure::Json,
        SymbolInspectionRenderError::Source => DiagnosticSymbolInspectionFailure::Source,
        SymbolInspectionRenderError::SourceIndex => DiagnosticSymbolInspectionFailure::SourceIndex,
        SymbolInspectionRenderError::Symbol => DiagnosticSymbolInspectionFailure::Symbol,
        SymbolInspectionRenderError::SymbolCycle => DiagnosticSymbolInspectionFailure::SymbolCycle,
        SymbolInspectionRenderError::SymbolFact => DiagnosticSymbolInspectionFailure::SymbolState,
        SymbolInspectionRenderError::Type => DiagnosticSymbolInspectionFailure::Type,
        SymbolInspectionRenderError::UnsupportedRelationship => {
            DiagnosticSymbolInspectionFailure::UnsupportedRelationship
        }
    }
}

const fn bound_inspection_failure(
    error: BoundInspectionRenderError,
) -> DiagnosticBoundInspectionFailure {
    match error {
        BoundInspectionRenderError::BoundFact => DiagnosticBoundInspectionFailure::BoundState,
        BoundInspectionRenderError::Json => DiagnosticBoundInspectionFailure::Json,
        BoundInspectionRenderError::MissingNode => DiagnosticBoundInspectionFailure::MissingNode,
        BoundInspectionRenderError::Source => DiagnosticBoundInspectionFailure::Source,
        BoundInspectionRenderError::SourceIndex => DiagnosticBoundInspectionFailure::SourceIndex,
        BoundInspectionRenderError::StorageFact => DiagnosticBoundInspectionFailure::StorageState,
        BoundInspectionRenderError::Symbol => DiagnosticBoundInspectionFailure::Symbol,
        BoundInspectionRenderError::SymbolFact => DiagnosticBoundInspectionFailure::SymbolState,
        BoundInspectionRenderError::Type => DiagnosticBoundInspectionFailure::Type,
        BoundInspectionRenderError::TypeFact => DiagnosticBoundInspectionFailure::TypeState,
        BoundInspectionRenderError::SelectionFact => {
            DiagnosticBoundInspectionFailure::SelectionState
        }
        BoundInspectionRenderError::Selection => DiagnosticBoundInspectionFailure::Selection,
    }
}

const fn lowered_inspection_failure(
    error: LoweredInspectionRenderError,
) -> DiagnosticLoweredInspectionFailure {
    match error {
        LoweredInspectionRenderError::Json => DiagnosticLoweredInspectionFailure::Json,
        LoweredInspectionRenderError::LoweringFact => {
            DiagnosticLoweredInspectionFailure::LoweringState
        }
        LoweredInspectionRenderError::Model => DiagnosticLoweredInspectionFailure::Model,
        LoweredInspectionRenderError::Source => DiagnosticLoweredInspectionFailure::Source,
        LoweredInspectionRenderError::SymbolFact => DiagnosticLoweredInspectionFailure::SymbolState,
        LoweredInspectionRenderError::UnitFact => DiagnosticLoweredInspectionFailure::UnitState,
    }
}
