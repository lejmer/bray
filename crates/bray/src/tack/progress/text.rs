use bray_messages::{
    BuildProgressAction as MessageAction, BuildProgressConfiguration, BuildProgressLineKind,
    BuildProgressMessageRenderer, BuildProgressOperation, ProgressField,
};

use super::model::{BuildProgressAction, BuildProgressStatus};
use super::presentation::{ProgressVisualState, render_plain_line};
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
    status: BuildProgressStatus,
    operation: BuildProgressOperation,
    subject: &str,
    path: &str,
    completed: u64,
    total: u64,
    duration_milliseconds: u128,
) -> String {
    render_plain_line(messages.fields(kind), visual_state(status), |field| {
        match field {
            ProgressField::Operation => Some(messages.operation(operation).to_owned()),
            ProgressField::Subject => Some(subject.to_owned()),
            ProgressField::Path => Some(path.to_owned()),
            ProgressField::Count => Some(messages.unit_count(completed, total)),
            ProgressField::Duration => Some(messages.duration(duration_milliseconds)),
            ProgressField::Bar | ProgressField::Percentage | ProgressField::Detail => None,
        }
    })
}

const fn visual_state(status: BuildProgressStatus) -> ProgressVisualState {
    match status {
        BuildProgressStatus::Complete => ProgressVisualState::Complete,
        BuildProgressStatus::Failed => ProgressVisualState::Failed,
    }
}
