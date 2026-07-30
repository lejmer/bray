//! Driver output rendering and shared output data.

mod diagnostic;
mod location;
mod origin;
mod path;
mod style;

pub(crate) use diagnostic::{
    DiagnosticJson, diagnostic_jsons, write_diagnostic_groups,
    write_driver_output, write_driver_output_error,
};
pub(crate) use location::{SourceLocationOutput, TextRangeOutput};
pub(crate) use origin::SourceOriginOutput;
pub(crate) use path::path_to_output_string;
pub(crate) use style::{
    clap_styles, color_bright_text, color_frame_text, color_note_heading,
    color_severity_label, render_styled_text,
};
