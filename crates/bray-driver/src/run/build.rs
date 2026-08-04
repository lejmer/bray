use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use bray_base::{FileReplacementMode, StagedFile};
use bray_compilation::ProductEmissionInputs;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    SeverityKind,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_runtime_interface::RuntimeArtifact;
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{TargetOutputDescription, TargetOutputKind};
use bray_tooling::{
    OutputFormat, exit_code_from_diagnostics, load_llvm_compilation, load_runtime_artifact,
    native_linker,
};

use super::execute::{DriverRunResult, compilation_request, driver_result_from_compilation};
use crate::command::{
    DriverBackend, DriverOptions, DriverProductConfiguration, DriverRuntimeSelection,
};

pub(crate) fn run_build_command(
    options: &DriverOptions,
    configuration: DriverProductConfiguration,
    files: Vec<PathBuf>,
    output_format: OutputFormat,
) -> DriverRunResult {
    let compilation_configuration = options.compilation();
    let native_target = compilation_configuration.target();
    let selected_target = bray_compilation::SelectedTarget::for_native(native_target);
    let product = compilation_configuration.product().clone();
    let product_kind = compilation_configuration.product_kind();
    let artifacts = required_artifacts(product_kind, native_target, &configuration);
    let export_interface = artifacts.contains(&TargetOutputKind::PackageInterface);

    let request = match compilation_request(options, files, export_interface) {
        Ok(request) => request,
        Err(diagnostics) => {
            let exit_code = exit_code_from_diagnostics(&diagnostics);

            return DriverRunResult::new(exit_code, diagnostics, output_format);
        }
    };

    let compilation = match configuration.backend() {
        DriverBackend::Llvm => load_llvm_compilation(request),
    };

    let compilation = match compilation {
        Some(compilation) => compilation,
        None => {
            return DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new(), output_format);
        }
    };

    let linked = artifacts.iter().any(|artifact| is_linked(*artifact));

    let requires_codegen = artifacts
        .iter()
        .copied()
        .map(ArtifactKind::from)
        .any(|artifact| artifact.backend_kind().is_some());

    let requires_generation = linked || requires_codegen;

    let linker = if linked {
        match native_linker(native_target) {
            Some(linker) => Some(linker),
            None => return unsupported_product_result(compilation, output_format),
        }
    } else {
        None
    };

    let native = if requires_generation {
        let runtime = match resolve_runtime(configuration.runtime(), &selected_target) {
            Ok(runtime) => runtime,
            Err(()) => return unsupported_product_result(compilation, output_format),
        };

        match compilation.native_product_facts(
            product.clone(),
            configuration.build(),
            runtime,
            configuration.required_capabilities().iter().copied(),
            linker.as_ref(),
        ) {
            Ok(native) => Some(native),
            Err(error) if error.is_unsupported() => {
                return unsupported_product_result(compilation, output_format);
            }
            Err(error) => {
                return native_product_failure_result(compilation, output_format, &error);
            }
        }
    } else {
        None
    };

    let target_outputs = target_outputs(native_target, &artifacts, &configuration);

    let request = emission_request(
        product.clone(),
        product_kind,
        &selected_target,
        &artifacts,
        &configuration,
        native
            .as_ref()
            .and_then(|native| native.executable_host().cloned()),
    );

    let mut inputs = ProductEmissionInputs::new(&target_outputs);

    match (native.as_ref(), linker.as_ref()) {
        (Some(native), Some(linker)) => {
            inputs = inputs.with_native_product(native, linker);
        }
        (Some(native), None) => {
            inputs = inputs.with_native_codegen(native);
        }
        (None, _) => {}
    }

    match compilation.emit_product(request, inputs) {
        Ok(outcome) => {
            let diagnostics = outcome.diagnostics().clone();

            let exit_code = match outcome.status() {
                EmissionStatus::Complete => exit_code_from_diagnostics(&diagnostics),
                EmissionStatus::Failed(_) | EmissionStatus::Cancelled => ExitCode::FAILURE,
            };

            if exit_code == ExitCode::SUCCESS
                && let Some(destination) = configuration.test_catalog()
                && let Err(diagnostic) = publish_test_catalog(&compilation, &product, destination)
            {
                return driver_result_from_compilation(
                    compilation,
                    DiagnosticBag::single(diagnostic),
                    output_format,
                    ExitCode::FAILURE,
                );
            }

            driver_result_from_compilation(compilation, diagnostics, output_format, exit_code)
        }
        Err(error) => driver_result_from_compilation(
            compilation,
            error.diagnostics().clone(),
            output_format,
            ExitCode::FAILURE,
        ),
    }
}

