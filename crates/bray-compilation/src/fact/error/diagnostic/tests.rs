use bray_binder::{BoundUnitBindingError, BoundUnitConstructionError};
use bray_compiler_known::CompilerKnownScopeId;
use bray_diagnostics::DiagnosticFailureValue;
use bray_symbols::{CompilerKnownSymbolBuildError, SymbolGraphBuildError};

use super::{diagnostic_binding_failure, diagnostic_symbol_graph_failure};

#[test]
fn binding_construction_retains_capacity_cause() {
    let failure = diagnostic_binding_failure(&BoundUnitBindingError::Construction(
        BoundUnitConstructionError::BoundTree(
            bray_bound_tree::BoundTreeBuildError::ArenaCapacityExceeded(
                bray_bound_tree::BoundNodeKind::Expression,
            ),
        ),
    ));

    assert_eq!(
        failure.as_str(),
        "binding_construction_bound_tree_capacity_exceeded"
    );

    assert_eq!(failure.context()[0].name(), "node_kind");

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Text("expression".to_owned())
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
