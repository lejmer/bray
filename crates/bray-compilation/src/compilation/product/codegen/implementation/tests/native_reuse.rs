use super::support::dependencies::{
    GenericDependencyFixture, dependency_from_fixture_with_native, generic_dependency_from_fixture,
    native_fixture_compilation,
};
use crate::compilation::CodegenPreparationError;
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::compilation::product::specialization::ConcreteCodegenReachability;
use crate::{CancellationToken, DependencyInterfaceInput};
use bray_codegen::{
    CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration, CodegenOptions, OptimizationLevel,
};
use bray_ir::MirUnitKey;
use bray_linker::LinkInputSource;
use bray_native_artifact::NativeUnitKind;
use bray_symbols::ProductKind;
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn exact_native_binding_replaces_imported_mir_and_missing_binding_falls_back() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let selected = native_fixture_reachability(dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, false, None)),
    ))
    .expect("exact native dependency must plan");

    let imported = selected
        .graph()
        .external_instances()
        .iter()
        .filter(|key| matches!(key.template(), MirUnitKey::ImportedExecutable(_)))
        .collect::<Vec<_>>();

    let [imported] = imported.as_slice() else {
        panic!("one imported definition must become an external native demand");
    };

    assert_eq!(
        selected
            .selected_native(imported)
            .expect("native unit must be selected")
            .symbol
            .as_str(),
        "bray_test_precompiled",
    );

    assert!(
        selected
            .graph()
            .instances()
            .iter()
            .all(|instance| !matches!(
                instance.key().template(),
                MirUnitKey::ImportedExecutable(_)
            ))
    );

    for dependency in [
        generic_dependency_from_fixture(true, false, fixture),
        dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((
                CodegenOptions::default().with_optimization(OptimizationLevel::Full),
                false,
                false,
                None,
            )),
        ),
    ] {
        let fallback = native_fixture_reachability(dependency)
            .expect("missing native specialization must use imported MIR");

        assert!(fallback.graph().instances().iter().any(|instance| matches!(
            instance.key().template(),
            MirUnitKey::ImportedExecutable(_)
        )));

        assert_eq!(fallback.selected_native_units().count(), 0);
    }
}

#[test]
fn native_reuse_defers_optional_executable_body_decoding() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                func hot(pos value: i32) -> i32
                {
                    return value + 1;
                }
            "#,
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let selected = native_fixture_reachability(dependency_from_fixture_with_native(
        true,
        true,
        fixture,
        Some((CodegenOptions::default(), false, false, None)),
    ))
    .expect("matching native units must not decode unused executable bodies");

    assert_eq!(selected.selected_native_units().count(), 1);

    let fallback = native_fixture_reachability(dependency_from_fixture_with_native(
        true,
        true,
        fixture,
        Some((
            CodegenOptions::default().with_optimization(OptimizationLevel::Full),
            false,
            false,
            None,
        )),
    ));

    let error = match fallback {
        Ok(_) => panic!("incompatible native code must demand the malformed executable body"),
        Err(error) => error,
    };

    let NativeProductPlanningError::Codegen(crate::CodegenPreparationError::Diagnostics(
        diagnostics,
    )) = error
    else {
        panic!("malformed executable body must retain its diagnostic: {error:?}");
    };

    assert!(diagnostics.has_errors());
}

#[test]
fn imported_native_binding_retains_references_but_excludes_disconnected_package_units() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, true, None)),
    );

    let reachability =
        native_fixture_reachability(dependency.clone()).expect("native binding must be reused");

    let payloads = native_fixture_payloads(dependency, ProductKind::Executable)
        .expect("references must close during product selection");

    let selected = reachability.selected_native_units().collect::<Vec<_>>();

    let [selected] = selected.as_slice() else {
        panic!("one imported definition must select native units");
    };

    assert_eq!(payloads.len(), 2);
    assert_eq!(selected.symbol.as_str(), "bray_test_precompiled");

    assert!(
        payloads
            .iter()
            .all(|unit| unit.bytes.as_ref() != b"disconnected object")
    );

    assert!(
        payloads
            .iter()
            .any(|unit| unit.native_links.iter().any(|link| link.source()
                == &LinkInputSource::try_native_library("native_support")
                    .expect("test native library name")))
    );

    assert!(
        reachability
            .graph()
            .instances()
            .iter()
            .all(|instance| !matches!(
                instance.key().template(),
                MirUnitKey::ImportedExecutable(_)
            ))
    );
}

