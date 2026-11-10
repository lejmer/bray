use bray_diagnostics::{DiagnosticCondition as Condition, DiagnosticConditionProjection as Projection, DiagnosticContractObligation, DiagnosticContractObligationKind, DiagnosticExecutionProofFailure};
use serde::Serialize;

use super::value::DiagnosticTypeArgumentJson;

#[derive(Serialize)]
pub(super) struct WitnessTransferJson {
    operation: &'static str,
    carrier: ConditionJson,
    guarantee: ConditionJson,
}

impl WitnessTransferJson {
    pub(super) fn from_failure(value: &bray_diagnostics::DiagnosticWitnessTransfer) -> Self {
        Self {
            operation: match value.kind {
                bray_diagnostics::DiagnosticWitnessTransferKind::Copy => "copy",
                bray_diagnostics::DiagnosticWitnessTransferKind::Separation => "separation",
            },
            carrier: ConditionJson::from_condition(&value.carrier),
            guarantee: ConditionJson::from_condition(&value.guarantee),
        }
    }
}

#[derive(Serialize)]
pub(super) struct ContractObligationJson {
    boundary: &'static str,
    required: ConditionJson,
    actual: ConditionJson,
}

impl ContractObligationJson {
    pub(super) fn from_obligation(value: &DiagnosticContractObligation) -> Self {
        Self {
            boundary: match value.kind {
                DiagnosticContractObligationKind::CallRequirement => "call_requirement",
                DiagnosticContractObligationKind::CleanupRequirement => "cleanup_requirement",
                DiagnosticContractObligationKind::Postcondition => "postcondition",
                DiagnosticContractObligationKind::ExecutionRequirement => "execution_requirement",
            },
            required: ConditionJson::from_condition(&value.required),
            actual: ConditionJson::from_condition(&value.actual),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(super) enum ExecutionProofFailureJson {
    Unproven,
    UnboundedLoop,
    Panic,
    Cancellation,
    NonReturningPath,
    Recovery,
    InputMutation,
    DependencyErrors,
    MissingEvidence,
    ContractMismatch,
    Circular,
    DepthLimit { depth: u64, limit: u64 },
}

impl ExecutionProofFailureJson {
    pub(super) const fn from_failure(value: DiagnosticExecutionProofFailure) -> Self {
        match value {
            DiagnosticExecutionProofFailure::Unproven => Self::Unproven,
            DiagnosticExecutionProofFailure::UnboundedLoop => Self::UnboundedLoop,
            DiagnosticExecutionProofFailure::Panic => Self::Panic,
            DiagnosticExecutionProofFailure::Cancellation => Self::Cancellation,
            DiagnosticExecutionProofFailure::NonReturningPath => Self::NonReturningPath,
            DiagnosticExecutionProofFailure::Recovery => Self::Recovery,
            DiagnosticExecutionProofFailure::InputMutation => Self::InputMutation,
            DiagnosticExecutionProofFailure::DependencyErrors => Self::DependencyErrors,
            DiagnosticExecutionProofFailure::MissingEvidence => Self::MissingEvidence,
            DiagnosticExecutionProofFailure::ContractMismatch => Self::ContractMismatch,
            DiagnosticExecutionProofFailure::Circular => Self::Circular,
            DiagnosticExecutionProofFailure::DepthLimit { depth, limit } => Self::DepthLimit { depth, limit },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum ConditionJson {
    Source(String),
    Literal(String),
    Boolean(bool),
    Result,
    Argument(u32),
    Application { path: Vec<String>, types: Vec<DiagnosticTypeArgumentJson>, arguments: Vec<Self> },
    Operation { operation: &'static str, operands: Vec<Self> },
    Projection { value: Box<Self>, projection: ProjectionJson },
    Present(Box<Self>),
    Unknown,
    Absent,
    String(String),
    Character(char),
    CapturedInput { owner: Box<Self>, ordinal: u32 },
    VariantTest { value: Box<Self>, variant: String },
    StaticAddress(Vec<String>),
    Elided,
}

impl ConditionJson {
    fn from_condition(value: &Condition) -> Self {
        match value {
            Condition::Source(value) => Self::Source(value.to_owned()),
            Condition::Literal(value) => Self::Literal(value.to_owned()),
            Condition::Boolean(value) => Self::Boolean(*value),
            Condition::Result => Self::Result,
            Condition::Argument(value) => Self::Argument(*value),
            Condition::Application { path, types, arguments } => Self::Application {
                path: path.to_vec(), types: types.iter().map(DiagnosticTypeArgumentJson::from_argument).collect(),
                arguments: arguments.iter().map(Self::from_condition).collect(),
            },
            Condition::Operation { operation, operands } => Self::Operation {
                operation: operation.as_str(), operands: operands.iter().map(Self::from_condition).collect(),
            },
            Condition::Projection { value, projection } => Self::Projection {
                value: Self::boxed(value), projection: ProjectionJson::from_projection(projection),
            },
            Condition::Present(value) => Self::Present(Self::boxed(value)),
            Condition::Unknown => Self::Unknown,
            Condition::Absent => Self::Absent,
            Condition::String(value) => Self::String(value.to_owned()),
            Condition::Character(value) => Self::Character(*value),
            Condition::CapturedInput { owner, ordinal } => Self::CapturedInput { owner: Self::boxed(owner), ordinal: *ordinal },
            Condition::VariantTest { value, variant } => Self::VariantTest { value: Self::boxed(value), variant: variant.to_owned() },
            Condition::StaticAddress(path) => Self::StaticAddress(path.to_vec()),
            Condition::Elided => Self::Elided,
        }
    }

    fn boxed(value: &Condition) -> Box<Self> {
        Box::new(Self::from_condition(value))
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum ProjectionJson {
    Field(String),
    TupleElement(u32),
    ArrayElement(u32),
    Index(Box<ConditionJson>),
    Slice { lower: Option<Box<ConditionJson>>, upper: Option<Box<ConditionJson>> },
    NullableValue,
}

impl ProjectionJson {
    fn from_projection(value: &Projection) -> Self {
        match value {
            Projection::Field(value) => Self::Field(value.to_owned()),
            Projection::TupleElement(value) => Self::TupleElement(*value),
            Projection::ArrayElement(value) => Self::ArrayElement(*value),
            Projection::Index(value) => Self::Index(ConditionJson::boxed(value)),
            Projection::Slice { lower, upper } => Self::Slice {
                lower: lower.as_deref().map(ConditionJson::boxed), upper: upper.as_deref().map(ConditionJson::boxed),
            },
            Projection::NullableValue => Self::NullableValue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractObligationJson, ExecutionProofFailureJson, WitnessTransferJson};
    use bray_diagnostics::{DiagnosticCondition as Condition, DiagnosticConditionProjection as Projection, DiagnosticContractObligation, DiagnosticContractObligationKind, DiagnosticExecutionProofFailure};

    #[test]
    fn json_preserves_the_transfer_operation_carrier_and_complete_guarantee() {
        use bray_diagnostics::{DiagnosticWitnessTransfer, DiagnosticWitnessTransferKind};

        for (kind, operation) in [(DiagnosticWitnessTransferKind::Copy, "copy"),
            (DiagnosticWitnessTransferKind::Separation, "separation")] {
            let source = format!("inline {{\n{}\n}}", "statement;".repeat(20));
            let failure = DiagnosticWitnessTransfer {
                kind, carrier: Condition::Source(source.clone()),
                guarantee: Condition::Application {
                    path: vec!["app".to_owned(), "live".to_owned()], types: vec![],
                    arguments: vec![Condition::Source(source.clone())],
                },
            };
            let json = serde_json::to_value(WitnessTransferJson::from_failure(&failure))
                .expect("witness transfer JSON serializes");

            assert_eq!(json["operation"], operation);
            assert_eq!(json["carrier"]["value"], source);
            assert_eq!(json["guarantee"]["value"]["path"][1], "live");
            assert_eq!(json["guarantee"]["value"]["arguments"][0]["value"], source);
        }
    }

    #[test]
    fn json_preserves_contract_boundaries_and_nested_actual_arguments() {
        let actual = Condition::Application {
            path: vec!["api".to_owned(), "live".to_owned()], types: vec![],
            arguments: vec![Condition::Projection {
                value: Box::new(Condition::Source("owner".to_owned())), projection: Projection::Field("token".to_owned()),
            }, Condition::Literal("42".to_owned())],
        };
        let obligation = DiagnosticContractObligation {
            kind: DiagnosticContractObligationKind::CleanupRequirement,
            required: Condition::Argument(0), actual,
        };
        let json = serde_json::to_value(ContractObligationJson::from_obligation(&obligation)).expect("contract JSON serializes");

        assert_eq!(json["boundary"], "cleanup_requirement");
        assert_eq!(json["required"]["kind"], "argument");
        assert_eq!(json["actual"]["value"]["path"][1], "live");
        assert_eq!(json["actual"]["value"]["arguments"][0]["value"]["projection"]["value"], "token");
        assert_eq!(json["actual"]["value"]["arguments"][1]["value"], "42");
    }

    #[test]
    fn json_keeps_operational_proof_limits_typed() {
        let json = serde_json::to_value(ExecutionProofFailureJson::from_failure(
            DiagnosticExecutionProofFailure::DepthLimit { depth: 129, limit: 128 },
        )).expect("proof failure JSON serializes");

        assert_eq!(json["reason"], "depth_limit");
        assert_eq!(json["depth"], 129);
        assert_eq!(json["limit"], 128);

        let json = serde_json::to_value(ExecutionProofFailureJson::from_failure(
            DiagnosticExecutionProofFailure::UnboundedLoop,
        )).expect("termination failure JSON serializes");
        assert_eq!(json["reason"], "unbounded_loop");
    }
}
