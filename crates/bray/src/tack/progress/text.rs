use bray_messages::{
    BuildProgressAction as MessageAction, BuildProgressConfiguration, BuildProgressField,
    BuildProgressLineKind, BuildProgressMessageRenderer, BuildProgressOperation,
};

use super::model::{BuildProgressAction, BuildProgressStatus};
use crate::tack::model::TackBuildConfiguration;

pub(super) fn message_configuration(
    configuration: TackBuildConfiguration,
) -> BuildProgressConfiguration {
    match configuration {
        TackBuildConfiguration::Debug => BuildProgressConfiguration::Debug,
        TackBuildConfiguration::Release => BuildProgressConfiguration::Release,
    }
}

pub(super) fn message_action(action: BuildProgressAction) -> MessageAction {
    match action {
        BuildProgressAction::CheckInterface => MessageAction::CheckInterface,
        BuildProgressAction::ProduceArtifacts => MessageAction::ProduceArtifacts,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "localized progress lines carry one value for every visible report column"
)]
pub(super) fn render_line(
    messages: BuildProgressMessageRenderer,
    kind: BuildProgressLineKind,
    operation: BuildProgressOperation,
    subject: &str,
    path: &str,
    completed: u64,
    total: u64,
    duration_milliseconds: u128,
) -> String {
    let mut fields = Vec::new();

    for field in messages.fields(kind) {
        let value = match field {
            BuildProgressField::Operation => messages.operation(operation).to_owned(),
            BuildProgressField::Subject => subject.to_owned(),
            BuildProgressField::Path => path.to_owned(),
            BuildProgressField::UnitCount => messages.unit_count(completed, total),
            BuildProgressField::Duration => messages.duration(duration_milliseconds),
            BuildProgressField::Bar | BuildProgressField::Percentage => continue,
        };

        fields.push(value);
    }

    fields.join(" ")
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