#[test]
fn opaque_package_code_uses_source_template_to_preserve_support_demand() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let reachability = native_fixture_reachability(dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((
            CodegenOptions::default(),
            false,
            false,
            Some(NativeUnitKind::Bitcode),
        )),
    ))
    .expect("opaque package code must use its source template");

    assert_eq!(reachability.selected_native_units().count(), 0);

    assert!(
        reachability
            .graph()
            .instances()
            .iter()
            .any(|instance| matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)))
    );
}

#[test]
fn static_library_reuses_imported_binding_without_flattening_dependency_archive() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((
            CodegenOptions::default(),
            false,
            false,
            Some(NativeUnitKind::OpaqueArchive),
        )),
    );

    let executable = native_fixture_reachability(dependency.clone())
        .expect("executable must select its opaque archive input");

    assert_eq!(executable.selected_native_units().count(), 1);

    assert!(
        native_fixture_payloads(dependency.clone(), ProductKind::Executable)
            .expect("executable closure")
            .iter()
            .any(|unit| unit.kind == NativeUnitKind::OpaqueArchive)
    );

    assert!(
        native_fixture_payloads(dependency.clone(), ProductKind::Library)
            .expect("library keeps its dependencies separate")
            .is_empty()
    );

    let reachability = native_fixture_reachability_for_product(dependency, ProductKind::Library)
        .expect("static library must reuse the imported native binding");

    assert_eq!(reachability.selected_native_units().count(), 1);

    assert!(
        reachability
            .graph()
            .instances()
            .iter()
            .all(|instance| !matches!(
                instance.key().template(),
                MirUnitKey::ImportedExecutable(_)
            ))
    );
}

#[test]
fn native_companions_reject_conflicting_publications_and_semantic_interfaces() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, false, None)),
    );

    let artifact = dependency
        .shared_implementation_artifact()
        .unwrap()
        .unwrap();

    let duplicate = bray_package_interface::PackageArtifactInput::memory(
        "copy.brayimpl",
        artifact.shared_bytes().unwrap(),
    );

    let payloads = native_fixture_payloads(
        dependency.clone().with_native_implementations([duplicate]),
        ProductKind::Executable,
    )
    .expect("identical companion must select one representation");

    assert_eq!(payloads.len(), 1);

    let conflicting = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, true, None)),
    );

    let artifact = conflicting
        .shared_implementation_artifact()
        .unwrap()
        .unwrap();

    let companion = bray_package_interface::PackageArtifactInput::memory(
        "conflict.brayimpl",
        artifact.shared_bytes().unwrap(),
    );

    let error = native_fixture_payloads(
        dependency.clone().with_native_implementations([companion]),
        ProductKind::Executable,
    )
    .expect_err("different publications must not silently choose the first input");

    assert!(
        matches!(error, NativeProductPlanningError::ConflictingNativeArtifacts { first, second }
            if first == std::path::Path::new("dependency.brayimpl")
                && second == std::path::Path::new("conflict.brayimpl"))
    );

    let different_interface = dependency_from_fixture_with_native(
        true,
        false,
        GenericDependencyFixture {
            source: r#"
                    module templates;

                    public struct Extra {}

                    public func hot(pos value: i32) -> i32 {
                        return value + 1;
                    }
                "#,
            ..fixture
        },
        Some((CodegenOptions::default(), false, false, None)),
    );

    let artifact = different_interface
        .shared_implementation_artifact()
        .unwrap()
        .unwrap();

    let companion = bray_package_interface::PackageArtifactInput::memory(
        "wrong-interface.brayimpl",
        artifact.shared_bytes().unwrap(),
    );

    let error = native_fixture_payloads(
        dependency.with_native_implementations([companion]),
        ProductKind::Executable,
    )
    .expect_err("companion must belong to the selected semantic interface");

    let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
        error
    else {
        panic!("interface mismatch must retain validation diagnostics: {error:?}");
    };

    assert!(diagnostics.iter().any(|diagnostic| diagnostic.kind()
        == bray_diagnostics::DiagnosticKind::InterfaceHashMismatch
        && diagnostic.args().iter().any(|arg| matches!(
            arg.value(),
            bray_diagnostics::DiagnosticArgValue::InterfaceValidationFailure(
                bray_diagnostics::DiagnosticInterfaceValidationFailure::ContentHashMismatch { .. }
            )
        ))));
}