fn publish_test_catalog(
    compilation: &bray_compilation::Compilation,
    product: &ProductIdentity,
    destination: &std::path::Path,
) -> Result<(), Diagnostic> {
    let discovery = compilation
        .test_discovery(product.clone())
        .map_err(|_| catalog_publication_diagnostic(destination, io::ErrorKind::InvalidData))?;

    let (bytes, _) = bray_test_protocol::encode_test_catalog(discovery.value().catalog())
        .map_err(|_| catalog_publication_diagnostic(destination, io::ErrorKind::InvalidData))?;

    let mut staging = StagedFile::create(destination, FileReplacementMode::ReplaceExisting, None)
        .map_err(|error| catalog_publication_diagnostic(destination, error.kind()))?;

    staging
        .write_all(&bytes)
        .map_err(|error| catalog_publication_diagnostic(destination, error.kind()))?;

    staging
        .finish()
        .and_then(|staged| staged.promote(destination))
        .map_err(|error| catalog_publication_diagnostic(destination, error.kind()))
}

fn catalog_publication_diagnostic(path: &std::path::Path, kind: io::ErrorKind) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionArtifactWriteFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path(path))
    .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
        kind,
    )))
}

fn required_artifacts(
    product_kind: ProductKind,
    target: bray_target::NativeTarget,
    configuration: &DriverProductConfiguration,
) -> Vec<TargetOutputKind> {
    let mut artifacts = if configuration.artifacts().is_empty() {
        match product_kind {
            ProductKind::Library => vec![
                TargetOutputKind::PackageInterface,
                TargetOutputKind::StaticLibrary,
            ],
            ProductKind::Executable | ProductKind::Test => vec![TargetOutputKind::Executable],
        }
    } else {
        configuration.artifacts().to_vec()
    };

    let needs_debug_companion = configuration
        .build()
        .requires_linked_debug_companion(target.object_format())
        && artifacts.iter().any(|artifact| {
            matches!(
                artifact,
                TargetOutputKind::Executable | TargetOutputKind::SharedLibrary
            )
        });

    if needs_debug_companion && !artifacts.contains(&TargetOutputKind::LinkedCompanion) {
        artifacts.push(TargetOutputKind::LinkedCompanion);
    }

    artifacts
}

const fn is_linked(artifact: TargetOutputKind) -> bool {
    matches!(
        artifact,
        TargetOutputKind::Executable
            | TargetOutputKind::StaticLibrary
            | TargetOutputKind::SharedLibrary
            | TargetOutputKind::LinkedCompanion
    )
}

fn target_outputs(
    target: bray_target::NativeTarget,
    artifacts: &[TargetOutputKind],
    configuration: &DriverProductConfiguration,
) -> TargetOutputDescription {
    let inspections =
        configuration
            .inspections()
            .iter()
            .copied()
            .map(|inspection| match inspection.artifact_kind() {
                ArtifactKind::Assembly => TargetOutputKind::Assembly,
                ArtifactKind::BackendIr => TargetOutputKind::BackendIr,
                ArtifactKind::BackendBitcode => TargetOutputKind::BackendBitcode,
                ArtifactKind::RelocatableObject => TargetOutputKind::RelocatableObject,
                _ => {
                    unreachable!("driver inspection selections cover only backend inspection kinds")
                }
            });

    TargetOutputDescription::for_native(target, artifacts.iter().copied().chain(inspections))
}

fn emission_request(
    product: ProductIdentity,
    product_kind: ProductKind,
    selected: &bray_compilation::SelectedTarget,
    artifacts: &[TargetOutputKind],
    configuration: &DriverProductConfiguration,
    executable_host: Option<bray_runtime_interface::ExecutableHostContract>,
) -> EmissionRequest {
    let required = artifacts.iter().copied().map(|artifact| {
        RequestedArtifact::new(ArtifactKind::from(artifact), ArtifactRequirement::Required)
    });

    let inspections = configuration.inspections().iter().copied().map(|artifact| {
        RequestedArtifact::new(artifact.artifact_kind(), ArtifactRequirement::Optional)
    });

    EmissionRequest::try_new(
        product,
        product_kind,
        executable_host,
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(configuration.output().to_path_buf()),
        required.chain(inspections),
        ReplacementPolicy::ReplaceExisting,
    )
    .unwrap_or_else(|error| panic!("validated build emission request must be valid: {error:?}"))
}

