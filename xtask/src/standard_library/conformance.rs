use std::fs;
use std::path::Path;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_diagnostics::DiagnosticKind;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    InterfaceValidationPolicy, ValidatedPackageInterface,
};
use bray_project::{PACKAGE_MANIFEST_FILE_NAME, WORKSPACE_MANIFEST_FILE_NAME};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY, STANDARD_LIBRARY_MANIFEST_FILE_NAME,
    StandardLibraryBundleManifest, StandardLibraryLoadError, StandardLibraryResolver,
    StandardLibraryRoot,
};
use bray_symbols::{PackageIdentity, ProductKind};
use bray_target::{NativeTarget, TargetIdentity};

use super::command::{BuildError, build, compare_bundles, read_manifest, write_bundle_artifact};

pub(super) fn verify(scratch: &Path) -> Result<(), BuildError> {
    let first = scratch.join("first");
    let second = scratch.join("second");
    let source = scratch.join("source");

    write_synthetic_project(&source)?;
    build(&source, &first)?;
    build(&source, &second)?;
    compare_bundles(&first, &second)?;

    verify_bundle(&first, scratch)
}

fn verify_bundle(bundle: &Path, scratch: &Path) -> Result<(), BuildError> {
    let manifest = read_manifest(bundle)?;
    let resolver = resolver(bundle)?;

    verify_package_interfaces(&resolver, &manifest)?;
    verify_configured_root(bundle)?;
    verify_target_selection(&resolver, &manifest)?;
    verify_missing_artifact_diagnostic(bundle, &scratch.join("missing-artifact"))?;

    verify_artifact_integrity(bundle, &scratch.join("corrupted-artifact"), &manifest)
}

fn write_synthetic_project(root: &Path) -> Result<(), BuildError> {
    let target = NativeTarget::current().ok_or_else(|| {
        BuildError::conformance(
            "production-build",
            "the compiler host has no supported native target",
        )
    })?;

    write_file(
        &root.join(WORKSPACE_MANIFEST_FILE_NAME),
        &format!(
            r#"{{
                "format": 1,
                "package": {{"version": "0.1.0"}},
                "output_root": "build",
                "targets": [{{"name": "native", "identity": "{}"}}],
                "packages": [{{"path": "std", "role": "root"}}]
            }}"#,
            target.as_str()
        ),
    )?;

    write_file(
        &root.join("std").join(PACKAGE_MANIFEST_FILE_NAME),
        r#"{
            "format": 1,
            "identity": "std",
            "version": {"workspace": true},
            "source_roots": [{"name": "library", "path": "src"}],
            "products": [{
                "name": "library",
                "kind": "library",
                "source_roots": ["library"],
                "targets": ["native"],
                "outputs": ["package_interface", "static_library"]
            }]
        }"#,
    )?;

    write_file(
        &root.join("std").join("src").join("std.bray"),
        "module std;\n",
    )
}

fn write_file(path: &Path, contents: &str) -> Result<(), BuildError> {
    let Some(parent) = path.parent() else {
        return Err(BuildError::conformance(
            "production-build",
            format!("{} has no parent directory", path.display()),
        ));
    };

    fs::create_dir_all(parent).map_err(|error| {
        BuildError::conformance(
            "production-build",
            format!("could not create {}: {error}", parent.display()),
        )
    })?;

    fs::write(path, contents).map_err(|error| {
        BuildError::conformance(
            "production-build",
            format!("could not write {}: {error}", path.display()),
        )
    })
}

fn verify_package_interfaces(
    resolver: &StandardLibraryResolver,
    manifest: &StandardLibraryBundleManifest,
) -> Result<(), BuildError> {
    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    for target in manifest.targets() {
        let artifact = resolver
            .interface(target.target(), target.runtime_abi())
            .map_err(|error| BuildError::conformance("package-interface", format!("{error:?}")))?;

        verify_package_interface(artifact.shared_bytes(), policy)?;
    }

    Ok(())
}

