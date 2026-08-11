use serde::Serialize;

use super::super::{DiagnosticArtifactDigestJson, DiagnosticOutputSinkJson};
use super::context::{
    codegen_failure_context, link_plan_failure_context, package_interface_failure_context,
    planning_failure_context, staging_failure_context,
};

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticEmissionFailureJson {
    category: &'static str,
    reason: &'static str,
    context: Vec<DiagnosticEmissionFieldJson>,
}

impl DiagnosticEmissionFailureJson {
    pub(in crate::output::diagnostic::json) fn from_failure(
        failure: &bray_diagnostics::DiagnosticEmissionFailure,
    ) -> Self {
        use bray_diagnostics::DiagnosticEmissionFailure as Failure;

        let context = match failure {
            Failure::Planning(failure) => planning_failure_context(failure),
            Failure::PackageInterface(failure) => package_interface_failure_context(failure),
            Failure::Codegen(failure) => codegen_failure_context(failure),
            Failure::Staging(failure) => staging_failure_context(failure),
            Failure::LinkPlan(failure) => link_plan_failure_context(failure),
            Failure::MissingContribution(artifact)
            | Failure::InvalidContribution(artifact)
            | Failure::Publication(artifact) => vec![artifact_field("artifact", *artifact)],
            Failure::InvalidRequest
            | Failure::Evaluation(_)
            | Failure::Linking
            | Failure::IncompleteProduct => Vec::new(),
        };

        Self {
            category: failure.category(),
            reason: failure.reason(),
            context,
        }
    }
}

#[derive(Serialize)]
pub(super) struct DiagnosticEmissionFieldJson {
    name: &'static str,
    value: DiagnosticEmissionFieldValueJson,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(super) enum DiagnosticEmissionFieldValueJson {
    Artifact(DiagnosticEmissionArtifactJson),
    ArtifactDigest(DiagnosticArtifactDigestJson),
    ArtifactKind(&'static str),
    ArtifactRequirement(&'static str),
    AssemblySyntax(&'static str),
    Count(u64),
    DebugInformationMode(&'static str),
    DebugOutputMode(&'static str),
    IoErrorKind(&'static str),
    LinkInputKind(&'static str),
    LinkedArtifactKind(&'static str),
    LinkedProductKind(&'static str),
    OutputSink(DiagnosticOutputSinkJson),
    Path(String),
    ProductKind(&'static str),
    Text(String),
}

#[derive(Serialize)]
pub(super) struct DiagnosticEmissionArtifactJson {
    kind: &'static str,
    ordinal: u32,
}

pub(super) fn field(
    name: &'static str,
    value: DiagnosticEmissionFieldValueJson,
) -> DiagnosticEmissionFieldJson {
    DiagnosticEmissionFieldJson { name, value }
}

pub(super) fn artifact_field(
    name: &'static str,
    artifact: bray_diagnostics::DiagnosticEmissionArtifact,
) -> DiagnosticEmissionFieldJson {
    field(
        name,
        DiagnosticEmissionFieldValueJson::Artifact(DiagnosticEmissionArtifactJson {
            kind: artifact.kind().as_str(),
            ordinal: artifact.ordinal(),
        }),
    )
}

pub(super) fn digest_field(
    name: &'static str,
    digest: &bray_diagnostics::DiagnosticArtifactDigest,
) -> DiagnosticEmissionFieldJson {
    field(
        name,
        DiagnosticEmissionFieldValueJson::ArtifactDigest(
            DiagnosticArtifactDigestJson::from_digest(digest),
        ),
    )
}

pub(super) fn count_field(name: &'static str, value: u32) -> DiagnosticEmissionFieldJson {
    field(
        name,
        DiagnosticEmissionFieldValueJson::Count(u64::from(value)),
    )
}

pub(super) fn text_field(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticEmissionFieldJson {
    field(name, DiagnosticEmissionFieldValueJson::Text(value.into()))
}
