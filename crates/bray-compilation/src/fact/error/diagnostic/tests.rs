use bray_binder::BoundUnitConstructionError;
use bray_compiler_known::CompilerKnownScopeId;
use bray_diagnostics::DiagnosticFailureValue;
use bray_symbols::{CompilerKnownSymbolBuildError, SymbolGraphBuildError};

use super::{diagnostic_binding_failure, diagnostic_symbol_capacity_failure};

#[test]
fn binding_construction_retains_capacity_cause() {
    let failure = diagnostic_binding_failure(&BoundUnitConstructionError::BoundTree(
        bray_bound_tree::BoundTreeBuildError::ArenaCapacityExceeded(
            bray_bound_tree::BoundNodeKind::Expression,
        ),
    ));

    assert_eq!(failure.as_str(), "binding_tree_capacity_exceeded");

    assert_eq!(failure.context()[0].name(), "node_kind");

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Text("expression".to_owned())
    );
}

#[test]
fn compiler_known_symbol_failures_retain_expected_and_actual_ids() {
    let panic = std::panic::catch_unwind(|| {
        crate::fact::FactQueryError::from(SymbolGraphBuildError::CompilerKnown(
            CompilerKnownSymbolBuildError::NonCanonicalScopeId {
                expected: CompilerKnownScopeId::new(3),
                actual: CompilerKnownScopeId::new(8),
            },
        ))
    })
    .err()
    .expect("compiler invariant must panic");

    let message = bray_testing::panic_payload_text(panic.as_ref());

    assert!(message.contains("NonCanonicalScopeId"));
    assert!(message.contains("3"));
    assert!(message.contains("8"));
}

#[test]
fn symbol_graph_failures_retain_exact_capacity_index() {
    let failure = diagnostic_symbol_capacity_failure(47);

    assert_eq!(failure.category(), "symbol_capacity");
    assert_eq!(failure.reason(), "symbol_capacity_exceeded");
    assert_eq!(failure.context()[0].name(), "index");

    assert_eq!(
        failure.context()[0].value(),
        &DiagnosticFailureValue::Natural("47".to_owned())
    );
}