fn verify_package_interface(
    bytes: std::sync::Arc<[u8]>,
    policy: InterfaceValidationPolicy,
) -> Result<(), BuildError> {
    let validated = ValidatedPackageInterface::try_new(bytes, policy)
        .map_err(|error| BuildError::conformance("package-interface", format!("{error:?}")))?;

    let surface = validated
        .decode_identity_surface()
        .map_err(|error| BuildError::conformance("package-interface", format!("{error:?}")))?;

    validated
        .decode_semantics(&surface)
        .map_err(|error| BuildError::conformance("package-interface", format!("{error:?}")))?;

    let identity = surface.identity();

    if identity.package().as_str() != PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
        || identity.product().as_str() != PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY
        || identity.kind() != InterfaceProductKind::Library
        || identity.public_surface() != PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY
    {
        return Err(BuildError::conformance(
            "package-interface",
            "the interface does not publish the canonical std:library surface",
        ));
    }

    Ok(())
}

fn verify_configured_root(bundle: &Path) -> Result<(), BuildError> {
    let compilation = compilation(bundle)?;
    let result = standard_library_interface_result(&compilation)?;
    let diagnostics = diagnostic_kinds(result.diagnostics());

    if !diagnostics.is_empty() {
        return Err(BuildError::conformance(
            "configured-root",
            format!("the configured interface produced diagnostics: {diagnostics:?}"),
        ));
    }

    if result.value().is_none() {
        return Err(BuildError::conformance(
            "configured-root",
            "the configured interface did not produce an imported surface",
        ));
    }

    Ok(())
}

fn verify_target_selection(
    resolver: &StandardLibraryResolver,
    manifest: &StandardLibraryBundleManifest,
) -> Result<(), BuildError> {
    for selected in manifest.targets() {
        let resolved = resolver
            .target_artifacts(selected.target(), selected.runtime_abi())
            .map_err(|error| BuildError::conformance("target-selection", format!("{error:?}")))?;

        if resolved.len() != selected.artifacts().len()
            || !resolved
                .iter()
                .zip(selected.artifacts())
                .all(|(actual, expected)| actual.metadata() == expected)
        {
            return Err(BuildError::conformance(
                "target-selection",
                format!(
                    "the resolver selected the wrong artifact set for {}",
                    selected.target().as_str()
                ),
            ));
        }

        let incompatible = incompatible_runtime_abi(selected.runtime_abi());

        if !matches!(
            resolver.target_artifacts(selected.target(), incompatible),
            Err(StandardLibraryLoadError::RuntimeAbiMismatch {
                target,
                expected,
                actual,
            }) if target == *selected.target()
                && expected == incompatible
                && actual == selected.runtime_abi()
        ) {
            return Err(BuildError::conformance(
                "runtime-abi-selection",
                format!(
                    "the resolver accepted an incompatible runtime ABI for {}",
                    selected.target().as_str()
                ),
            ));
        }
    }

    let unavailable = TargetIdentity::try_new("bray-conformance-unavailable-target")
        .ok_or(BuildError::InvalidIdentity)?;

    if manifest
        .targets()
        .iter()
        .any(|selected| selected.target() == &unavailable)
    {
        return Err(BuildError::conformance(
            "target-selection",
            "the conformance-only unavailable target appears in the bundle",
        ));
    }

    if !matches!(
        resolver.target_artifacts(&unavailable, RuntimeAbiVersion::new(0, 0)),
        Err(StandardLibraryLoadError::TargetUnavailable(target)) if target == unavailable
    ) {
        return Err(BuildError::conformance(
            "target-selection",
            "the resolver accepted a target absent from the manifest",
        ));
    }

    Ok(())
}

fn verify_missing_artifact_diagnostic(bundle: &Path, missing: &Path) -> Result<(), BuildError> {
    write_manifest(bundle, missing)?;

    let compilation = compilation(missing)?;
    let result = standard_library_interface_result(&compilation)?;
    let diagnostics = diagnostic_kinds(result.diagnostics());

    if diagnostics != [DiagnosticKind::StandardLibraryArtifactReadFailed] {
        return Err(BuildError::conformance(
            "missing-artifact-diagnostic",
            format!("unexpected diagnostics: {diagnostics:?}"),
        ));
    }

    Ok(())
}

