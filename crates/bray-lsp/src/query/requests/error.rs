use crate::workspace::WorkspaceError;
use bray_bound_tree::BoundSourceAnchor;
use bray_compilation::FactQueryError;
use bray_source::SourceId;
use bray_symbols::CallableSymbolId;

#[derive(Debug)]
pub(crate) enum QueryError {
    Cancelled,
    Evaluation(FactQueryError),
    MissingSyntax(SourceId),
    MissingLocation(BoundSourceAnchor),
    MissingCallableParameters(CallableSymbolId),
    MissingSource {
        source: SourceId,
        expected_version: Option<u64>,
    },
    Workspace(WorkspaceError),
    Serialization(serde_json::Error),
}

impl QueryError {
    pub(crate) fn data(&self) -> serde_json::Value {
        use serde_json::json;

        match self {
            Self::Cancelled => json!({ "reason": "cancelled" }),
            Self::Evaluation(error) => bray_tooling::diagnostic_evaluation_failure_json(
                &error.diagnostic_evaluation_failure(),
            )
            .unwrap_or_else(|cause| {
                json!({
                    "reason": "evaluation_serialization",
                    "cause": cause.to_string(),
                    "evaluation": format!("{error:?}"),
                })
            }),
            Self::MissingSyntax(source) => json!({
                "reason": "missing_syntax",
                "source": source.raw(),
            }),
            Self::MissingLocation(anchor) => json!({
                "reason": "missing_location",
                "anchor": format!("{anchor:?}"),
            }),
            Self::MissingCallableParameters(callable) => json!({
                "reason": "missing_callable_parameters",
                "callable": format!("{callable:?}"),
            }),
            Self::MissingSource {
                source,
                expected_version,
            } => json!({
                "reason": "missing_source",
                "source": source.raw(),
                "expected_version": expected_version,
            }),
            Self::Workspace(error) => error.data(),
            Self::Serialization(cause) => json!({
                "reason": "serialization",
                "cause": cause.to_string(),
            }),
        }
    }
}

impl From<FactQueryError> for QueryError {
    fn from(error: FactQueryError) -> Self {
        match error {
            FactQueryError::Cancelled => Self::Cancelled,
            error => Self::Evaluation(error),
        }
    }
}

impl From<WorkspaceError> for QueryError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
