use crate::{
    BuildProgressAction, BuildProgressConfiguration, BuildProgressLineKind, BuildProgressOperation,
    ProgressField,
};

const PACKAGE_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Subject,
    ProgressField::Path,
    ProgressField::Count,
    ProgressField::Duration,
];

const ACTIVE_PRODUCT_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Subject,
    ProgressField::Bar,
    ProgressField::Percentage,
    ProgressField::Count,
    ProgressField::Duration,
];

const FINISHED_PRODUCT_FIELDS: &[ProgressField] = &[
    ProgressField::Operation,
    ProgressField::Subject,
    ProgressField::Path,
    ProgressField::Count,
    ProgressField::Duration,
];

pub(crate) const fn fields(kind: BuildProgressLineKind) -> &'static [ProgressField] {
    match kind {
        BuildProgressLineKind::Package => PACKAGE_FIELDS,
        BuildProgressLineKind::ActiveProduct => ACTIVE_PRODUCT_FIELDS,
        BuildProgressLineKind::FinishedProduct => FINISHED_PRODUCT_FIELDS,
    }
}

pub(crate) fn heading(product: &str, configuration: BuildProgressConfiguration) -> String {
    format!("Building {product} [{}]", configuration_text(configuration))
}

pub(crate) const fn operation(operation: BuildProgressOperation) -> &'static str {
    match operation {
        BuildProgressOperation::Building => "Building",
        BuildProgressOperation::Compiling => "Compiling",
        BuildProgressOperation::Compiled => "Compiled",
        BuildProgressOperation::Finished => "Finished",
        BuildProgressOperation::Failed => "Failed",
    }
}

pub(crate) const fn action(action: BuildProgressAction) -> &'static str {
    match action {
        BuildProgressAction::CheckInterface => "Checking dependency interface",
        BuildProgressAction::ProduceArtifacts => "Producing product artifacts",
    }
}

pub(crate) fn unit_count(completed: u64, total: u64) -> String {
    format!("{completed}/{total} units")
}

pub(crate) fn duration(milliseconds: u128) -> String {
    format!("{milliseconds} ms")
}

pub(crate) fn percentage(value: u64) -> String {
    format!("{value}%")
}

const fn configuration_text(configuration: BuildProgressConfiguration) -> &'static str {
    match configuration {
        BuildProgressConfiguration::Debug => "debug",
        BuildProgressConfiguration::Release => "release",
    }
}