fn resolve_runtime(
    selection: Option<&DriverRuntimeSelection>,
    target: &bray_compilation::SelectedTarget,
) -> Result<Option<RuntimeArtifact>, ()> {
    let Some(selection) = selection else {
        return Ok(None);
    };

    let metadata = match selection {
        DriverRuntimeSelection::Artifact(path) => path.clone(),
        DriverRuntimeSelection::Profile(profile) => {
            runtime_profile_metadata(target, profile.as_str()).ok_or(())?
        }
    };

    load_runtime_artifact(&metadata).map(Some).ok_or(())
}

fn runtime_profile_metadata(
    target: &bray_compilation::SelectedTarget,
    profile: &str,
) -> Option<PathBuf> {
    let executable = env::current_exe().ok()?;

    executable
        .ancestors()
        .map(|ancestor| {
            ancestor
                .join("runtimes")
                .join(target.profile().identity().as_str())
                .join(profile)
                .join("bray-runtime.brayrt")
        })
        .find(|path| path.is_file())
}

fn unsupported_product_result(
    compilation: bray_compilation::Compilation,
    output_format: OutputFormat,
) -> DriverRunResult {
    let unsupported = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RequestUnsupportedProductEmission,
        SeverityKind::Error,
    );

    let diagnostics = compilation
        .check_diagnostics()
        .merged(&DiagnosticBag::single(unsupported));

    driver_result_from_compilation(compilation, diagnostics, output_format, ExitCode::FAILURE)
}

