//! Human-readable and machine-readable compiler fact inspection.

mod bound;
mod declaration;
mod source;
mod support;
mod symbol;
mod syntax;
mod token;
mod types;

pub(crate) use bound::render_bound_inspection;
pub(crate) use declaration::render_declaration_inspection;
pub(crate) use source::render_source_inspection;
pub(crate) use support::{
    InspectionOutput, InspectionSourceError, InspectionSources, InspectionSymbolIdentity,
    InspectionSyntaxAnchor, InspectionTrivia, InspectionTriviaError, TreeWriter, escaped_text,
    location_for_range, location_range_text, location_start_text, push_text_diagnostic,
    quoted_text, range_text, trivia_entries, trivia_summary,
};
pub(crate) use symbol::render_symbol_inspection;
pub(crate) use syntax::render_syntax_inspection;
pub(crate) use token::render_token_inspection;
pub(crate) use types::{InspectionType, TypeInspectionError};
