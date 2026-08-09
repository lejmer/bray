use std::path::Path;

use bray_compilation::{
    BuildConfiguration, Compilation, ProductEmissionInputs, SelectedTarget,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_linker::LinkSearchPath;
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetOutputDescription, TargetOutputKind};
use bray_tooling::{load_runtime_artifact, native_linker};

pub(crate) fn emit_executable(
    compilation: Compilation,
    product: ProductIdentity,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    additional_search_paths: impl IntoIterator<Item = LinkSearchPath>,
) -> Result<(), String> {
    let selected = SelectedTarget::for_native(target);

    let linker = native_linker(target)
        .ok_or_else(|| format!("native linker is unavailable for {}", target.as_str()))?;

    let runtime = load_runtime_artifact(runtime)
        .ok_or_else(|| "native runtime artifact is invalid".to_owned())?;

    let native = compilation
        .native_product_facts(
            product.clone(),
            BuildConfiguration::Release,
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
        RequestedArtifactDestination::FilesystemDirectory(output.to_path_buf()),
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
