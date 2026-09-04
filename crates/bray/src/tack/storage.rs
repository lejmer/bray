use std::path::Path;
use std::process::ExitCode;

use bray_diagnostics::{
    DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, DiagnosticProjectSelectionProblem, SeverityKind,
};
use bray_emitter::{StorageCategory, StorageEntryReport, StoragePolicy, StorageSelection};
use bray_messages::{StorageReportMessage, StorageReportMessageRenderer};
use bray_project::ProjectGraph;
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::TargetIdentity;
use bray_tooling::OutputFormat;
use serde::Serialize;

use super::error::{operation_diagnostics, selection_diagnostics};
use super::model::{TackStorageAction, TackStorageOptions};
use super::output::failure;
use super::result::TackRunResult;

pub(crate) fn run_storage_command(
    workspace: &Path,
    graph: &ProjectGraph,
    options: TackStorageOptions,
    action: TackStorageAction,
    output_format: OutputFormat,
) -> TackRunResult {
    let selection = match storage_selection(graph, options) {
        Ok(selection) => selection,
        Err(diagnostics) => return failure(diagnostics, output_format),
    };

    let root = graph.output_root().beneath(workspace);

    let rows = match action {
        TackStorageAction::Clean | TackStorageAction::PreviewClean => bray_emitter::clean_storage(
            &root,
            &selection,
            action == TackStorageAction::PreviewClean,
            &|| false,
        ),
        TackStorageAction::Report => {
            bray_emitter::inspect_storage(&root, &selection, StoragePolicy::default(), &|| false)
        }
    };

    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            return failure(
                DiagnosticBag::single(
                    error.into_diagnostic(DiagnosticId::new(0), SeverityKind::Error),
                ),
                output_format,
            );
        }
    };

    let report = StorageReport::new(&rows, action);

    let output = match output_format {
        OutputFormat::Text => report.text(),
        OutputFormat::Json => match serde_json::to_string_pretty(&report) {
            Ok(json) => format!("{json}\n"),
            Err(error) => {
                return failure(
                    operation_diagnostics(DiagnosticProjectCommandFailure::Document {
                        operation: DiagnosticProjectOperation::StorageReportJson,
                        path: Some(root),
                        problem: DiagnosticDocumentParseKind::Serialization,
                        detail: Some(error.to_string()),
                    }),
                    output_format,
                );
            }
        },
    };

    TackRunResult::with_output(
        ExitCode::SUCCESS,
        DiagnosticBag::new(),
        output_format,
        output,
        String::new(),
    )
}

fn storage_selection(
    graph: &ProjectGraph,
    options: TackStorageOptions,
) -> Result<StorageSelection, DiagnosticBag> {
    let product = options
        .product
        .map(|value| {
            let identity = value.rsplit_once('/').and_then(|(package, name)| {
                ProductIdentity::try_new(PackageIdentity::try_new(package)?, name)
            });

            identity.ok_or_else(|| {
                selection_diagnostics(DiagnosticProjectSelectionProblem::InvalidProductIdentity(
                    value,
                ))
            })
        })
        .transpose()?;

    let target = options
        .target
        .map(|value| {
            graph
                .targets()
                .iter()
                .find(|target| target.name() == value)
                .map(|target| target.identity().clone())
                .or_else(|| TargetIdentity::try_new(value.as_str()))
                .ok_or_else(|| {
                    selection_diagnostics(DiagnosticProjectSelectionProblem::UnknownTarget(value))
                })
        })
        .transpose()?;

    Ok(StorageSelection {
        product,
        target,
        profile: options.profile,
        toolchain: options.toolchain,
        kind: options.kind,
    })
}

#[derive(Serialize)]
struct StorageReport<'a> {
    kind: &'static str,
    clean: bool,
    dry_run: bool,
    entries: Vec<StorageRow<'a>>,
}

impl<'a> StorageReport<'a> {
    fn new(rows: &'a [StorageEntryReport], action: TackStorageAction) -> Self {
        Self {
            kind: "storage_report",
            clean: action != TackStorageAction::Report,
            dry_run: action == TackStorageAction::PreviewClean,
            entries: rows.iter().map(StorageRow::new).collect(),
        }
    }

    fn text(&self) -> String {
        let renderer = StorageReportMessageRenderer::english();

        if self.entries.is_empty() {
            return renderer.render(StorageReportMessage::Empty).to_owned();
        }

        let mut output = renderer.render(StorageReportMessage::Heading).to_owned();

        for row in &self.entries {
            let bytes = row.bytes.map_or_else(
                || {
                    renderer
                        .render(StorageReportMessage::ChangingBytes)
                        .to_owned()
                },
                |bytes| bytes.to_string(),
            );

            let shared = row.shared_bytes.map_or_else(
                || {
                    renderer
                        .render(StorageReportMessage::ChangingBytes)
                        .to_owned()
                },
                |bytes| bytes.to_string(),
            );

            let status = if row.active {
                StorageReportMessage::KeptActive
            } else if row.removed {
                StorageReportMessage::Removed
            } else if self.dry_run {
                StorageReportMessage::WouldRemove
            } else {
                StorageReportMessage::Retained
            };

            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                renderer.render(category_message(row.category)),
                row.product.as_deref().unwrap_or("-"),
                row.target.as_deref().unwrap_or("-"),
                row.profile.as_deref().unwrap_or("-"),
                row.toolchain.as_deref().unwrap_or("-"),
                bytes,
                shared,
                renderer.render(status),
            ));
        }

        output
    }
}

#[derive(Serialize)]
struct StorageRow<'a> {
    category: StorageCategory,
    product: Option<String>,
    target: Option<&'a str>,
    profile: Option<&'a str>,
    toolchain: Option<&'a str>,
    bytes: Option<u64>,
    shared_bytes: Option<u64>,
    active: bool,
    removed: bool,
}

impl<'a> StorageRow<'a> {
    fn new(row: &'a StorageEntryReport) -> Self {
        Self {
            category: row.category,
            product: row.product.as_ref().map(ToString::to_string),
            target: row.target.as_ref().map(|target| target.as_str()),
            profile: row.profile.as_deref(),
            toolchain: row.toolchain.as_deref(),
            bytes: row.bytes,
            shared_bytes: row.shared_bytes,
            active: row.active,
            removed: row.removed,
        }
    }
}

const fn category_message(category: StorageCategory) -> StorageReportMessage {
    match category {
        StorageCategory::CurrentOutputs => StorageReportMessage::CurrentOutputs,
        StorageCategory::RetainedRerun => StorageReportMessage::RetainedRerun,
        StorageCategory::RetainedHistory => StorageReportMessage::RetainedHistory,
        StorageCategory::ActiveWork => StorageReportMessage::ActiveWork,
        StorageCategory::ReusableCache => StorageReportMessage::ReusableCache,
        StorageCategory::Reclaimable => StorageReportMessage::Reclaimable,
    }
}
