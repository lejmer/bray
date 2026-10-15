use bray_diagnostics::{
    DiagnosticArgName, DiagnosticArgValue, DiagnosticFailureValue, DiagnosticKind,
    DiagnosticNativeProductFailureKind, DiagnosticNoteKind, DiagnosticSemanticValueFailure,
};
use bray_symbols::{PackageIdentity, ProductIdentity, SemanticValueKind, SemanticValueStoreError};
use bray_testing::assert_goal_state_diagnostic_kind;

use super::context::failure_detail;
use super::link_input::diagnostic_native_link_input_failure;
use super::presentation::native_product_failure_kind;
use super::query::fact_query_failure_kind;
use super::{
    NativeLinkInputPlanningError, NativeProductPlanningError, codegen_preparation_failure_kind,
    native_product_preparation_diagnostic,
};
use crate::fact::FactQueryError;

#[test]
fn runtime_selection_reports_unresolved_native_symbol() {
    let error = NativeProductPlanningError::NativeResolution(
        bray_native_artifact::NativeResolutionError::Unresolved(
            bray_symbols::NativeSymbolContract::required_name(
                bray_base::NonEmptySharedStr::try_new("runtime_missing")
                    .expect("test symbol must be nonempty"),
            ),
        ),
    );

    let failure = native_product_failure_kind(&error)
        .unwrap_or_else(|| panic!("runtime selection failure must diagnose"));

    let DiagnosticNativeProductFailureKind::NativeResolution(detail) = failure else {
        panic!("native resolution must retain its exact diagnostic leaf");
    };

    assert_eq!(detail.reason(), "native_symbol_unresolved");

    assert!(matches!(
        detail.context()[0].value(),
        DiagnosticFailureValue::Text(symbol) if symbol == "runtime_missing"
    ));
}

#[test]
fn codegen_preparation_reports_mir_capacity() {
    let failure = codegen_preparation_failure_kind(
        &crate::compilation::CodegenPreparationError::MirCapacity(
            bray_ir::MirCapacityError::IdentityCapacityExceeded,
        ),
    )
    .unwrap_or_else(|| panic!("codegen preparation failure must diagnose"));

    let DiagnosticNativeProductFailureKind::CodegenMirCapacityExceeded = failure else {
        panic!("MIR capacity failure must retain its diagnostic category");
    };
}

#[test]
fn native_product_evaluation_failures_preserve_specific_reasons() {
    use DiagnosticNativeProductFailureKind as Kind;

    let capacity = SemanticValueStoreError::CapacityExhausted {
        kind: SemanticValueKind::ConstantValue,
    };

    let cases = [
        (
            FactQueryError::ConstantCallableBodyUnavailable,
            Kind::EvaluationConstantCallableBodyUnavailable,
        ),
        (
            FactQueryError::ConstantCallableRootUnavailable,
            Kind::EvaluationConstantCallableRootUnavailable,
        ),
        (
            FactQueryError::AtomicInitializerArgumentUnavailable,
            Kind::EvaluationAtomicInitializerArgumentUnavailable,
        ),
        (
            FactQueryError::AtomicInitializerResultUnavailable,
            Kind::EvaluationAtomicInitializerResultUnavailable,
        ),
        (
            FactQueryError::UninitInitializerResultUnavailable,
            Kind::EvaluationUninitInitializerResultUnavailable,
        ),
        (
            FactQueryError::MirCapacity(bray_ir::MirCapacityError::IdentityCapacityExceeded),
            Kind::CodegenMirCapacityExceeded,
        ),
        (
            FactQueryError::SemanticValueStore(capacity),
            Kind::EvaluationSemanticValue(DiagnosticSemanticValueFailure::CapacityExhausted {
                kind: "constant_value",
            }),
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(fact_query_failure_kind(&error), Some(expected));
    }
}

#[test]
fn native_link_input_failures_use_stable_leaf_fields() {
    let failure =
        diagnostic_native_link_input_failure(&NativeLinkInputPlanningError::InvalidRequirement {
            name: "provider".into(),
            kind: bray_symbols::NativeLinkKind::Static,
            provenance: bray_linker::LinkInputProvenance::Package(
                PackageIdentity::try_new("example.provider").unwrap(),
            ),
        });

    assert_eq!(
        failure,
        bray_diagnostics::DiagnosticNativeLinkInputFailure::InvalidRequirement {
            name: "provider".into(),
            link_kind: "static".into(),
            provenance_kind: "package",
            provenance_identity: Some("example.provider".into()),
        }
    );
}

#[test]
fn native_product_failures_preserve_exact_product_target_and_reason() {
    let package = PackageIdentity::try_new("example")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let product = ProductIdentity::try_new(package, "application")
        .unwrap_or_else(|| panic!("test product identity must be valid"));

    let diagnostics = NativeProductPlanningError::MissingRuntime
        .diagnostic(&product, "x86_64-pc-windows-msvc")
        .unwrap_or_else(|| panic!("terminal native product failure must diagnose"));

    assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::NativeProductPreparationFailed);

    let diagnostic = diagnostics
        .iter()
        .next()
        .unwrap_or_else(|| panic!("test diagnostic must exist"));

    assert!(diagnostic.args().iter().any(|arg| {
        arg.name() == DiagnosticArgName::NativeProductFailureKind
            && matches!(
                arg.value(),
                DiagnosticArgValue::NativeProductFailureKind(
                    DiagnosticNativeProductFailureKind::MissingRuntime
                )
            )
    }));

    assert!(diagnostic.notes().is_empty());
}

#[test]
fn native_library_failures_preserve_product_target_and_exact_symbol() {
    let package = PackageIdentity::try_new("example").unwrap();
    let product = ProductIdentity::try_new(package, "application").unwrap();

    let error = NativeProductPlanningError::NativeResolution(
        bray_native_artifact::NativeResolutionError::Unresolved(
            bray_symbols::NativeSymbolContract::required_name(
                bray_base::NonEmptySharedStr::try_new("provider_entry").unwrap(),
            ),
        ),
    );

    let diagnostics = error
        .diagnostic(&product, "x86_64-pc-windows-msvc")
        .unwrap();

    assert_goal_state_diagnostic_kind(&diagnostics, DiagnosticKind::NativeProductPreparationFailed);

    let diagnostic = diagnostics.iter().next().unwrap();

    assert!(diagnostic.args().iter().any(|arg| matches!(arg.value(),
        DiagnosticArgValue::NativeProductFailureKind(DiagnosticNativeProductFailureKind::NativeResolution(detail))
        if detail.reason() == "native_symbol_unresolved"
            && detail.context().iter().any(|field| matches!(field.value(), DiagnosticFailureValue::Text(name) if name == "provider_entry"))
    )));
}

#[test]
fn unavailable_native_program_provides_recovery_guidance() {
    let package = PackageIdentity::try_new("example")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let product = ProductIdentity::try_new(package, "application")
        .unwrap_or_else(|| panic!("test product identity must be valid"));

    let diagnostic = native_product_preparation_diagnostic(
        DiagnosticNativeProductFailureKind::CodegenMirUnavailable(failure_detail(
            "codegen_mir_unavailable",
            [],
        )),
        &product,
        "x86_64-pc-windows-msvc",
    );

    assert_eq!(diagnostic.notes().len(), 1);

    assert_eq!(
        diagnostic.notes()[0].kind(),
        DiagnosticNoteKind::NativeProductPreparationRecovery
    );
}
