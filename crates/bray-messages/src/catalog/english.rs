mod argument;
mod build_progress;
mod diagnostics;
#[cfg(test)]
mod guard;
mod interface;
mod label;
mod language_server;
mod profile;
mod related;
mod suggestion;
mod test_report;
#[cfg(test)]
mod tests;

pub(crate) const INTERNAL_COMPILER_ERROR: &str = "internal compiler error";

pub(crate) fn format_internal_compiler_error(detail: impl AsRef<str>) -> String {
    format!("{INTERNAL_COMPILER_ERROR}: {}", detail.as_ref())
}

pub(crate) use argument::{format_source_location, format_source_span, format_value};
pub(crate) use build_progress::{
    action as build_progress_action, duration as build_progress_duration,
    fields as build_progress_fields, heading as build_progress_heading,
    operation as build_progress_operation, percentage as build_progress_percentage,
    unit_count as build_progress_unit_count,
};
pub(crate) use diagnostics::{
    diagnostic_template, label_heading, note_heading, note_kind, note_template,
    related_location_heading, severity_label,
};
#[cfg(test)]
pub(crate) use guard::{forbidden_internal_term, forbidden_ordinary_diagnostic_term};
pub(crate) use label::{style as label_style, template as label_template};
pub(crate) use language_server::message as language_server_message;
pub(crate) use profile::{
    comparison as compiler_profile_comparison, summary as compiler_profile_summary,
};
pub(crate) use related::template as related_location_template;
pub(crate) use suggestion::template as suggestion_template;
pub(crate) use test_report::{
    activity_operation as test_report_activity_operation,
    captured_stream as test_report_captured_stream, duration as test_report_duration,
    fields as test_report_fields, heading as test_report_heading,
    result_operation as test_report_result_operation, summary_counts as test_report_summary_counts,
    summary_operation as test_report_summary_operation,
};
