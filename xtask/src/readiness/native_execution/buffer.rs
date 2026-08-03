use std::path::Path;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, ProductEmissionInputs, SelectedTarget,
    WorkerBudget,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_runtime_interface::{RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactMetadata};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetOutputDescription, TargetOutputKind};
use bray_tooling::{load_llvm_compilation, native_linker, source_inputs_from_file_arguments};
use sha2::{Digest, Sha256};

use super::core::{PRODUCT_NAME, executable_path, execute_product, native_output};

const STANDARD_BUFFER_FIXTURES: &[&str] = &[
    "xtask/fixtures/native-execution/standard-raw-buffer.bray",
    "xtask/fixtures/native-execution/standard-buffer.bray",
    "xtask/fixtures/native-execution/standard-buffer-reserve.bray",
    "xtask/fixtures/native-execution/standard-buffer-from-slice.bray",
    "xtask/fixtures/native-execution/standard-buffer-push.bray",
    "xtask/fixtures/native-execution/standard-buffer-append.bray",
    "xtask/fixtures/native-execution/standard-buffer-resize.bray",
    "xtask/fixtures/native-execution/standard-buffer-truncate.bray",
    "xtask/fixtures/native-execution/standard-buffer-clear.bray",
    "xtask/fixtures/native-execution/standard-buffer-pop.bray",
];
const STANDARD_BUFFER_MEMORY_FIXTURE: &str =
    "xtask/fixtures/native-execution/standard-buffer-memory.bray";
const STANDARD_BYTES_SOURCE: &str = "standard-library/std/src/bytes.bray";
const STANDARD_BYTES_IMPLEMENTATION: &str = "standard-library/std/src/bytes_impl.bray";
const STANDARD_CHARACTER_SOURCE: &str = "standard-library/std/src/character.bray";
const STANDARD_FORMAT_SOURCE: &str = "standard-library/std/src/format.bray";
const STANDARD_FORMAT_IMPLEMENTATION: &str = "standard-library/std/src/format_impl.bray";
const STANDARD_FORMAT_FIXTURE: &str = "xtask/fixtures/native-execution/standard-format.bray";
const STANDARD_STRING_SOURCE: &str = "standard-library/std/src/string.bray";

pub(super) fn audit_standard_buffer(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    for fixture in STANDARD_BUFFER_FIXTURES {
        let output = native_output("bray-native-standard-buffer-")?;

        let fixtures = [
            STANDARD_BUFFER_MEMORY_FIXTURE,
            STANDARD_BYTES_SOURCE,
            STANDARD_BYTES_IMPLEMENTATION,
            fixture,
        ];

        build_standard_library_fixtures(root, target, runtime, output.path(), &fixtures)
            .map_err(|error| format!("{fixture}: {error}"))?;

        let context = format!("executing standard byte-buffer fixture {fixture}");

        execute_product(&executable_path(output.path(), target), 0, &context)?;
    }

    Ok(())
}

pub(super) fn audit_standard_format(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    let output = native_output("bray-native-standard-format-")?;

    let fixtures = [
        STANDARD_BUFFER_MEMORY_FIXTURE,
        STANDARD_BYTES_SOURCE,
        STANDARD_BYTES_IMPLEMENTATION,
        STANDARD_STRING_SOURCE,
        STANDARD_CHARACTER_SOURCE,
        STANDARD_FORMAT_SOURCE,
        STANDARD_FORMAT_IMPLEMENTATION,
        STANDARD_FORMAT_FIXTURE,
    ];

    build_standard_library_fixtures(root, target, runtime, output.path(), &fixtures)?;

    execute_product(
        &executable_path(output.path(), target),
        0,
        "executing standard formatting fixture",
    )
}

pub(super) fn build_standard_library_fixtures(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
    output: &Path,
    fixtures: &[&str],
) -> Result<(), String> {
    let compilation = standard_library_compilation(root, target, fixtures)?;

    let package = PackageIdentity::try_new("std")
        .ok_or_else(|| "standard-library package identity is invalid".to_owned())?;

    let product = ProductIdentity::try_new(package, PRODUCT_NAME)
        .ok_or_else(|| "standard-library fixture product identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let linker = native_linker(target)
        .ok_or_else(|| format!("native linker is unavailable for {}", target.as_str()))?;

    let runtime = load_runtime_artifact(runtime)?;

    let native = compilation
        .native_product_facts(product.clone(), Some(runtime), [], Some(&linker))
        .map_err(|error| {
            format!(
                "could not build standard byte-buffer product: {error:?}; diagnostics={:?}",
                compilation.check_diagnostics()
            )
        })?;

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
    .map_err(|error| format!("standard byte-buffer emission request is invalid: {error:?}"))?;

    let inputs = ProductEmissionInputs::new(&outputs).with_native_product(&native, &linker);

    let outcome = compilation.emit_product(request, inputs).map_err(|error| {
        format!(
            "standard byte-buffer emission failed: {:?}; diagnostics={:?}",
            error.kind(),
            compilation.check_diagnostics()
        )
    })?;

    if matches!(outcome.status(), EmissionStatus::Complete) {
        Ok(())
    } else {
        Err(format!(
            "standard byte-buffer emission did not complete: {:?}; diagnostics={:?}",
            outcome.status(),
            outcome.diagnostics()
        ))
    }
}

pub(super) fn standard_library_compilation(
    root: &Path,
    target: NativeTarget,
    fixtures: &[&str],
) -> Result<Compilation, String> {
    let source_paths = fixtures.iter().map(|fixture| root.join(fixture));

    let sources = source_inputs_from_file_arguments(source_paths)
        .map_err(|error| format!("could not load standard-library fixture source: {error:?}"))?;

    let package = PackageIdentity::try_new("std")
        .ok_or_else(|| "standard-library package identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        selected.clone(),
    );

    let request = CompilationRequest::with_options(package, sources, options)
        .with_standard_library_source_authority();

    load_llvm_compilation(request).ok_or_else(|| "LLVM compiler backend is unavailable".to_owned())
}

fn load_runtime_artifact(metadata_path: &Path) -> Result<RuntimeArtifact, String> {
    let metadata_bytes = std::fs::read(metadata_path)
        .map_err(|error| format!("could not read runtime metadata: {error}"))?;

    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes)
        .map_err(|error| format!("could not decode runtime metadata: {error:?}"))?;

    let parent = metadata_path
        .parent()
        .ok_or_else(|| "runtime metadata has no parent directory".to_owned())?;

    let archive = parent.join(metadata.archive_file_name());

    let archive_bytes = std::fs::read(&archive)
        .map_err(|error| format!("could not read runtime archive: {error}"))?;

    let digest = RuntimeArtifactDigest::new(Sha256::digest(&archive_bytes).into());

    RuntimeArtifact::try_new(metadata, archive, digest)
        .map_err(|error| format!("runtime artifact is invalid: {error:?}"))
}
