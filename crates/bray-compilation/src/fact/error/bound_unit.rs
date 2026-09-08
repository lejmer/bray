use bray_bound_tree::BoundUnitBuildError;
use bray_diagnostics::DiagnosticFailureField;

use super::diagnostic_context::{count_field, identity_field, natural_field, text_field};

pub(super) fn diagnostic_bound_unit_build_failure(
    error: BoundUnitBuildError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    match error {
        BoundUnitBuildError::RootKindMismatch => ("bound_unit_root_kind_mismatch", Vec::new()),
        BoundUnitBuildError::MissingRoot { unit, kind } => (
            "bound_unit_missing_root",
            vec![
                count_field("missing_unit", u64::from(unit.raw())),
                text_field("root_kind", kind.as_str()),
            ],
        ),
        BoundUnitBuildError::LocalSymbolRegionMismatch => {
            ("bound_unit_local_symbol_region_mismatch", Vec::new())
        }
        BoundUnitBuildError::AnonymousCallableRegionMismatch { expected, actual } => (
            "bound_unit_anonymous_callable_region_mismatch",
            vec![
                count_field("expected_region", u64::from(expected.raw())),
                count_field("actual_region", u64::from(actual.raw())),
            ],
        ),
        BoundUnitBuildError::MissingAnonymousCallable { callable } => (
            "bound_unit_missing_anonymous_callable",
            vec![identity_field("callable", &callable)],
        ),
        BoundUnitBuildError::InvalidNestedUnit { index } => (
            "bound_unit_invalid_nested_unit",
            vec![natural_field("index", index)],
        ),
        BoundUnitBuildError::NonCanonicalNestedUnits { index } => (
            "bound_unit_noncanonical_nested_units",
            vec![natural_field("index", index)],
        ),
        BoundUnitBuildError::MissingContractParameter { parameter } => (
            "bound_unit_missing_contract_parameter",
            vec![identity_field("parameter", &parameter)],
        ),
        BoundUnitBuildError::InvalidContractParameter { index, parameter } => (
            "bound_unit_invalid_contract_parameter",
            vec![
                natural_field("index", index),
                identity_field("parameter", &parameter),
            ],
        ),
        BoundUnitBuildError::ContractParameterCountMismatch { expected, actual } => (
            "bound_unit_contract_parameter_count_mismatch",
            vec![
                natural_field("expected", expected),
                natural_field("actual", actual),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundNodeKind, BoundUnitBuildError, BoundUnitId};
    use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

    use super::diagnostic_bound_unit_build_failure;

    #[test]
    fn bound_unit_failures_preserve_root_identity_and_contract_input_counts() {
        assert_eq!(
            diagnostic_bound_unit_build_failure(BoundUnitBuildError::MissingRoot {
                unit: BoundUnitId::new(17),
                kind: BoundNodeKind::Block,
            }),
            (
                "bound_unit_missing_root",
                vec![
                    DiagnosticFailureField::new("missing_unit", DiagnosticFailureValue::Count(17)),
                    DiagnosticFailureField::new(
                        "root_kind",
                        DiagnosticFailureValue::Text("block".into())
                    ),
                ]
            ),
        );

        assert_eq!(
            diagnostic_bound_unit_build_failure(
                BoundUnitBuildError::ContractParameterCountMismatch {
                    expected: 2,
                    actual: 1,
                }
            ),
            (
                "bound_unit_contract_parameter_count_mismatch",
                vec![
                    DiagnosticFailureField::new(
                        "expected",
                        DiagnosticFailureValue::Natural("2".into())
                    ),
                    DiagnosticFailureField::new(
                        "actual",
                        DiagnosticFailureValue::Natural("1".into())
                    ),
                ]
            ),
        );
    }
}
