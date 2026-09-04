use std::path::Path;

use bray_compilation::{BuildConfiguration, Compilation, ProductEmissionInputs, SelectedTarget};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus,
    ManagedFilesystemDestination, ManagedOutputDirectory, ReplacementPolicy, RequestedArtifact,
    RequestedArtifactDestination,
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
) -> Result<bray_emitter::PublishedArtifact, String> {
    let selected = SelectedTarget::for_native(target);
    let destination = managed_destination(output)?;

    let linker = native_linker(target, None, destination.root()).map_err(|error| {
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
            Some(linker.linker()),
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
        RequestedArtifactDestination::FilesystemDirectory(destination.clone()),
        [RequestedArtifact::new(
            ArtifactKind::StaticLibrary,
            ArtifactRequirement::Required,
        )],
        ReplacementPolicy::ReplaceExisting,
    )
    .map_err(|error| format!("native library emission request is invalid: {error:?}"))?
    .with_storage_profile(BuildConfiguration::ObjectRelease.as_str());

    let inputs = ProductEmissionInputs::new(&outputs).with_native_product(&native, linker.linker());

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

    let artifact = outcome
        .artifacts()
        .artifacts()
        .iter()
        .find(|artifact| artifact.id().kind() == ArtifactKind::StaticLibrary)
        .ok_or_else(|| "native library emission did not publish its static archive".to_owned())?;

    bray_emitter::resolve_published_artifact(
        destination,
        artifact.id().product(),
        ArtifactKind::StaticLibrary,
        artifact.id().ordinal(),
    )
    .map_err(|error| format!("could not retain the published native archive: {error:?}"))
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
    let destination = managed_destination(output)?;

    let linker = native_linker(target, map_output, destination.root()).map_err(|error| {
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
            Some(linker.linker()),
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
        RequestedArtifactDestination::FilesystemDirectory(destination),
        [
            RequestedArtifact::new(ArtifactKind::Executable, ArtifactRequirement::Required),
            RequestedArtifact::new(
                ArtifactKind::RelocatableObject,
                ArtifactRequirement::Optional,
            ),
        ],
        ReplacementPolicy::ReplaceExisting,
    )
    .map_err(|error| format!("native fixture emission request is invalid: {error:?}"))?
    .with_storage_profile(configuration.as_str());

    let inputs = ProductEmissionInputs::new(&outputs).with_native_product(&native, linker.linker());

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

pub(crate) fn managed_destination(output: &Path) -> Result<ManagedFilesystemDestination, String> {
    let root = output
        .parent()
        .filter(|root| !root.as_os_str().is_empty())
        .ok_or_else(|| {
            format!(
                "native output directory has no storage root: {}",
                output.display()
            )
        })?;

    let directory = output
        .file_name()
        .and_then(|directory| directory.to_str())
        .and_then(ManagedOutputDirectory::try_new)
        .ok_or_else(|| format!("native output directory is invalid: {}", output.display()))?;

    Ok(ManagedFilesystemDestination::new(root, directory))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn native_outputs_share_storage_in_their_parent_build_directory() {
        let destination = super::managed_destination(Path::new("build/native/release")).unwrap();

        assert_eq!(destination.root(), Path::new("build/native"));
        assert_eq!(destination.directory(), Path::new("build/native/release"));
    }
}
