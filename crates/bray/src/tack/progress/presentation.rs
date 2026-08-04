use std::time::Duration;

use bray_messages::ProgressField;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::tack) enum ProgressVisualState {
    Waiting,
    Active,
    Complete,
    Failed,
}

pub(in crate::tack) fn terminal_progress() -> MultiProgress {
    MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(20))
}

pub(in crate::tack) fn terminal_heading(
    progress: &MultiProgress,
    message: String,
) -> ProgressBar {
    let heading = progress.add(ProgressBar::new(0));

    heading.set_style(progress_style("{msg:.bold}"));
    heading.set_message(message);

    heading
}

#[expect(
    clippy::too_many_arguments,
    reason = "localized progress lines carry formatters for each dynamic field"
)]
pub(in crate::tack) fn terminal_line_style<Count, DurationText, Percentage>(
    fields: &[ProgressField],
    state: ProgressVisualState,
    path: String,
    detail: String,
    count: Count,
    duration: DurationText,
    percentage: Percentage,
) -> ProgressStyle
where
    Count: Clone + Fn(u64, u64) -> String + Send + Sync + 'static,
    DurationText: Clone + Fn(u128) -> String + Send + Sync + 'static,
    Percentage: Clone + Fn(u64) -> String + Send + Sync + 'static,
{
    let color = state_color(state);

    let fields = fields
        .iter()
        .map(|field| field_template(*field, color))
        .collect::<Vec<_>>()
        .join(" ");

    let template = format!("   {} {fields}", state_marker(state));

    let style = progress_style(&template)
        .with_key(
            "path",
            move |_: &ProgressState, writer: &mut dyn std::fmt::Write| {
                let _ = writer.write_str(&path);
            },
        )
        .with_key(
            "count",
            move |state: &ProgressState, writer: &mut dyn std::fmt::Write| {
                let _ = writer.write_str(&count(
                    state.pos(),
                    state.len().unwrap_or_default(),
                ));
            },
        )
        .with_key(
            "detail",
            move |_: &ProgressState, writer: &mut dyn std::fmt::Write| {
                let _ = writer.write_str(&detail);
            },
        )
        .with_key(
            "duration",
            move |state: &ProgressState, writer: &mut dyn std::fmt::Write| {
                let _ = writer.write_str(&duration(state.elapsed().as_millis()));
            },
        )
        .with_key(
            "percentage",
            move |state: &ProgressState, writer: &mut dyn std::fmt::Write| {
                let value = match state.len().unwrap_or_default() {
                    0 => 100,
                    total => state.pos().saturating_mul(100) / total,
                };

                let _ = writer.write_str(&percentage(value));
            },
        )
        .progress_chars("━╸ ");

    match state {
        ProgressVisualState::Active => {
            style.tick_strings(&["◐", "◓", "◑", "◒", "◐"])
        }
        ProgressVisualState::Waiting => style.tick_strings(&["○", "○"]),
        ProgressVisualState::Complete => style.tick_strings(&["✓", "✓"]),
        ProgressVisualState::Failed => style.tick_strings(&["×", "×"]),
    }
}

pub(in crate::tack) fn render_plain_line(
    fields: &[ProgressField],
    state: ProgressVisualState,
    mut value: impl FnMut(ProgressField) -> Option<String>,
) -> String {
    let fields = fields
        .iter()
        .filter_map(|field| value(*field))
        .collect::<Vec<_>>()
        .join(" ");

    format!("   {} {fields}", plain_marker(state))
}

pub(in crate::tack) const fn plain_marker(state: ProgressVisualState) -> &'static str {
    match state {
        ProgressVisualState::Waiting => "○",
        ProgressVisualState::Active => "◐",
        ProgressVisualState::Complete => "✓",
        ProgressVisualState::Failed => "×",
    }
}

pub(in crate::tack) fn max_column_width<'text>(
    values: impl IntoIterator<Item = &'text str>,
) -> usize {
    values
        .into_iter()
        .map(|value| value.chars().count())
        .max()
        .unwrap_or(0)
}

pub(in crate::tack) fn padded(value: &str, width: usize) -> String {
    format!("{value:<width$}")
}

pub(in crate::tack) fn duration_milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub(in crate::tack) fn empty_count(_: u64, _: u64) -> String {
    String::new()
}

pub(in crate::tack) fn empty_percentage(_: u64) -> String {
    String::new()
}

fn field_template(field: ProgressField, color: &str) -> String {
    match field {
        ProgressField::Operation => format!("{{prefix:.{color}.bold}}"),
        ProgressField::Subject => String::from("{msg}"),
        ProgressField::Path => String::from("{path}"),
        ProgressField::Bar => String::from("{bar:20.cyan/dim}"),
        ProgressField::Percentage => String::from("{percentage}"),
        ProgressField::Count => String::from("{count}"),
        ProgressField::Detail => String::from("{detail}"),
        ProgressField::Duration => String::from("{duration:.dim}"),
    }
}

fn progress_style(template: &str) -> ProgressStyle {
    match ProgressStyle::with_template(template) {
        Ok(style) => style,
        Err(error) => panic!("validated progress template must compile: {error}"),
    }
}

const fn state_marker(state: ProgressVisualState) -> &'static str {
    match state {
        ProgressVisualState::Waiting => "{spinner:.white}",
        ProgressVisualState::Active => "{spinner:.cyan.bold}",
        ProgressVisualState::Complete => "{spinner:.green.bold}",
        ProgressVisualState::Failed => "{spinner:.red.bold}",
    }
}

const fn state_color(state: ProgressVisualState) -> &'static str {
    match state {
        ProgressVisualState::Waiting => "white",
        ProgressVisualState::Active => "cyan",
        ProgressVisualState::Complete => "green",
        ProgressVisualState::Failed => "red",
    }
}

#[cfg(test)]
mod tests {
    use bray_messages::ProgressField;

    use super::{ProgressVisualState, terminal_line_style};

    #[test]
    fn terminal_styles_share_canonical_markers_and_spinner_frames() {
        let fields = [ProgressField::Operation, ProgressField::Subject];
        let count = |completed, total| format!("{completed}/{total}");
        let duration = |milliseconds| format!("{milliseconds} ms");
        let percentage = |value| format!("{value}%");

        let active = terminal_line_style(
            &fields,
            ProgressVisualState::Active,
            String::new(),
            String::new(),
            count,
            duration,
            percentage,
        );

        assert_eq!(active.get_tick_str(0), "◐");
        assert_eq!(active.get_tick_str(1), "◓");
        assert_eq!(active.get_tick_str(2), "◑");
        assert_eq!(active.get_tick_str(3), "◒");

        for (state, marker) in [
            (ProgressVisualState::Waiting, "○"),
            (ProgressVisualState::Complete, "✓"),
            (ProgressVisualState::Failed, "×"),
        ] {
            let style = terminal_line_style(
                &fields,
                state,
                String::new(),
                String::new(),
                count,
                duration,
                percentage,
            );

            assert_eq!(style.get_tick_str(0), marker);
            assert_eq!(style.get_final_tick_str(), marker);
        }
    }
}
