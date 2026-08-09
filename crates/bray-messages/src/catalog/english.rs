mod argument;
mod build_progress;
mod diagnostics;
mod interface;
mod label;
mod language_server;
mod profile;
mod test_report;

pub(crate) use argument::{format_source_location, format_source_span, format_value};
pub(crate) use build_progress::{
    action as build_progress_action, duration as build_progress_duration,
    fields as build_progress_fields, heading as build_progress_heading,
    operation as build_progress_operation, percentage as build_progress_percentage,
    unit_count as build_progress_unit_count,
};
pub(crate) use diagnostics::{
    diagnostic_template, note_heading, note_kind, note_template, severity_label,
};
pub(crate) use label::{style as label_style, template as label_template};
pub(crate) use language_server::message as language_server_message;
pub(crate) use profile::{
    heading as compiler_profile_heading, queries as compiler_profile_queries,
    trace as compiler_profile_trace,
};
pub(crate) use test_report::{
    activity_operation as test_report_activity_operation,
    captured_stream as test_report_captured_stream, duration as test_report_duration,
    fields as test_report_fields, heading as test_report_heading,
    result_operation as test_report_result_operation, summary_counts as test_report_summary_counts,
    summary_operation as test_report_summary_operation,
};
