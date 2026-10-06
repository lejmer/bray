use crate::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_codegen::{CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration};
use bray_runtime_interface::PlatformServiceBinding;
use bray_symbols::{NativeLinkRequirement, ProductIdentity, ProductKind};
use std::sync::Arc;

pub(in super::super) fn codegen_compilation(
    source: &str,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_product(source, ProductKind::Executable)
}

pub(in super::super) fn codegen_compilation_for_product(
    source: &str,
    product_kind: ProductKind,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_product_target(source, product_kind, SelectedTarget::baseline())
}

fn codegen_compilation_for_product_target(
    source: &str,
    product_kind: ProductKind,
    target: SelectedTarget,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_sources_target(&[source], product_kind, target, &[])
}

pub(in super::super) fn codegen_compilation_for_sources_target(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_sources_target_with_platform_services(
        sources,
        product_kind,
        target,
        native_link_inputs,
        [],
    )
}

pub(super) fn codegen_compilation_for_sources_target_with_platform_services(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
        sources,
        product_kind,
        target,
        native_link_inputs,
        platform_services,
        WorkerBudget::serial(),
    )
}

pub(in super::super) fn codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    worker_budget: WorkerBudget,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_sources_target_with_source_roles(
        sources,
        product_kind,
        target,
        native_link_inputs,
        platform_services,
        [],
        worker_budget,
    )
}

pub(in super::super) fn codegen_compilation_for_sources_target_with_source_roles(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    runtime_roles: impl IntoIterator<Item = bray_runtime_interface::RuntimeRoleSourceBinding>,
    worker_budget: WorkerBudget,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    let backend = Arc::new(
        bray_codegen_llvm::LlvmCodeGenerator::try_new()
            .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
    );

    let registry = CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
        .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

    let runtime_dependency = crate::test_support::runtime_standard_library_dependency(&target);

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        sources
            .iter()
            .enumerate()
            .map(|(identity, source)| {
                crate::test_support::source_input(
                    source,
                    u32::try_from(identity)
                        .unwrap_or_else(|_| panic!("test source identity must fit u32")),
                )
            })
            .collect(),
        CompilationOptions::new(worker_budget, product_kind, target)
            .with_native_link_inputs(native_link_inputs.iter().cloned()),
    )
    .with_dependency_interfaces([runtime_dependency])
    .with_platform_services(platform_services)
    .with_runtime_roles(runtime_roles);

    let compilation = crate::Compilation::load_with_codegen(request, codegen)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    (backend, compilation)
}

pub(in super::super) fn test_product_identity() -> ProductIdentity {
    ProductIdentity::try_new(crate::test_support::package_identity(), "application")
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