fn native_product_failure_result(
    compilation: bray_compilation::Compilation,
    output_format: OutputFormat,
    error: &bray_compilation::NativeProductFactError,
) -> DriverRunResult {
    let diagnostics = match error {
        bray_compilation::NativeProductFactError::StandardLibrary(error) => compilation
            .check_diagnostics()
            .merged(&compilation.standard_library_load_diagnostics(error)),
        _ => compilation.check_diagnostics().clone(),
    };

    driver_result_from_compilation(compilation, diagnostics, output_format, ExitCode::FAILURE)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::DiagnosticKind;
    use bray_emitter::{ArtifactKind, ArtifactRequirement};
    use bray_symbols::ProductKind;

    use super::{emission_request, required_artifacts};
    use crate::command::{DriverBackend, DriverInspectionArtifact, DriverProductConfiguration};
    use crate::run::run_result;
    use crate::test_support::TemporaryFile;

    #[test]
    fn emission_request_keeps_required_product_and_optional_inspections_typed() {
        let configuration = DriverProductConfiguration::new(
            DriverBackend::Llvm,
            bray_compilation::BuildConfiguration::Development,
            None,
            vec![],
            "out".into(),
            None,
            vec![],
            vec![DriverInspectionArtifact::BackendIr],
        );

        let target = bray_compilation::SelectedTarget::baseline();

        let package = bray_symbols::PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = bray_symbols::ProductIdentity::try_new(package, "library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let request = emission_request(
            product,
            ProductKind::Library,
            &target,
            &[
                bray_target::TargetOutputKind::PackageInterface,
                bray_target::TargetOutputKind::StaticLibrary,
            ],
            &configuration,
            None,
        );

        assert_eq!(
            request
                .artifact(ArtifactKind::PackageInterface)
                .map(|artifact| artifact.requirement()),
            Some(ArtifactRequirement::Required)
        );

        assert_eq!(
            request
                .artifact(ArtifactKind::BackendIr)
                .map(|artifact| artifact.requirement()),
            Some(ArtifactRequirement::Optional)
        );
    }

    #[test]
    fn development_executables_request_target_required_debug_companions() {
        let configuration = DriverProductConfiguration::new(
            DriverBackend::Llvm,
            bray_compilation::BuildConfiguration::Development,
            None,
            vec![],
            "out".into(),
            None,
            vec![],
            vec![],
        );

        let artifacts = required_artifacts(
            ProductKind::Executable,
            bray_target::NativeTarget::X86_64WindowsMsvc,
            &configuration,
        );

        assert_eq!(
            artifacts,
            [
                bray_target::TargetOutputKind::Executable,
                bray_target::TargetOutputKind::LinkedCompanion,
            ]
        );

        let artifacts = required_artifacts(
            ProductKind::Executable,
            bray_target::NativeTarget::Aarch64MacOs,
            &configuration,
        );

        assert_eq!(
            artifacts,
            [
                bray_target::TargetOutputKind::Executable,
                bray_target::TargetOutputKind::LinkedCompanion,
            ]
        );
    }

    #[test]
    fn library_build_emits_interface_and_static_library() {
        let source = TemporaryFile::write("library.bray", b"module app;");

        let output = source
            .path()
            .parent()
            .unwrap_or_else(|| panic!("temporary source must have a parent"))
            .join("out");

        std::fs::create_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be created: {error:?}"));

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("build"),
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(
            result.exit_code(),
            ExitCode::SUCCESS,
            "{:?}",
            result.diagnostics()
        );

        assert!(result.diagnostics().is_empty());
        assert!(output.join("library.brayi").is_file());
        assert!(output.join("liblibrary.a").is_file());

        std::fs::remove_file(output.join("library.brayi"))
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));

        std::fs::remove_file(output.join("liblibrary.a"))
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));

        std::fs::remove_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be removed: {error:?}"));
    }

    #[test]
    fn explicit_interface_only_build_does_not_require_native_linking() {
        let source = TemporaryFile::write("library.bray", b"module app;");

        let output = source
            .path()
            .parent()
            .unwrap_or_else(|| panic!("temporary source must have a parent"))
            .join("out");

        std::fs::create_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be created: {error:?}"));

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("build"),
            OsString::from("--artifact"),
            OsString::from("package-interface"),
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(
            result.exit_code(),
            ExitCode::SUCCESS,
            "{:#?}",
            result.diagnostics()
        );

        assert!(result.diagnostics().is_empty());
        assert!(output.join("library.brayi").is_file());
        assert!(!output.join("liblibrary.a").exists());

        std::fs::remove_file(output.join("library.brayi"))
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));

        std::fs::remove_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be removed: {error:?}"));
    }

    #[test]
    fn library_build_returns_merged_source_diagnostics() {
        let source = TemporaryFile::write("library.bray", b"$");

        let output = source
            .path()
            .parent()
            .unwrap_or_else(|| panic!("temporary source must have a parent"))
            .join("out");

        std::fs::create_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be created: {error:?}"));

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("build"),
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::LexicalInvalidCharacter)
                .count(),
            1
        );

        assert!(!output.join("library.brayi").exists());

        std::fs::remove_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be removed: {error:?}"));
    }

    #[test]
    fn executable_build_emits_one_native_product() {
        let source = TemporaryFile::write(
            "application.bray",
            concat!(
                "module app;\n",
                "\n",
                "func helper()\n",
                "{\n",
                "}\n",
                "\n",
                "func main()\n",
                "{\n",
                "    if true\n",
                "    {\n",
                "        helper();\n",
                "    }\n",
                "}\n",
            )
            .as_bytes(),
        );

        let output = source
            .path()
            .parent()
            .unwrap_or_else(|| panic!("temporary source must have a parent"))
            .join("out");

        std::fs::create_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be created: {error:?}"));

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("build"),
            OsString::from("--product-kind"),
            OsString::from("executable"),
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(
            result.exit_code(),
            ExitCode::SUCCESS,
            "{:?}",
            result.diagnostics()
        );

        assert!(result.diagnostics().is_empty());
        assert!(output.join("application").is_file());

        std::fs::remove_file(output.join("application"))
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));

        std::fs::remove_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be removed: {error:?}"));
    }

    #[test]
    fn unavailable_runtime_profile_reports_unsupported_product_emission() {
        let source = TemporaryFile::write("application.bray", b"module app;");

        let output = source
            .path()
            .parent()
            .unwrap_or_else(|| panic!("temporary source must have a parent"))
            .join("out");

        let result = run_result([
            OsString::from("brayc"),
            OsString::from("build"),
            OsString::from("--product-kind"),
            OsString::from("executable"),
            OsString::from("--runtime-profile"),
            OsString::from("unavailable"),
            OsString::from("--output"),
            output.into_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestUnsupportedProductEmission)
                .count(),
            1,
            "{:#?}",
            result.diagnostics()
        );
    }
}