fn verify_artifact_integrity(
    bundle: &Path,
    corrupted: &Path,
    manifest: &StandardLibraryBundleManifest,
) -> Result<(), BuildError> {
    let Some(selected) = manifest.targets().first() else {
        return Err(BuildError::conformance(
            "artifact-integrity",
            "the manifest contains no target artifact set",
        ));
    };

    let Some(artifact) = selected.artifacts().first() else {
        return Err(BuildError::conformance(
            "artifact-integrity",
            "the selected target contains no artifact",
        ));
    };

    let source = artifact.beneath(bundle);
    let mut bytes = fs::read(&source).map_err(|error| BuildError::read(&source, error))?;

    let Some(first) = bytes.first_mut() else {
        return Err(BuildError::conformance(
            "artifact-integrity",
            "the selected artifact is empty",
        ));
    };

    *first ^= u8::MAX;

    write_manifest(bundle, corrupted)?;
    write_bundle_artifact(corrupted, artifact.path(), &bytes)?;

    let resolver = resolver(corrupted)?;

    if !matches!(
        resolver.target_artifacts(selected.target(), selected.runtime_abi()),
        Err(StandardLibraryLoadError::ArtifactDigestMismatch { .. })
    ) {
        return Err(BuildError::conformance(
            "artifact-integrity",
            "the resolver accepted artifact bytes that disagree with the manifest digest",
        ));
    }

    Ok(())
}

fn compilation(bundle: &Path) -> Result<Compilation, BuildError> {
    let target = NativeTarget::current().ok_or_else(|| {
        BuildError::conformance(
            "configured-root",
            "the compiler host has no supported native target",
        )
    })?;

    let package =
        PackageIdentity::try_new("bray.conformance").ok_or(BuildError::InvalidIdentity)?;

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "standard-library-conformance",
        SourceVersion::new(0),
        "module conformance;",
    );

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Library,
        SelectedTarget::for_native(target),
    );

    let request = CompilationRequest::with_options(package, [source].into(), options)
        .with_standard_library_root(standard_library_root(bundle)?);

    Compilation::load(request)
        .map_err(|error| BuildError::conformance("configured-root", format!("{error:?}")))
}

fn standard_library_interface_result(
    compilation: &Compilation,
) -> Result<
    &bray_diagnostics::DiagnosticResult<Option<bray_package_interface::PackageInterfaceSurface>>,
    BuildError,
> {
    let package = PackageIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)?;

    let product = InterfaceProductIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY)
        .ok_or(BuildError::InvalidIdentity)?;

    let interface = compilation
        .dependency_interface_id(&package, &product)
        .ok_or_else(|| {
            BuildError::conformance(
                "configured-root",
                "the compilation did not register the std:library dependency",
            )
        })?;

    compilation
        .dependency_interface_result(interface)
        .ok_or_else(|| {
            BuildError::conformance(
                "configured-root",
                "the compilation did not publish a standard library interface result",
            )
        })
}

fn write_manifest(bundle: &Path, destination: &Path) -> Result<(), BuildError> {
    let source = bundle.join(STANDARD_LIBRARY_MANIFEST_FILE_NAME);
    let bytes = fs::read(&source).map_err(|error| BuildError::read(&source, error))?;

    write_bundle_artifact(destination, STANDARD_LIBRARY_MANIFEST_FILE_NAME, &bytes)
}

fn resolver(bundle: &Path) -> Result<StandardLibraryResolver, BuildError> {
    standard_library_root(bundle).map(StandardLibraryResolver::new)
}

fn standard_library_root(bundle: &Path) -> Result<StandardLibraryRoot, BuildError> {
    StandardLibraryRoot::try_new(bundle).ok_or_else(|| {
        BuildError::conformance(
            "configured-root",
            "the synthetic bundle root is not absolute",
        )
    })
}

fn diagnostic_kinds(diagnostics: &bray_diagnostics::DiagnosticBag) -> Vec<DiagnosticKind> {
    diagnostics
        .iter()
        .map(bray_diagnostics::Diagnostic::kind)
        .collect()
}

fn incompatible_runtime_abi(actual: RuntimeAbiVersion) -> RuntimeAbiVersion {
    let zero = RuntimeAbiVersion::new(0, 0);

    if actual == zero {
        RuntimeAbiVersion::new(0, 1)
    } else {
        zero
    }
}

#[cfg(test)]
mod tests {
    use super::verify;

    #[test]
    fn synthetic_standard_library_bundles_satisfy_conformance() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary root must exist: {error}"));

        verify(directory.path())
            .unwrap_or_else(|error| panic!("synthetic bundle must conform: {error}"));
    }
}
