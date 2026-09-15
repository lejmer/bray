use std::fs;
use std::path::{Path, PathBuf};

use bray_base::NonEmptySharedStr;
use bray_compilation::{CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};
use bray_runtime_interface::{
    PlatformServiceBinding, PlatformServiceRole, RuntimeAbiRole, RuntimeRoleSourceBinding,
};
use bray_standard_library::StandardLibraryRoot;
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, PackageIdentity, ProductIdentity, ProductKind,
};
use bray_target::NativeTarget;
use bray_tooling::{load_llvm_compilation, source_inputs_from_file_arguments};

const PACKAGE_IDENTITY: &str = "bray_runtime_bootstrap";
const PRODUCT_NAME: &str = "runtime";
const MODULE: &str = "bray.runtime.bootstrap";

pub(super) fn build(root: &Path, target: NativeTarget, destination: &Path) -> Result<(), String> {
    let support = tempfile::Builder::new()
        .prefix("bray-runtime-bootstrap-")
        .tempdir()
        .map_err(|error| format!("could not create bootstrap build directory: {error}"))?;

    let standard_library = root
        .join("target/runtime-bootstrap-standard-library")
        .join(target.as_str());

    crate::standard_library::build_target_bundle(
        &root.join("standard-library"),
        &standard_library,
        target,
    )
    .map_err(|error| format!("could not build bootstrap standard library: {error}"))?;

    let source_root = root.join("runtime/bootstrap/src");
    let sources = source_paths(&source_root)?;

    let sources = source_inputs_from_file_arguments(sources)
        .map_err(|error| format!("could not load bootstrap source: {error:?}"))?;

    let package = PackageIdentity::try_new(PACKAGE_IDENTITY)
        .ok_or_else(|| "bootstrap package identity is invalid".to_owned())?;

    let product = ProductIdentity::try_new(package.clone(), PRODUCT_NAME)
        .ok_or_else(|| "bootstrap product identity is invalid".to_owned())?;

    let selected = SelectedTarget::for_native(target);

    let mut native_links = crate::standard_library::native_links(selected.profile().identity())
        .map_err(|error| format!("could not load bootstrap native link inputs: {error}"))?;

    native_links.push(NativeLinkRequirement::new(
        NonEmptySharedStr::try_new("bray_runtime_host")
            .unwrap_or_else(|| unreachable!("the native library name is non-empty")),
        NativeLinkKind::System,
    ));

    native_links.push(NativeLinkRequirement::new(
        NonEmptySharedStr::try_new("bray_runtime_callback")
            .unwrap_or_else(|| unreachable!("the native library name is non-empty")),
        NativeLinkKind::System,
    ));

    let options = CompilationOptions::new(WorkerBudget::default(), ProductKind::Library, selected)
        .with_native_link_inputs(native_links);

    let standard_library = StandardLibraryRoot::try_new(&standard_library)
        .ok_or_else(|| "bootstrap standard-library root is invalid".to_owned())?;

    let request = CompilationRequest::with_options(package, sources, options)
        .with_standard_library_root(standard_library)
        .with_platform_services(platform_bindings()?)
        .with_runtime_roles(runtime_bindings()?);

    let compilation = load_llvm_compilation(request)
        .map_err(|error| format!("bootstrap compiler backend is unavailable: {error}"))?;

    let diagnostics = compilation.check_diagnostics();

    if !diagnostics.is_empty() {
        let detail =
            crate::diagnostic_output::render_diagnostics(diagnostics, compilation.sources())
                .unwrap_or_else(|| "the bootstrap diagnostics could not be rendered".to_owned());

        return Err(format!("bootstrap source did not type-check:\n{detail}"));
    }

    let emission = support.path().join("emission");

    fs::create_dir_all(&emission)
        .map_err(|error| format!("could not create bootstrap emission directory: {error}"))?;

    let archive =
        crate::native_product::emit_static_library(&compilation, product, target, &emission)?;

    fs::copy(&archive, destination).map_err(|error| {
        format!(
            "could not publish bootstrap archive {}: {error}",
            destination.display()
        )
    })?;

    Ok(())
}

fn source_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(root)
        .map_err(|error| format!("could not read bootstrap source directory: {error}"))?;

    let mut paths = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("could not read bootstrap source entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "bray")
    });

    paths.sort();

    if paths.is_empty() {
        return Err("bootstrap source directory contains no Bray files".to_owned());
    }

    Ok(paths)
}

fn platform_bindings() -> Result<Vec<PlatformServiceBinding>, String> {
    PlatformServiceRole::ALL
        .iter()
        .copied()
        .filter_map(|role| {
            role.bootstrap_declaration()
                .map(|declaration| (role, declaration))
        })
        .map(|(role, declaration)| {
            let path = format!("{MODULE}.{declaration}");

            PlatformServiceBinding::try_new(role, &path)
                .ok_or_else(|| format!("bootstrap platform binding is invalid: {path}"))
        })
        .collect()
}

fn runtime_bindings() -> Result<Vec<RuntimeRoleSourceBinding>, String> {
    RuntimeAbiRole::ALL
        .into_iter()
        .filter_map(|role| {
            role.bootstrap_declaration()
                .map(|declaration| (role, declaration))
        })
        .chain([(RuntimeAbiRole::OutgoingAdmission, "outgoing_admission")])
        .map(|(role, declaration)| {
            let path = format!("{MODULE}.{declaration}");

            RuntimeRoleSourceBinding::try_new(role, &path)
                .ok_or_else(|| format!("bootstrap runtime binding is invalid: {path}"))
        })
        .collect()
}
