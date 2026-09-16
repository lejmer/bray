use bray_checker::CheckerInfrastructureError;
use bray_diagnostics::{
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticFailureValue, DiagnosticKind,
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
fn runtime_selection_failures_preserve_component_path_io_and_digests() {
    let error = NativeProductPlanningError::InvalidRuntimeSelection(
        bray_runtime_interface::RuntimeArtifactSelectionError::ArchiveDigestMismatch {
            component: bray_runtime_interface::RuntimeArtifactId::try_new(
                "runtime.product.execution",
            )
            .unwrap_or_else(|| panic!("test runtime component identity must be valid")),
            path: std::path::PathBuf::from("runtime/product.lib"),
            expected: bray_runtime_interface::RuntimeArtifactDigest::new([3; 32]),
            actual: bray_runtime_interface::RuntimeArtifactDigest::new([5; 32]),
        },
    );

    let failure = native_product_failure_kind(&error)
        .unwrap_or_else(|| panic!("runtime selection failure must diagnose"));

    let DiagnosticNativeProductFailureKind::RuntimeSelectionArchiveDigestMismatch(detail) = failure
    else {
        panic!("archive authentication must retain its exact diagnostic leaf");
    };

    assert_eq!(detail.reason(), "runtime_selection_archive_digest_mismatch");

    assert!(matches!(
        detail.context()[0].value(),
        DiagnosticFailureValue::Text(component) if component == "runtime.product.execution"
    ));

    assert!(matches!(
        detail.context()[1].value(),
        DiagnosticFailureValue::Path(path) if path == std::path::Path::new("runtime/product.lib")
    ));

    assert!(matches!(
        detail.context()[2].value(),
        DiagnosticFailureValue::ArtifactDigest(digest) if digest.bytes() == &[3; 32]
    ));

    assert!(matches!(
        detail.context()[3].value(),
        DiagnosticFailureValue::ArtifactDigest(digest) if digest.bytes() == &[5; 32]
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
            FactQueryError::CheckerInfrastructure(
                CheckerInfrastructureError::AtomicRepresentationTypeUnavailable,
            ),
            Kind::EvaluationAtomicRepresentationTypeUnavailable,
        ),
        (
            FactQueryError::CheckerInfrastructure(
                CheckerInfrastructureError::AtomicRepresentationArgumentsUnavailable,
            ),
            Kind::EvaluationAtomicRepresentationArgumentsUnavailable,
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
    let failure = diagnostic_native_link_input_failure(
        &NativeLinkInputPlanningError::InvalidStandardLibraryArtifact {
            path: std::path::PathBuf::from("lib/example.a"),
            kind: bray_linker::LinkInputKind::Archive,
            cause: bray_linker::LinkInputBuildError::WholeArchiveRequiresArchive,
        },
    );

    assert!(matches!(
        failure,
        bray_diagnostics::DiagnosticNativeLinkInputFailure::InvalidStandardLibraryArtifact {
            input_kind: "archive",
            cause: "whole_archive_requires_archive",
            ..
        }
    ));
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
fn standard_library_failures_preserve_product_target_and_exact_cause() {
    let package = PackageIdentity::try_new("example")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let product = ProductIdentity::try_new(package, "application")
        .unwrap_or_else(|| panic!("test product identity must be valid"));

    let target = bray_target::TargetIdentity::try_new("x86_64-pc-windows-msvc")
        .unwrap_or_else(|| panic!("test target identity must be valid"));

    let diagnostics = NativeProductPlanningError::StandardLibrary {
        artifact_path: std::path::PathBuf::from("standard-library.json"),
        cause: bray_standard_library::StandardLibraryLoadError::OptimizationUnavailable {
            target: target.clone(),
        },
    }
    .diagnostic(&product, target.as_str())
    .unwrap_or_else(|| panic!("standard-library planning failure must diagnose"));

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.kind() == DiagnosticKind::StandardLibraryOptimizationUnavailable
    }));

    assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::StandardLibraryOptimizationUnavailable,
    );

    let outer = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.kind() == DiagnosticKind::NativeProductPreparationFailed)
        .unwrap_or_else(|| panic!("product-context diagnostic must exist"));

    assert!(outer.args().iter().any(|arg| {
        matches!(
            arg.value(),
            DiagnosticArgValue::NativeProductFailureKind(
                DiagnosticNativeProductFailureKind::StandardLibraryUnavailable
            )
        )
    }));

    let artifact_path = std::path::PathBuf::from("targets/test/libstd.a");

    let infrastructure = NativeProductPlanningError::StandardLibrary {
        artifact_path: std::path::PathBuf::from("standard-library.json"),
        cause: bray_standard_library::StandardLibraryLoadError::Infrastructure {
            path: artifact_path.clone(),
        },
    }
    .diagnostic(&product, target.as_str())
    .unwrap_or_else(|| panic!("standard-library infrastructure failure must diagnose"));

    let cause = infrastructure
        .iter()
        .find(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::StandardLibraryInfrastructureFailure
        })
        .unwrap_or_else(|| panic!("standard-library infrastructure cause must exist"));

    assert_eq!(cause.args(), &[DiagnosticArg::artifact_path(artifact_path)]);
    bray_testing::assert_goal_state_diagnostic(cause);
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
