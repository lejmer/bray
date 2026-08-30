use std::path::{Path, PathBuf};

use bray_compilation::{BuildConfiguration, Compilation, ProductEmissionInputs, SelectedTarget};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_linker::LinkSearchPath;
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetOutputDescription, TargetOutputKind};
use bray_tooling::{load_runtime_artifact, native_linker};

pub(crate) fn emit_executable(
    compilation: &Compilation,
    product: ProductIdentity,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    additional_search_paths: impl IntoIterator<Item = LinkSearchPath>,
    map_output: Option<bray_linker::SystemLinkerMapOutput>,
) -> Result<(), String> {
    emit_executable_with_configuration(
        compilation,
        product,
        target,
        runtime,
        output,
        additional_search_paths,
        map_output,
        BuildConfiguration::Release,
    )
}

pub(crate) fn emit_static_library(
    compilation: &Compilation,
    product: ProductIdentity,
    target: NativeTarget,
    output: &Path,
) -> Result<PathBuf, String> {
    let selected = SelectedTarget::for_native(target);

    let linker = native_linker(target, None, output).map_err(|error| {
        format!(
            "native linker is unavailable for {}: {error:?}",
            target.as_str()
        )
    })?;

    let native = compilation
        .native_product_plan(
            product.clone(),
            BuildConfiguration::ObjectRelease,
            None,
            [],
            Some(&linker),
        )
        .map_err(|error| {
            let detail = match error.as_ref() {
                bray_compilation::NativeProductPlanningError::Codegen(
                    bray_compilation::CodegenPreparationError::RecursiveValueType(ty),
                ) => {
                    let data = compilation
                        .semantic_value_store()
                        .ok()
                        .and_then(|values| values.type_data(*ty).ok());

                    let symbol = data.as_deref().and_then(|data| {
                        let bray_symbols::TypeData::Named { definition, .. } = data else {
                            return None;
                        };

                        let bray_symbols::NamedTypeSymbolId::Struct(structure) = definition else {
                            return None;
                        };

                        let symbol = compilation
                            .symbol_graph()
                            .ok()
                            .and_then(|graph| graph.structure(*structure))?;

                        let name = symbol.declaration().and_then(|declaration| {
                            compilation
                                .product_source_graph()
                                .ok()
                                .and_then(|graph| graph.declarations().declaration(declaration))
                                .and_then(|declaration| declaration.name())
                                .and_then(|name| name.as_identifier())
                                .map(str::to_owned)
                        });

                        Some((symbol.key().clone(), name))
                    });

                    format!("; recursive type={data:?}; symbol={symbol:?}")
                }
                _ => String::new(),
            };

            format!("could not plan native library product: {error:?}{detail}")
        })?;

    let outputs = TargetOutputDescription::for_native(target, [TargetOutputKind::StaticLibrary]);

    let request = EmissionRequest::try_new(
        product,
        ProductKind::Library,
        None,
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(output.to_path_buf().into()),
        [RequestedArtifact::new(
            ArtifactKind::StaticLibrary,
            ArtifactRequirement::Required,
        )],
        ReplacementPolicy::ReplaceExisting,
    )
    .map_err(|error| format!("native library emission request is invalid: {error:?}"))?;

    let inputs = ProductEmissionInputs::new(&outputs).with_native_product(&native, &linker);

    let outcome = compilation
        .emit_product(request, inputs)
        .map_err(|error| format!("native library emission failed: {:?}", error.kind()))?;

    if !matches!(outcome.status(), EmissionStatus::Complete) {
        return Err(format!(
            "native library emission did not complete: {:?}; diagnostics={:?}",
            outcome.status(),
            outcome.diagnostics()
        ));
    }

    outcome
        .artifacts()
        .artifacts()
        .iter()
        .find(|artifact| artifact.id().kind() == ArtifactKind::StaticLibrary)
        .and_then(|artifact| {
            outcome
                .generation()
                .and_then(|generation| generation.artifact_path(artifact.id()))
        })
        .ok_or_else(|| "native library emission did not publish its static archive".to_owned())
}

pub(crate) fn emit_executable_with_configuration(
    compilation: &Compilation,
    product: ProductIdentity,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    additional_search_paths: impl IntoIterator<Item = LinkSearchPath>,
    map_output: Option<bray_linker::SystemLinkerMapOutput>,
    configuration: BuildConfiguration,
) -> Result<(), String> {
    let selected = SelectedTarget::for_native(target);
    let compiler_state_root = output.parent().unwrap_or(output);

    let linker = native_linker(target, map_output, compiler_state_root).map_err(|error| {
        format!(
            "native linker is unavailable for {}: {error:?}",
            target.as_str()
        )
    })?;

    let runtime = load_runtime_artifact(
        runtime,
        selected.profile().identity(),
        selected.runtime_abi(),
    )
    .map_err(|error| format!("native runtime artifact is invalid: {error:?}"))?;

    let native = compilation
        .native_product_plan(
            product.clone(),
            configuration,
            Some(runtime),
            [],
            Some(&linker),
        )
        .map_err(|error| format!("could not build native fixture product: {error:?}"))?;

    let native = native
        .as_ref()
        .clone()
        .with_additional_link_search_paths(additional_search_paths);

    let outputs = TargetOutputDescription::for_native(
        target,
        [
            TargetOutputKind::Executable,
            TargetOutputKind::RelocatableObject,
        ],
    );

    let request = EmissionRequest::try_new(
        product,
        ProductKind::Executable,
        native.executable_host().cloned(),
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(output.to_path_buf().into()),
        [
            RequestedArtifact::new(ArtifactKind::Executable, ArtifactRequirement::Required),
            RequestedArtifact::new(
                ArtifactKind::RelocatableObject,
                ArtifactRequirement::Optional,
            ),
        ],
        ReplacementPolicy::ReplaceExisting,
    )
    .map_err(|error| format!("native fixture emission request is invalid: {error:?}"))?;

    let inputs = ProductEmissionInputs::new(&outputs).with_native_product(&native, &linker);

    let outcome = compilation
        .emit_product(request, inputs)
        .map_err(|error| format!("native fixture emission failed: {:?}", error.kind()))?;

    if matches!(outcome.status(), EmissionStatus::Complete) {
        return Ok(());
    }

    Err(format!(
        "native fixture emission did not complete: {:?}; diagnostics={:?}",
        outcome.status(),
        outcome.diagnostics()
    ))
}
