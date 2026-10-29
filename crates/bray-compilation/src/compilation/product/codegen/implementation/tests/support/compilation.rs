use crate::{CompilationOptions, CompilationRequest};
use bray_codegen::{CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration};
use bray_symbols::ProductIdentity;
use std::sync::Arc;

pub(in super::super) fn codegen_compilation(
    request: CompilationRequest,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    let (backend, codegen) = codegen_configuration();

    let compilation = crate::Compilation::load_with_codegen(request, codegen)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    (backend, compilation)
}

/// Assigns source identities in order and imports the target's cached test runtime library.
pub(in super::super) fn source_request_with_runtime(
    sources: &[&str],
    options: CompilationOptions,
) -> CompilationRequest {
    let runtime_dependency =
        crate::test_support::runtime_standard_library_dependency(options.selected_target());

    CompilationRequest::with_options(
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
        options,
    )
    .with_dependency_interfaces([runtime_dependency])
}

pub(in super::super) fn test_product_identity() -> ProductIdentity {
    ProductIdentity::try_new(crate::test_support::package_identity(), "application")
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}

pub(in super::super) fn codegen_configuration() -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    CodegenConfiguration,
) {
    let backend = Arc::new(
        bray_codegen_llvm::LlvmCodeGenerator::try_new()
            .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
    );

    let registry = CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
        .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

    (backend, codegen)
}
