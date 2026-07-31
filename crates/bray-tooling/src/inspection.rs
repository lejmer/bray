//! Human-readable and machine-readable compiler fact inspection.

mod api;
mod bound;
mod declaration;
mod lowered;
mod source;
mod support;
mod symbol;
mod syntax;
mod token;
mod types;
mod unit;

pub use api::{
    InspectionError, render_bound_inspection,
    render_declaration_inspection, render_lowered_inspection,
    render_mir_inspection, render_source_inspection,
    render_symbol_inspection, render_syntax_inspection,
    render_token_inspection,
};
pub use support::InspectionOutput;
pub use types::format_semantic_type;
pub(crate) use support::{
    InspectionSourceError, InspectionSources, InspectionSymbolIdentity,
    InspectionSyntaxAnchor, InspectionTrivia, InspectionTriviaError,
    TreeWriter, escaped_text, location_for_range, location_range_text,
    location_start_text, push_text_diagnostic, quoted_text, range_text,
    trivia_entries, trivia_summary,
};
pub(crate) use types::{InspectionType, TypeInspectionError};
