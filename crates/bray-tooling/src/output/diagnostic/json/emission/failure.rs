use serde::Serialize;

use super::super::DiagnosticInterfaceSymbolIdentityJson;
use super::super::{
    DiagnosticArtifactDigestJson, DiagnosticExternalToolExitJson, DiagnosticOutputSinkJson,
    DiagnosticPathJson, DiagnosticProblemJson,
};
use super::context::{
    codegen_failure_context, evaluation_failure_context, link_plan_failure_context,
    package_interface_failure_context, planning_failure_context, staging_failure_context,
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
            Failure::Evaluation(failure) => evaluation_failure_context(failure),
            Failure::TestCatalog(failure) => match failure {
                bray_diagnostics::DiagnosticTestCatalogFailure::UnsupportedVersion(version) => {
                    vec![count_u64_field("version", u64::from(*version))]
                }
                bray_diagnostics::DiagnosticTestCatalogFailure::Io
                | bray_diagnostics::DiagnosticTestCatalogFailure::Malformed
                | bray_diagnostics::DiagnosticTestCatalogFailure::ResourceLimit => Vec::new(),
            },
            Failure::Codegen(failure) => codegen_failure_context(failure),
            Failure::Staging(failure) => staging_failure_context(failure),
            Failure::LinkPlan(failure) => link_plan_failure_context(failure),
            Failure::MissingContribution(artifact)
            | Failure::InvalidContribution(artifact)
            | Failure::Publication(artifact) => vec![artifact_field("artifact", *artifact)],
            Failure::InvalidRequest | Failure::Linking | Failure::IncompleteProduct => Vec::new(),
        };

        Self {
            category: failure.category(),
            reason: failure.reason(),
            context,
        }
    }

    pub(in crate::output::diagnostic::json) fn from_evaluation(
        failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
    ) -> Self {
        Self {
            category: "evaluation",
            reason: failure.as_str(),
            context: evaluation_failure_context(failure),
        }
    }
}

#[cfg(feature = "analysis")]
pub fn diagnostic_evaluation_failure_json(
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(DiagnosticEmissionFailureJson::from_evaluation(failure))
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticEmissionFieldJson {
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
    Boolean(bool),
    Count(u64),
    DebugInformationMode(&'static str),
    DebugOutputMode(&'static str),
    IoErrorKind(&'static str),
    Identity(String),
    IdentityList(Vec<String>),
    Evaluation(Box<DiagnosticEmissionFailureJson>),
    ExternalToolExit(DiagnosticExternalToolExitJson),
    InterfaceSymbolIdentity(DiagnosticInterfaceSymbolIdentityJson),
    InterfaceSymbolGraphProblem(DiagnosticProblemJson),
    InterfaceValidationFailure(DiagnosticProblemJson),
    LinkInputKind(&'static str),
    LinkedArtifactKind(&'static str),
    LinkedProductKind(&'static str),
    Natural(String),
    Path(DiagnosticPathJson),
    OutputSink(DiagnosticOutputSinkJson),
    Problem(DiagnosticProblemJson),
    ProductKind(&'static str),
    Signed(i64),
    Text(String),
    TextList(Vec<String>),
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
    count_u64_field(name, u64::from(value))
}

pub(super) fn count_u64_field(name: &'static str, value: u64) -> DiagnosticEmissionFieldJson {
    field(name, DiagnosticEmissionFieldValueJson::Count(value))
}

pub(super) fn count_usize_field(
    name: &'static str,
    value: usize,
) -> DiagnosticEmissionFieldJson {
    count_u64_field(name, u64::try_from(value).unwrap_or(u64::MAX))
}

pub(in crate::output::diagnostic::json) fn text_field(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticEmissionFieldJson {
    field(name, DiagnosticEmissionFieldValueJson::Text(value.into()))
}
