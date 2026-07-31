//! Shared command diagnostic rendering and output data.

mod diagnostic;
mod location;
mod origin;
mod path;
mod style;

#[cfg(feature = "analysis")]
pub(crate) use diagnostic::{DiagnosticJson, diagnostic_jsons};
pub use diagnostic::{write_diagnostic_groups, write_diagnostics};
pub(crate) use location::SourceLocationOutput;
#[cfg(feature = "analysis")]
pub(crate) use location::TextRangeOutput;
pub(crate) use origin::SourceOriginOutput;
pub(crate) use path::path_to_output_string;
pub use style::{clap_styles, render_styled_text};
pub(crate) use style::{
    color_bright_text, color_frame_text, color_note_heading, color_severity_label,
};
