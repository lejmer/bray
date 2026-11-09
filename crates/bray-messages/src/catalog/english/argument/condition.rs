use bray_diagnostics::{DiagnosticCondition, DiagnosticConditionProjection, DiagnosticConstantOperation, DiagnosticContractObligation, DiagnosticContractObligationKind, DiagnosticTypeArgument};

pub(super) fn format_obligation(obligation: &DiagnosticContractObligation) -> String {
    let role = match obligation.kind {
        DiagnosticContractObligationKind::CallRequirement => "call requirement",
        DiagnosticContractObligationKind::CleanupRequirement => "cleanup requirement",
        DiagnosticContractObligationKind::Postcondition => "postcondition",
        DiagnosticContractObligationKind::ExecutionRequirement => "execution-context requirement",
    };
    let actual = format_condition(&obligation.actual);
    let condition = if matches!(obligation.actual, DiagnosticCondition::Unknown | DiagnosticCondition::Elided) {
        format!("{} (actual condition: {actual})", format_condition(&obligation.required))
    } else {
        actual
    };
    format!("{role} `{condition}`")
}

fn format_condition(condition: &DiagnosticCondition) -> String {
    match condition {
        DiagnosticCondition::Source(value) | DiagnosticCondition::Literal(value) => value.clone(),
        DiagnosticCondition::Boolean(value) => value.to_string(),
        DiagnosticCondition::String(value) => format!("{value:?}"),
        DiagnosticCondition::Character(value) => format!("{value:?}"),
        DiagnosticCondition::Absent => "none".to_owned(),
        DiagnosticCondition::Result => "result".to_owned(),
        DiagnosticCondition::Argument(ordinal) => format!("argument {}", u64::from(*ordinal) + 1),
        DiagnosticCondition::Application { path, types, arguments } => {
            let mut name = path.join(".");
            if !types.is_empty() {
                let types = types.iter().map(|argument| match argument {
                    DiagnosticTypeArgument::Type(ty) => super::source::format_english_type(ty),
                    DiagnosticTypeArgument::Constant => "…".to_owned(),
                }).collect::<Vec<_>>().join(", ");
                name.push_str(&format!("<{types}>"));
            }
            format!("{name}({})", arguments.iter().map(format_condition).collect::<Vec<_>>().join(", "))
        }
        DiagnosticCondition::Operation { operation, operands } => {
            let operands = operands.iter().map(format_condition).collect::<Vec<_>>();
            let operator = operation_token(*operation);
            match operands.as_slice() {
                [value] => format!("{operator}({value})"),
                [left, right] => format!("({left} {operator} {right})"),
                operands => format!("{operator}({})", operands.join(", ")),
            }
        }
        DiagnosticCondition::Projection { value, projection } => {
            let value = format_condition(value);
            match projection {
                DiagnosticConditionProjection::Field(field) => format!("{value}.{field}"),
                DiagnosticConditionProjection::TupleElement(index) => format!("{value}.{index}"),
                DiagnosticConditionProjection::ArrayElement(index) => format!("{value}[{index}]"),
                DiagnosticConditionProjection::Index(index) => format!("{value}[{}]", format_condition(index)),
                DiagnosticConditionProjection::Slice { lower, upper } => format!("{value}[{}..{}]",
                    lower.as_deref().map(format_condition).unwrap_or_default(),
                    upper.as_deref().map(format_condition).unwrap_or_default()),
                DiagnosticConditionProjection::NullableValue => format!("present_value({value})"),
            }
        }
        DiagnosticCondition::Present(value) => format!("?{}", format_condition(value)),
        DiagnosticCondition::CapturedInput { owner, ordinal } => format!("capture({}, {})", format_condition(owner), ordinal),
        DiagnosticCondition::VariantTest { value, variant } => format!("{} matches {variant}", format_condition(value)),
        DiagnosticCondition::StaticAddress(path) => format!("address_of({})", path.join(".")),
        DiagnosticCondition::Unknown => "unknown value".to_owned(),
        DiagnosticCondition::Elided => "…".to_owned(),
    }
}

const fn operation_token(operation: DiagnosticConstantOperation) -> &'static str {
    match operation {
        DiagnosticConstantOperation::Add => "+",
        DiagnosticConstantOperation::Subtract => "-",
        DiagnosticConstantOperation::Multiply => "*",
        DiagnosticConstantOperation::Divide => "/",
        DiagnosticConstantOperation::Remainder => "%",
        DiagnosticConstantOperation::Exponentiate => "**",
        DiagnosticConstantOperation::LogicalAnd => "&&",
        DiagnosticConstantOperation::LogicalOr => "||",
        DiagnosticConstantOperation::LogicalNot => "!",
        DiagnosticConstantOperation::BitwiseAnd => "&",
        DiagnosticConstantOperation::BitwiseOr => "|",
        DiagnosticConstantOperation::BitwiseXor => "^",
        DiagnosticConstantOperation::BitwiseNot => "~",
        DiagnosticConstantOperation::ShiftLeft => "<<",
        DiagnosticConstantOperation::ShiftRight => ">>",
        DiagnosticConstantOperation::Equal => "==",
        DiagnosticConstantOperation::NotEqual => "!=",
        DiagnosticConstantOperation::Less => "<",
        DiagnosticConstantOperation::LessEqual => "<=",
        DiagnosticConstantOperation::Greater => ">",
        DiagnosticConstantOperation::GreaterEqual => ">=",
        DiagnosticConstantOperation::MatrixMultiply => "@",
        DiagnosticConstantOperation::Conversion => "as",
    }
}

pub(super) fn format_proof_failure(cause: bray_diagnostics::DiagnosticExecutionProofFailure) -> String {
    use bray_diagnostics::DiagnosticExecutionProofFailure;

    match cause {
        DiagnosticExecutionProofFailure::Unproven => "this operation or its cleanup does not establish the required contract".to_owned(),
        DiagnosticExecutionProofFailure::DependencyErrors => "this required callable has checking errors".to_owned(),
        DiagnosticExecutionProofFailure::MissingEvidence => "this dependency has no checked evidence for the required contract".to_owned(),
        DiagnosticExecutionProofFailure::ContractMismatch => "this callable's declared contract does not provide the required guarantee".to_owned(),
        DiagnosticExecutionProofFailure::Circular => "this dependency assumes its own unverified completion".to_owned(),
        DiagnosticExecutionProofFailure::DepthLimit { depth, limit } => format!("proof depth {depth} exceeds the configured limit of {limit}"),
    }
}

#[cfg(test)]
mod tests {
    use super::format_condition;
    use bray_diagnostics::{DiagnosticCondition, DiagnosticConditionProjection};

    #[test]
    fn tuple_and_array_components_keep_their_source_access_forms() {
        let component = |projection| DiagnosticCondition::Projection {
            value: Box::new(DiagnosticCondition::Source("packet".to_owned())), projection,
        };

        assert_eq!(format_condition(&component(DiagnosticConditionProjection::TupleElement(0))), "packet.0");
        assert_eq!(format_condition(&component(DiagnosticConditionProjection::ArrayElement(0))), "packet[0]");
    }
}
