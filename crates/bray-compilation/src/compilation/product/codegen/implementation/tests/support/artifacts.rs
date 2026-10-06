use super::compilation::{codegen_compilation_for_product, test_product_identity};
use crate::CancellationToken;
use bray_codegen::{
    BackendArtifactId, BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestEntry,
    BackendArtifactRequirement, BackendSerializationOptions, CodeGenerator, CodegenOptions,
    CodegenRequest, CodegenStatus, LinkableArtifactKind, LinkableArtifactRequirement,
};
use bray_symbols::ProductKind;

pub(in super::super) fn assert_source_emits_valid_native_units(
    source: &str,
    configuration: crate::BuildConfiguration,
) {
    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let plan = compilation
        .native_product_plan(test_product_identity(), configuration, None, [], None)
        .unwrap_or_else(|error| panic!("target-control source must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

pub(in super::super) fn generated_artifacts(
    backend: &bray_codegen_llvm::LlvmCodeGenerator,
    plan: &crate::compilation::product::codegen::NativeProductPlan,
) -> Vec<Vec<u8>> {
    generated_artifacts_of_kind(backend, plan, BackendArtifactKind::RelocatableObject)
}

pub(in super::super) fn generated_artifacts_of_kind(
    backend: &bray_codegen_llvm::LlvmCodeGenerator,
    plan: &crate::compilation::product::codegen::NativeProductPlan,
    kind: BackendArtifactKind,
) -> Vec<Vec<u8>> {
    generated_artifacts_of_kind_with_options(backend, plan, kind, plan.options())
}

pub(in super::super) fn generated_artifacts_of_kind_with_options(
    backend: &bray_codegen_llvm::LlvmCodeGenerator,
    plan: &crate::compilation::product::codegen::NativeProductPlan,
    kind: BackendArtifactKind,
    options: &CodegenOptions,
) -> Vec<Vec<u8>> {
    plan.units()
        .iter()
        .zip(plan.mappings())
        .map(|(unit, mappings)| {
            let artifact = BackendArtifactId::new(unit.key().clone(), kind, 0);

            let artifacts = BackendArtifactRequest::new(
                unit.key().clone(),
                [BackendArtifactRequestEntry::new(
                    artifact,
                    BackendArtifactRequirement::Required,
                )],
                plan.backend().policy().debug_output(),
                (kind == BackendArtifactKind::RelocatableObject).then(|| {
                    LinkableArtifactRequirement::new(
                        LinkableArtifactKind::RelocatableObject,
                        BackendArtifactRequirement::Required,
                    )
                }),
                BackendSerializationOptions::new(bray_codegen::AssemblySyntaxKind::TargetDefault),
            );

            let cancellation = CancellationToken::new();

            let request = CodegenRequest::new(
                unit,
                backend.identity(),
                backend.capabilities().revision(),
                ProductKind::Executable,
                plan.target(),
                mappings,
                options,
                &artifacts,
                &cancellation,
            );

            let outcome = backend.generate(request);

            assert!(
                matches!(outcome.status(), CodegenStatus::Complete(_)),
                "{:?}: {:?}",
                unit.key(),
                outcome.status()
            );

            let contribution = outcome
                .artifacts()
                .and_then(|artifacts| artifacts.first())
                .unwrap_or_else(|| panic!("test codegen must publish one artifact"));

            match contribution.content().source() {
                bray_codegen::ArtifactContentSource::Memory(bytes) => bytes.to_vec(),
                bray_codegen::ArtifactContentSource::CompilerSpool(_) => {
                    panic!("test artifact must remain memory-backed");
                }
            }
        })
        .collect()
}
