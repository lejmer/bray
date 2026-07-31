use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::{
    CompilationOptions, CompilationRequest, ProductEmissionInputs,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_runtime_interface::{
    RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactMetadata,
};
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{TargetOutputDescription, TargetOutputKind};
use bray_tooling::{
    OutputFormat, baseline_output_name, baseline_target_outputs,
    compilation_request_from_file_arguments, load_llvm_compilation,
    exit_code_from_diagnostics, native_linker,
    package_interface_export_request,
};
use sha2::{Digest, Sha256};

use super::execute::{
    DriverRunResult, command_line_package_identity, driver_result_from_compilation,
};
use crate::command::{
    DriverBackend, DriverProductConfiguration, DriverRuntimeSelection,
};

pub(crate) fn run_build_command(
    worker_budget: bray_compilation::WorkerBudget,
    configuration: DriverProductConfiguration,
    files: Vec<PathBuf>,
    output_format: OutputFormat,
) -> DriverRunResult {
    let package = command_line_package_identity();
    let selected_target = configuration.target().selected_target();

    let options = CompilationOptions::new(
        worker_budget,
        configuration.product_kind(),
        selected_target.clone(),
    );

    let request = match compilation_request_from_file_arguments(package.clone(), files, options) {
        Ok(request) => request,
        Err(diagnostics) => {
            let exit_code = exit_code_from_diagnostics(&diagnostics);

            return DriverRunResult::new(exit_code, diagnostics, output_format);
        }
    };

    let product = product_identity(package, &configuration);

    let request = configure_package_interface_export(
        request,
        &configuration,
        product.clone(),
    );

    let compilation = match configuration.backend() {
        DriverBackend::Llvm => load_llvm_compilation(request),
    };

    let compilation = match compilation {
        Some(compilation) => compilation,
        None => return DriverRunResult::new(ExitCode::FAILURE, DiagnosticBag::new(), output_format),
    };

    let Some(linker) = native_linker() else {
        return unsupported_product_result(compilation, output_format);
    };

    let runtime = match resolve_runtime(configuration.runtime(), &selected_target) {
        Ok(runtime) => runtime,
        Err(()) => return unsupported_product_result(compilation, output_format),
    };

    let native = match compilation.native_product_facts(
        product.clone(),
        runtime,
        configuration.required_capabilities().iter().copied(),
        &linker,
    ) {
        Ok(native) => native,
        Err(error) if error.is_unsupported() => {
            return unsupported_product_result(compilation, output_format);
        }
        Err(_) => return native_product_failure_result(compilation, output_format),
    };

    let target_outputs = target_outputs(&selected_target, &configuration);

    let request = emission_request(
        product,
        &selected_target,
        &configuration,
        native.executable_host().cloned(),
    );

    let inputs = ProductEmissionInputs::new(&target_outputs).with_native_product(&native, &linker);

    match compilation.emit_product(request, inputs) {
        Ok(outcome) => {
            let diagnostics = outcome.diagnostics().clone();

            let exit_code = match outcome.status() {
                EmissionStatus::Complete => exit_code_from_diagnostics(&diagnostics),
                EmissionStatus::Failed(_) | EmissionStatus::Cancelled => ExitCode::FAILURE,
            };

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

fn configure_package_interface_export(
    request: CompilationRequest,
    configuration: &DriverProductConfiguration,
    product: ProductIdentity,
) -> CompilationRequest {
    if configuration.product_kind() != ProductKind::Library {
        return request;
    }

    request.with_package_interface_export(
        package_interface_export_request(product),
    )
}

fn product_identity(
    package: bray_symbols::PackageIdentity,
    configuration: &DriverProductConfiguration,
) -> ProductIdentity {
    ProductIdentity::try_new(package, configuration.product_name())
        .unwrap_or_else(|| panic!("the command-line product identity must be valid"))
}

fn target_outputs(
    selected: &bray_compilation::SelectedTarget,
    configuration: &DriverProductConfiguration,
) -> TargetOutputDescription {
    let product = match configuration.product_kind() {
        ProductKind::Library => baseline_output_name(TargetOutputKind::StaticLibrary),
        ProductKind::Executable | ProductKind::Test => {
            baseline_output_name(TargetOutputKind::Executable)
        }
    };

    let interface = (configuration.product_kind() == ProductKind::Library)
        .then(|| baseline_output_name(TargetOutputKind::PackageInterface));

    let inspections = configuration
        .inspections()
        .iter()
        .copied()
        .map(|inspection| match inspection.artifact_kind() {
            ArtifactKind::Assembly => baseline_output_name(TargetOutputKind::Assembly),
            ArtifactKind::BackendIr => baseline_output_name(TargetOutputKind::BackendIr),
            ArtifactKind::BackendBitcode => baseline_output_name(TargetOutputKind::BackendBitcode),
            ArtifactKind::RelocatableObject => {
                baseline_output_name(TargetOutputKind::RelocatableObject)
            }
            _ => unreachable!("driver inspection selections cover only backend inspection kinds"),
        });

    baseline_target_outputs(
        selected,
        [Some(product), interface]
            .into_iter()
            .flatten()
            .chain(inspections)
            .map(|output| output.kind()),
    )
}

fn emission_request(
    product: ProductIdentity,
    selected: &bray_compilation::SelectedTarget,
    configuration: &DriverProductConfiguration,
    executable_host: Option<bray_runtime_interface::ExecutableHostContract>,
) -> EmissionRequest {
    let product_artifact = match configuration.product_kind() {
        ProductKind::Library => ArtifactKind::StaticLibrary,
        ProductKind::Executable | ProductKind::Test => ArtifactKind::Executable,
    };

    let required_product = RequestedArtifact::new(product_artifact, ArtifactRequirement::Required);

    let interface = (configuration.product_kind() == ProductKind::Library).then(|| {
        RequestedArtifact::new(
            ArtifactKind::PackageInterface,
            ArtifactRequirement::Required,
        )
    });

    let inspections = configuration.inspections().iter().copied().map(|artifact| {
        RequestedArtifact::new(artifact.artifact_kind(), ArtifactRequirement::Optional)
    });

    EmissionRequest::try_new(
        product,
        configuration.product_kind(),
        executable_host,
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(configuration.output().to_path_buf()),
        [Some(required_product), interface]
            .into_iter()
            .flatten()
            .chain(inspections),
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

    load_runtime_artifact(&metadata).map(Some)
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

fn load_runtime_artifact(metadata_path: &std::path::Path) -> Result<RuntimeArtifact, ()> {
    let metadata_bytes = std::fs::read(metadata_path).map_err(|_| ())?;
    let metadata = RuntimeArtifactMetadata::decode_json(&metadata_bytes).map_err(|_| ())?;
    let parent = metadata_path.parent().ok_or(())?;
    let archive = parent.join(metadata.archive_file_name());
    let archive_bytes = std::fs::read(&archive).map_err(|_| ())?;
    let digest = RuntimeArtifactDigest::new(Sha256::digest(&archive_bytes).into());

    RuntimeArtifact::try_new(metadata, archive, digest).map_err(|_| ())
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::DiagnosticKind;
    use bray_emitter::{ArtifactKind, ArtifactRequirement};
    use bray_symbols::ProductKind;

    use super::{emission_request, product_identity};
    use crate::command::{
        DriverBackend, DriverInspectionArtifact, DriverProductConfiguration, DriverTarget,
    };
    use crate::run::run_result;
    use crate::test_support::TemporaryFile;

    #[test]
    fn emission_request_keeps_required_product_and_optional_inspections_typed() {
        let configuration = DriverProductConfiguration::new(
            ProductKind::Library,
            DriverTarget::X86_64UnknownLinuxGnu,
            DriverBackend::Llvm,
            None,
            vec![],
            "out".into(),
            vec![DriverInspectionArtifact::BackendIr],
        );

        let target = configuration.target().selected_target();

        let product = product_identity(super::command_line_package_identity(), &configuration);

        let request = emission_request(product, &target, &configuration, None);

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

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

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
                "    };\n",
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

        assert_eq!(result.exit_code(), ExitCode::SUCCESS);

        assert!(result.diagnostics().is_empty());
        assert!(output.join("application").is_file());

        std::fs::remove_file(output.join("application"))
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));

        std::fs::remove_dir(&output)
            .unwrap_or_else(|error| panic!("build output directory must be removed: {error:?}"));
    }
}

fn native_product_failure_result(
    compilation: bray_compilation::Compilation,
    output_format: OutputFormat,
) -> DriverRunResult {
    let diagnostics = compilation.check_diagnostics().clone();

    driver_result_from_compilation(compilation, diagnostics, output_format, ExitCode::FAILURE)
}
