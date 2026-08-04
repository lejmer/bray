use bray_messages::{BuildProgressMessage, BuildProgressMessageRenderer};

use super::model::BuildProgressStatus;
use crate::tack::model::TackBuildConfiguration;

pub(super) fn configuration_text(
    messages: BuildProgressMessageRenderer,
    configuration: TackBuildConfiguration,
) -> &'static str {
    match configuration {
        TackBuildConfiguration::Debug => messages.render(BuildProgressMessage::Debug),
        TackBuildConfiguration::Release => messages.render(BuildProgressMessage::Release),
    }
}

pub(super) fn max_column_width<'text>(values: impl IntoIterator<Item = &'text str>) -> usize {
    values
        .into_iter()
        .map(|value| value.chars().count())
        .max()
        .unwrap_or(0)
}

pub(super) const fn status_marker(status: BuildProgressStatus) -> &'static str {
    match status {
        BuildProgressStatus::Complete => "✓",
        BuildProgressStatus::Failed => "×",
    }
}