#[test]
fn selected_corrupt_native_payload_reports_dependency_validation_failure() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), true, false, None)),
    );

    let error = native_fixture_payloads(dependency, ProductKind::Executable)
        .err()
        .expect("corrupt selected unit must fail planning");

    let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
        error
    else {
        panic!("corrupt native unit must retain structured validation diagnostics: {error:?}");
    };

    assert!(diagnostics.has_errors());

    assert!(diagnostics.iter().any(|diagnostic| diagnostic.kind()
        == bray_diagnostics::DiagnosticKind::InterfaceValidationFailed));
}

#[test]
fn selected_native_binding_with_wrong_producer_reports_exact_import_failure() {
    let fixture = GenericDependencyFixture {
        source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
        runtime_frames: None,
        executable_templates: 1,
        platform_service: None,
    };

    let backend = Arc::new(
        bray_codegen_llvm::LlvmCodeGenerator::try_new().expect("LLVM backend must initialize"),
    );

    let registry = CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
        .expect("LLVM backend must register");

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
        .expect("LLVM backend must select");

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, false, None)),
    );

    let error = native_fixture_reachability_with_codegen(dependency, Some(codegen))
        .err()
        .expect("mismatched selected producer must fail planning");

    let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
        error
    else {
        panic!("wrong native producer must retain structured diagnostics: {error:?}");
    };

    assert!(
        diagnostics
            .iter()
            .flat_map(|diagnostic| diagnostic.args())
            .any(|arg| {
                matches!(
                    arg.value(),
                    bray_diagnostics::DiagnosticArgValue::InterfaceValidationFailure(
                        bray_diagnostics::DiagnosticInterfaceValidationFailure::NativeArtifact {
                            cause: bray_diagnostics::DiagnosticNativeArtifactCause::WrongProducer,
                            ..
                        }
                    )
                )
            })
    );
}

fn native_fixture_reachability(
    dependency: DependencyInterfaceInput,
) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
    native_fixture_reachability_with_codegen(dependency, None)
}

fn native_fixture_reachability_with_codegen(
    dependency: DependencyInterfaceInput,
    codegen: Option<CodegenConfiguration>,
) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
    native_fixture_reachability_for_product_with_codegen(
        dependency,
        ProductKind::Executable,
        codegen,
    )
}

fn native_fixture_reachability_for_product(
    dependency: DependencyInterfaceInput,
    product_kind: ProductKind,
) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
    native_fixture_reachability_for_product_with_codegen(dependency, product_kind, None)
}

fn native_fixture_reachability_for_product_with_codegen(
    dependency: DependencyInterfaceInput,
    product_kind: ProductKind,
    codegen: Option<CodegenConfiguration>,
) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
    let compilation = native_fixture_compilation(dependency, product_kind, codegen);
    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("test target must support codegen");

    let semantic = compilation.product_semantics()?;

    let roots =
        compilation.product_root_instances(semantic.value(), None, &target, &cancellation)?;

    compilation.codegen_reachability(
        roots,
        None,
        &target,
        CodegenOptions::default(),
        false,
        &cancellation,
    )
}

fn native_fixture_payloads(
    dependency: DependencyInterfaceInput,
    kind: ProductKind,
) -> Result<
    Vec<crate::compilation::product::codegen::reuse::SelectedNativePayload>,
    NativeProductPlanningError,
> {
    native_fixture_compilation(dependency, kind, None)
        .native_library_link_inputs(
            kind,
            &BTreeSet::from(["bray_test_precompiled"]),
            &BTreeSet::new(),
        )
        .map(|(_, payloads)| payloads)
}
