use bray_binder::{BoundUnitBindingError, BoundUnitConstructionError};
use bray_compiler_known::CompilerKnownScopeId;
use bray_diagnostics::DiagnosticFailureValue;
use bray_symbols::{
    AnySymbolId, CompilerKnownSymbolBuildError, FunctionSymbolId, SymbolGraphBuildError, SymbolId,
};

use super::{diagnostic_binding_failure, diagnostic_symbol_graph_failure};

#[test]
fn binding_construction_retains_leaf_reason_and_identity() {
    let symbol = AnySymbolId::Function(FunctionSymbolId::from_symbol_id(SymbolId::new(19)));

    let failure = diagnostic_binding_failure(&BoundUnitBindingError::Construction(
        BoundUnitConstructionError::UnknownSurfaceSymbol(symbol),
    ));

    assert_eq!(
        failure.as_str(),
        "binding_construction_unknown_surface_symbol"
    );

    assert_eq!(failure.context()[0].name(), "symbol_kind");

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Text("function".to_owned())
    );

    assert_eq!(failure.context()[1].name(), "symbol");

    assert_eq!(
        failure.context()[1].value(),
        &DiagnosticFailureValue::Count(19)
    );
}

#[test]
fn compiler_known_symbol_failures_retain_expected_and_actual_ids() {
    let failure = diagnostic_symbol_graph_failure(SymbolGraphBuildError::CompilerKnown(
        CompilerKnownSymbolBuildError::NonCanonicalScopeId {
            expected: CompilerKnownScopeId::new(3),
            actual: CompilerKnownScopeId::new(8),
        },
    ));

    assert_eq!(
        failure.reason(),
        "symbol_graph_compiler_known_non_canonical_scope_id"
    );

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Count(3)
    );

    assert_eq!(
        failure.context()[1].value(),
        &DiagnosticFailureValue::Count(8)
    );
}

#[test]
fn symbol_graph_failures_retain_exact_capacity_index() {
    let failure = diagnostic_symbol_graph_failure(SymbolGraphBuildError::SymbolCapacityExceeded {
        index: 47,
    });

    assert_eq!(failure.category(), "symbol_graph");
    assert_eq!(failure.reason(), "symbol_graph_capacity_exceeded");
    assert_eq!(failure.context()[0].name(), "index");

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Natural("47".to_owned())
    );
}
