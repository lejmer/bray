use std::fs;
use std::path::{Path, PathBuf};

use bray_compilation::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeUnit, NativeUnitKind, scan_object_unit_summary,
};
use bray_runtime_interface::{
    PlatformServiceRole, RuntimeAbiRole, RuntimeArtifactMetadata, RuntimeArtifactPurpose,
    RuntimeNativeIndexMetadata,
};
use bray_source::{SourceIdentity, SourceInput};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::NativeTarget;
use bray_tooling::load_llvm_compilation;
use sha2::{Digest, Sha256};

use super::command::BuildError;

const CHECK: &str = "filesystem provider overrides";
const OVERRIDES: &[PlatformServiceRole] = &[
    PlatformServiceRole::FileOpen,
    PlatformServiceRole::FileClose,
    PlatformServiceRole::FileRead,
    PlatformServiceRole::FileWrite,
    PlatformServiceRole::FileFlush,
    PlatformServiceRole::FileSeek,
    PlatformServiceRole::FileMetadata,
    PlatformServiceRole::PathMetadata,
    PlatformServiceRole::DirectoryOpen,
    PlatformServiceRole::DirectoryNext,
    PlatformServiceRole::DirectoryClose,
];

pub(super) fn audit(
    root: &Path,
    output: &Path,
    toolchain: &Path,
    runtime: &Path,
    target: NativeTarget,
) -> Result<(), BuildError> {
    let output = output.join("filesystem-provider-override");

    fs::create_dir(&output).map_err(|error| BuildError::write(&output, error))?;

    let compiler =
        bray_tooling::llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver)
            .map_err(|error| {
                BuildError::conformance(CHECK, format!("compiler unavailable: {error}"))
            })?;

    let object = output.join("provider.o");

    crate::native_product::compile_c_fixture(
        &compiler,
        &root.join("xtask/fixtures/filesystem-provider-override.c"),
        &object,
        &[],
    )
    .map_err(|error| BuildError::conformance(CHECK, error))?;

    let runtime = runtime_with_provider(runtime, &output, &object, target)?;
    let source_path = root.join("xtask/fixtures/filesystem-provider-override.bray");

    let source =
        fs::read_to_string(&source_path).map_err(|error| BuildError::read(&source_path, error))?;

    let package = PackageIdentity::try_new("bray.filesystem_provider_fixture")
        .ok_or(BuildError::InvalidIdentity)?;

    let product = ProductIdentity::try_new(package.clone(), "filesystem-provider")
        .ok_or(BuildError::InvalidIdentity)?;

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Executable,
        SelectedTarget::for_native(target),
    );

    let root = StandardLibraryRoot::try_new(toolchain.join("lib/bray/standard-library"))
        .ok_or_else(|| BuildError::conformance(CHECK, "invalid standard-library root"))?;

    let request = CompilationRequest::with_options(
        package,
        vec![SourceInput::virtual_text(
            SourceIdentity::new(1),
            "filesystem-provider-override.bray",
            1,
            source,
        )],
        options,
    )
    .with_standard_library_root(root);

    let compilation = load_llvm_compilation(request).map_err(|error| {
        BuildError::conformance(CHECK, format!("compiler unavailable: {error}"))
    })?;

    let native_output = output.join("native-product");

    fs::create_dir(&native_output).map_err(|error| BuildError::write(&native_output, error))?;

    crate::native_product::emit_executable(
        &compilation,
        product.clone(),
        target,
        &runtime,
        &native_output,
        [],
        None,
    )
    .map_err(|error| BuildError::conformance(CHECK, error))?;

    let destination = crate::native_product::managed_destination(&native_output)
        .map_err(|error| BuildError::conformance(CHECK, error))?;

    let executable = super::artifact::resolve_executable(destination, &product, CHECK)?;

    crate::validation_process::success(
        subprocess::Exec::cmd(executable.path()).cwd(&output),
        crate::validation_process::TIMEOUT,
    )
    .map_err(|error| BuildError::conformance(CHECK, error.to_string()))?;

    Ok(())
}

