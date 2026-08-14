use std::fmt::Write as _;

use super::format::{grouped, milliseconds};
use super::html::BoundedHtml;
use super::model::WorkloadBatching;

pub(super) fn batching_detail(html: &mut BoundedHtml, batching: &WorkloadBatching) {
    match batching {
        WorkloadBatching::SingleExecution => {
            html.push_str("<dl><dt>Batch selection</dt><dd>single execution</dd></dl>");
        }
        WorkloadBatching::Calibrated {
            seed_inner_iterations,
            target_interval_nanoseconds,
            bray_samples_nanoseconds,
            rust_samples_nanoseconds,
            cpp_samples_nanoseconds,
            selected_inner_iterations,
        } => {
            let samples = |values: &[u64]| {
                values
                    .iter()
                    .map(|value| grouped(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let _ = write!(
                html,
                "<dl><dt>Batch selection</dt><dd>calibrated outside measured samples</dd>\
                <dt>Calibration seed</dt><dd>{} iterations</dd><dt>Target interval</dt>\
                <dd>{}</dd><dt>Selected batch</dt><dd>{} iterations</dd>\
                <dt>Bray calibration intervals</dt><dd>{} ns</dd>\
                <dt>Rust calibration intervals</dt><dd>{} ns</dd>\
                <dt>C++ calibration intervals</dt><dd>{} ns</dd></dl>",
                grouped(*seed_inner_iterations),
                milliseconds(*target_interval_nanoseconds),
                grouped(*selected_inner_iterations),
                samples(bray_samples_nanoseconds),
                samples(rust_samples_nanoseconds),
                samples(cpp_samples_nanoseconds),
            );
        }
    }
}
