use bray_diagnostics::{DiagnosticBindingFailure, DiagnosticFailureField, DiagnosticFailureValue};

pub(crate) fn diagnostic_binding_failure(
    error: &bray_binder::BoundUnitConstructionError,
) -> DiagnosticBindingFailure {
    let (reason, fields) = match error {
        bray_binder::BoundUnitConstructionError::BoundTree(
            bray_bound_tree::BoundTreeBuildError::ArenaCapacityExceeded(kind),
        ) => (
            "binding_tree_capacity_exceeded",
            vec![DiagnosticFailureField::new(
                "node_kind",
                DiagnosticFailureValue::Text(kind.as_str().to_owned()),
            )],
        ),
        bray_binder::BoundUnitConstructionError::LocalSymbol(
            bray_symbols::LocalSymbolBuildError::CapacityExceeded,
        ) => ("binding_local_identity_capacity_exceeded", Vec::new()),
    };

    DiagnosticBindingFailure::new(reason, fields)
}
