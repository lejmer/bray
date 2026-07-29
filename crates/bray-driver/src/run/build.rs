use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::{
    CompilationOptions, CompilationRequest, PackageInterfaceExportRequest,
    ProductEmissionInputs,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    PackageInterfaceIdentity,
};
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{
    TargetOutputDescription, TargetOutputKind, TargetOutputName,
};

use super::execute::{
    DriverRunResult, command_line_package_identity, driver_result_from_compilation,
    load_compilation,
};
use crate::command::{
    DriverOutputFormat, DriverProductConfiguration, compilation_request_from_file_arguments,
};
use crate::run::exit_code_from_diagnostics;

pub(crate) fn run_build_command(
    worker_budget: bray_compilation::WorkerBudget,
    configuration: DriverProductConfiguration,
    files: Vec<PathBuf>,
    output_format: DriverOutputFormat,
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

    let request = configure_package_interface_export(request, &configuration);

    let compilation = match load_compilation(request, None) {
        Some(compilation) => compilation,
        None => return DriverRunResult::new(
            ExitCode::FAILURE,
            DiagnosticBag::new(),
            output_format,
        ),
    };

    if configuration.product_kind() != ProductKind::Library
        || configuration.runtime().is_some()
        || !configuration.required_capabilities().is_empty()
    {
        return unsupported_product_result(compilation, output_format);
    }

    let product = product_identity(package, &configuration);
    let target_outputs = target_outputs(&selected_target, &configuration);
    let request = emission_request(product, &selected_target, &configuration);
    let inputs = ProductEmissionInputs::new(&target_outputs);

    match compilation.emit_product(request, inputs) {
        Ok(outcome) => {
            let diagnostics = outcome.diagnostics().clone();

            let exit_code = match outcome.status() {
                EmissionStatus::Complete => exit_code_from_diagnostics(&diagnostics),
                EmissionStatus::Failed(_) | EmissionStatus::Cancelled => ExitCode::FAILURE,
            };

            driver_result_from_compilation(
                compilation,
                diagnostics,
                output_format,
                exit_code,
            )
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
) -> CompilationRequest {
    if configuration.product_kind() != ProductKind::Library {
        return request;
    }

    let package = request.package_identity().clone();

    let product = InterfaceProductIdentity::try_new(configuration.product_name())
        .unwrap_or_else(|| panic!("the command-line product identity must be valid"));

    let identity = PackageInterfaceIdentity::try_new(
        package,
        product,
        InterfaceProductKind::Library,
        "command-line-public-v1",
    )
    .unwrap_or_else(|| panic!("the command-line public-surface identity must be valid"));

    request.with_package_interface_export(PackageInterfaceExportRequest::new(
        identity,
        InterfaceLanguageRevision::new(0),
    ))
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
    let interface = target_output_name(TargetOutputKind::PackageInterface, "", ".brayi");

    let inspections = configuration
        .inspections()
        .iter()
        .copied()
        .map(|inspection| match inspection.artifact_kind() {
            ArtifactKind::Assembly => target_output_name(TargetOutputKind::Assembly, "", ".s"),
            ArtifactKind::BackendIr => target_output_name(TargetOutputKind::BackendIr, "", ".ll"),
            ArtifactKind::BackendBitcode => {
                target_output_name(TargetOutputKind::BackendBitcode, "", ".bc")
            }
            ArtifactKind::RelocatableObject => {
                target_output_name(TargetOutputKind::RelocatableObject, "", ".o")
            }
            _ => unreachable!("driver inspection selections cover only backend inspection kinds"),
        });

    TargetOutputDescription::try_new(
        selected.profile().clone(),
        [interface].into_iter().chain(inspections),
    )
    .unwrap_or_else(|error| panic!("baseline target output names must be valid: {error:?}"))
}

fn target_output_name(
    kind: TargetOutputKind,
    prefix: &str,
    suffix: &str,
) -> TargetOutputName {
    TargetOutputName::try_new(kind, prefix, suffix)
        .unwrap_or_else(|error| panic!("baseline target output name must be valid: {error:?}"))
}

fn emission_request(
    product: ProductIdentity,
    selected: &bray_compilation::SelectedTarget,
    configuration: &DriverProductConfiguration,
) -> EmissionRequest {
    let required = RequestedArtifact::new(
        ArtifactKind::PackageInterface,
        ArtifactRequirement::Required,
    );

    let inspections = configuration.inspections().iter().copied().map(|artifact| {
        RequestedArtifact::new(artifact.artifact_kind(), ArtifactRequirement::Optional)
    });

    EmissionRequest::try_new(
        product,
        configuration.product_kind(),
        None,
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(configuration.output().to_path_buf()),
        [required].into_iter().chain(inspections),
        ReplacementPolicy::ReplaceExisting,
    )
    .unwrap_or_else(|error| panic!("validated build emission request must be valid: {error:?}"))
}

fn unsupported_product_result(
    compilation: bray_compilation::Compilation,
    output_format: DriverOutputFormat,
) -> DriverRunResult {
    // TODO(BRA-326): Supply complete native product facts before enabling native requests.
    let unsupported = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RequestUnsupportedProductEmission,
        SeverityKind::Error,
    );

    let diagnostics = compilation
        .check_diagnostics()
        .merged(&DiagnosticBag::single(unsupported));

    driver_result_from_compilation(
        compilation,
        diagnostics,
        output_format,
        ExitCode::FAILURE,
    )
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
        DriverBackend, DriverInspectionArtifact, DriverProductConfiguration,
        DriverTarget,
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

        let product = product_identity(
            super::command_line_package_identity(),
            &configuration,
        );

        let request = emission_request(product, &target, &configuration);

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
    fn library_build_emits_one_package_interface() {
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
    fn native_product_gap_is_explicit_and_fails() {
        let source = TemporaryFile::write(
            "application.bray",
            b"module app;\n\nfunc main()\n{\n}\n",
        );

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
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::RequestUnsupportedProductEmission)
                .count(),
            1
        );
    }
}