fn runtime_with_provider(
    runtime: &Path,
    output: &Path,
    object: &Path,
    target: NativeTarget,
) -> Result<PathBuf, BuildError> {
    let source = runtime
        .parent()
        .ok_or_else(|| BuildError::conformance(CHECK, "runtime has no parent"))?;

    let bytes = fs::read(runtime).map_err(|error| BuildError::read(runtime, error))?;

    let metadata = RuntimeArtifactMetadata::decode_json(&bytes)
        .map_err(|error| BuildError::conformance(CHECK, format!("runtime metadata: {error:?}")))?;

    let product_index = metadata
        .native_indexes()
        .iter()
        .find(|index| index.purpose() == RuntimeArtifactPurpose::Product)
        .ok_or_else(|| BuildError::conformance(CHECK, "product runtime index is missing"))?;

    let index_path = source.join(product_index.file_name());

    let index_bytes =
        fs::read(&index_path).map_err(|error| BuildError::read(&index_path, error))?;

    let index = NativeArtifactIndex::decode(&index_bytes, product_index.digest(), target)
        .map_err(|error| BuildError::conformance(CHECK, format!("runtime index: {error:?}")))?;

    let object_bytes = fs::read(object).map_err(|error| BuildError::read(object, error))?;
    let digest = NativeContentDigest::new(Sha256::digest(&object_bytes).into());

    let summary = scan_object_unit_summary(&object_bytes)
        .map_err(|error| BuildError::conformance(CHECK, format!("provider object: {error}")))?;

    let unit = NativeUnit::new(digest, NativeUnitKind::Object, summary, []);

    let mut revised = NativeArtifactIndex::try_new(
        target,
        index.producer(),
        index.units().iter().cloned().chain([unit]),
        index.co_retention_groups().iter().cloned(),
    )
    .map_err(|error| BuildError::conformance(CHECK, format!("provider index: {error:?}")))?;

    if let Some(revision) = index.bitcode_toolchain() {
        revised = revised.with_bitcode_toolchain(revision);
    }

    let revised_bytes = revised.encode().map_err(|error| {
        BuildError::conformance(CHECK, format!("provider index encoding: {error:?}"))
    })?;

    let revised_digest = NativeContentDigest::new(Sha256::digest(&revised_bytes).into());
    let destination = output.join("runtime");
    let payloads = destination.join("native");

    fs::create_dir_all(&payloads).map_err(|error| BuildError::write(&payloads, error))?;

    for entry in fs::read_dir(source.join("native"))
        .map_err(|error| BuildError::read(&source.join("native"), error))?
    {
        let entry = entry.map_err(|error| BuildError::read(source, error))?;
        let target_path = payloads.join(entry.file_name());

        fs::hard_link(entry.path(), &target_path)
            .or_else(|_| fs::copy(entry.path(), &target_path).map(|_| ()))
            .map_err(|error| BuildError::write(&target_path, error))?;
    }

    let provider_path = payloads.join(NativeUnitKind::Object.file_name(digest, target));

    fs::write(&provider_path, object_bytes)
        .map_err(|error| BuildError::write(&provider_path, error))?;

    let mut indexes = Vec::new();

    for index in metadata.native_indexes() {
        let path = destination.join(index.file_name());

        if index.purpose() == RuntimeArtifactPurpose::Product {
            fs::write(&path, &revised_bytes).map_err(|error| BuildError::write(&path, error))?;

            indexes.push(
                RuntimeNativeIndexMetadata::try_new(
                    index.purpose(),
                    index.file_name(),
                    revised_digest,
                )
                .map_err(|error| {
                    BuildError::conformance(CHECK, format!("runtime index metadata: {error:?}"))
                })?,
            );
        } else {
            fs::copy(source.join(index.file_name()), &path)
                .map_err(|error| BuildError::write(&path, error))?;

            indexes.push(index.clone());
        }
    }

    let mut found = false;

    let components = metadata
        .components()
        .iter()
        .cloned()
        .map(|component| {
            if component.purpose() == RuntimeArtifactPurpose::Product
                && component
                    .roles()
                    .contains(&RuntimeAbiRole::RuntimeInitialization)
            {
                found = true;

                let services = component
                    .platform_services()
                    .iter()
                    .copied()
                    .chain(OVERRIDES.iter().copied())
                    .collect::<Vec<_>>();

                component.with_platform_services(services)
            } else {
                component
            }
        })
        .collect::<Vec<_>>();

    if !found {
        return Err(BuildError::conformance(
            CHECK,
            "product runtime host is missing",
        ));
    }

    let revised =
        RuntimeArtifactMetadata::try_new(metadata.contract().clone(), components, indexes)
            .map_err(|error| {
                BuildError::conformance(CHECK, format!("provider runtime metadata: {error:?}"))
            })?;

    let bytes = revised
        .encode_json()
        .map_err(|error| BuildError::conformance(CHECK, format!("runtime encoding: {error:?}")))?;

    let path = destination.join("bray-runtime.brayrt");

    fs::write(&path, bytes).map_err(|error| BuildError::write(&path, error))?;

    Ok(path)
}
