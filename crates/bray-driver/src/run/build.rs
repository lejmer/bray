use std::path::PathBuf;
use std::process::ExitCode;

use bray_compilation::ProductEmissionInputs;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionFailure, DiagnosticId,
    DiagnosticIoErrorKind, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticProjectCommandFailure, DiagnosticProjectOperation, DiagnosticTestCatalogFailure,
    SeverityKind,
};
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ProductBuildIdentity,
    ProductBuildIdentityPart, ReplacementPolicy, RequestedArtifact, RequestedArtifactDestination,
};
use bray_symbols::{ProductIdentity, ProductKind};
use bray_target::{TargetOutputDescription, TargetOutputKind};
use bray_tooling::{
    OutputFormat, exit_code_from_diagnostics, load_llvm_compilation, native_linker,
};

use super::execute::{DriverRunResult, compilation_request, driver_result_from_compilation};
use super::runtime::{resolve_runtime, runtime_selection_diagnostics};
use crate::command::{
    DriverBackend, DriverOptions, DriverProductConfiguration, DriverRuntimeSelection,
};

/// Runs one typed build request through the compiler driver's normal planning and emission path.
///
/// This entry point is for toolchain-owned orchestrators that already hold validated driver
/// configuration and source paths. Ordinary command-line builds continue through [`crate::run`].
pub fn run_build_request(
    options: &DriverOptions,
    configuration: DriverProductConfiguration,
    files: Vec<PathBuf>,
) -> DriverRunResult {
    let output_format = options.output_format();
    let compilation_configuration = options.compilation();
    let native_target = compilation_configuration.target();
    let selected_target = bray_compilation::SelectedTarget::for_native(native_target);
    let product = compilation_configuration.product().clone();
    let product_kind = compilation_configuration.product_kind();
    let artifacts = required_artifacts(product_kind, native_target, &configuration);
    let export_interface = artifacts.contains(&TargetOutputKind::PackageInterface);
    let linked = artifacts.iter().any(|artifact| is_linked(*artifact));
    let build = native_build_configuration(configuration.build(), linked);

    let requires_codegen = artifacts
        .iter()
        .copied()
        .map(ArtifactKind::from)
        .any(|artifact| artifact.backend_kind().is_some());

    let requires_generation = linked || requires_codegen;

    let request = match compilation_request(options, files, export_interface) {
        Ok(request) => request,
        Err(diagnostics) => {
            let exit_code = exit_code_from_diagnostics(&diagnostics);

            return DriverRunResult::new(exit_code, diagnostics, output_format);
        }
    };

    let runtime = if requires_generation {
        match resolve_runtime(configuration.runtime(), &selected_target) {
            Ok(runtime) => runtime,
            Err(diagnostics) => {
                return DriverRunResult::new(ExitCode::FAILURE, diagnostics, output_format);
            }
        }
    } else {
        None
    };

    let compilation = match configuration.backend() {
        DriverBackend::Llvm => load_llvm_compilation(request),
    };

    let compilation = match compilation {
        Ok(compilation) => compilation,
        Err(error) => {
            return DriverRunResult::new(
                ExitCode::FAILURE,
                DiagnosticBag::single(
                    error.diagnostic(selected_target.profile().identity().as_str()),
                ),
                output_format,
            );
        }
    };

    let linker = if linked {
        // The driver retains its selections while the linker owns its independent output path.
        match native_linker(
            native_target,
            configuration.linker_map_output().cloned(),
            configuration.output_root(),
        ) {
            Ok(linker) => Some(linker),
            Err(error) => {
                let diagnostics = compilation
                    .check_diagnostics()
                    .merged(&DiagnosticBag::single(
                        error.diagnostic(selected_target.profile().identity().as_str()),
                    ));

                return driver_result_from_compilation(
                    compilation,
                    diagnostics,
                    output_format,
                    ExitCode::FAILURE,
                );
            }
        }
    } else {
        None
    };

    let native = if requires_generation {
        match compilation.native_product_plan(
            product.clone(),
            build,
            runtime,
            configuration.required_capabilities().iter().copied(),
            linker.as_ref().map(bray_tooling::NativeLinker::linker),
        ) {
            Ok(native) => Some(native),
            Err(error) => {
                return native_product_failure_result(
                    compilation,
                    output_format,
                    &product,
                    selected_target.profile().identity().as_str(),
                    &error,
                );
            }
        }
    } else {
        None
    };

    let test_catalog = if configuration.publishes_test_catalog() {
        match encode_test_catalog(&compilation, &product) {
            Ok(catalog) => Some(catalog),
            Err(error) => {
                let diagnostics = match error {
                    TestCatalogPublicationError::Cancelled => DiagnosticBag::new(),
                    TestCatalogPublicationError::Diagnostic(diagnostic) => {
                        DiagnosticBag::single(diagnostic)
                    }
                };

                return driver_result_from_compilation(
                    compilation,
                    diagnostics,
                    output_format,
                    ExitCode::FAILURE,
                );
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

    if let Some(test_catalog) = test_catalog.as_deref() {
        inputs = inputs.with_test_catalog(test_catalog);
    }

    let validate_publication = || match configuration.build_identity() {
        Some(identity) => verify_reusable_build_environment(options, &configuration, identity),
        None => Ok(()),
    };

    if configuration.build_identity().is_some() {
        inputs = inputs.with_publication_validation(&validate_publication);
    }

    match (native.as_ref(), linker.as_ref()) {
        (Some(native), Some(linker)) => {
            inputs = inputs.with_native_product(native, linker.linker());
        }
        (Some(native), None) => {
            inputs = inputs.with_native_codegen(native);
        }
        (None, _) => {}
    }

    match compilation.emit_product(request, inputs) {
        Ok(outcome) => {
            let diagnostics = outcome.diagnostics().clone();

            let published_artifacts = outcome
                .generation()
                .map(|generation| {
                    outcome
                        .artifacts()
                        .artifacts()
                        .iter()
                        .filter_map(|artifact| generation.published_artifact_path(artifact.id()))
                        .collect()
                })
                .unwrap_or_default();

            let exit_code = match outcome.status() {
                EmissionStatus::Complete => exit_code_from_diagnostics(&diagnostics),
                EmissionStatus::Failed(_) | EmissionStatus::Cancelled => ExitCode::FAILURE,
            };

            driver_result_from_compilation(compilation, diagnostics, output_format, exit_code)
                .with_published_artifacts(published_artifacts)
        }
        Err(error) => driver_result_from_compilation(
            compilation,
            error.diagnostics().clone(),
            output_format,
            ExitCode::FAILURE,
        ),
    }
}

fn encode_test_catalog(
    compilation: &bray_compilation::Compilation,
    product: &ProductIdentity,
) -> Result<Vec<u8>, TestCatalogPublicationError> {
    let discovery = compilation
        .test_discovery(product.clone())
        .map_err(|error| match error {
            bray_compilation::FactQueryError::Cancelled => TestCatalogPublicationError::Cancelled,
            error => {
                TestCatalogPublicationError::Diagnostic(test_discovery_failure(error, product))
            }
        })?;

    let (bytes, _) =
        bray_test_protocol::encode_test_catalog(discovery.value().catalog()).map_err(|error| {
            TestCatalogPublicationError::Diagnostic(test_catalog_failure(error, product))
        })?;

    Ok(bytes)
}

enum TestCatalogPublicationError {
    Cancelled,
    Diagnostic(Diagnostic),
}

fn test_discovery_failure(
    error: bray_compilation::FactQueryError,
    product: &ProductIdentity,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingCompilerDefect,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::emission_failure(
        DiagnosticEmissionFailure::Evaluation(error.diagnostic_evaluation_failure()),
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ReportCompilerDefect,
    ))
}

fn test_catalog_failure(
    error: bray_test_protocol::TestProtocolError,
    product: &ProductIdentity,
) -> Diagnostic {
    let failure = match error {
        bray_test_protocol::TestProtocolError::Io => DiagnosticTestCatalogFailure::Io,
        bray_test_protocol::TestProtocolError::Malformed => DiagnosticTestCatalogFailure::Malformed,
        bray_test_protocol::TestProtocolError::UnsupportedVersion(version) => {
            DiagnosticTestCatalogFailure::UnsupportedVersion(version)
        }
        bray_test_protocol::TestProtocolError::ResourceLimit => {
            DiagnosticTestCatalogFailure::ResourceLimit
        }
    };

    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingCompilerDefect,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::emission_failure(
        DiagnosticEmissionFailure::TestCatalog(failure),
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ReportCompilerDefect,
    ))
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
                TargetOutputKind::PackageImplementation,
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

    if configuration.publishes_test_catalog() && !artifacts.contains(&TargetOutputKind::TestCatalog)
    {
        artifacts.push(TargetOutputKind::TestCatalog);
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

const fn native_build_configuration(
    selected: bray_compilation::BuildConfiguration,
    linked: bool,
) -> bray_compilation::BuildConfiguration {
    match (selected, linked) {
        (bray_compilation::BuildConfiguration::Release, false) => {
            bray_compilation::BuildConfiguration::ObjectRelease
        }
        _ => selected,
    }
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

    let destination = configuration.managed_output_directory().map_or_else(
        || bray_emitter::ManagedFilesystemDestination::at_root(configuration.output_root()),
        |directory| {
            bray_emitter::ManagedFilesystemDestination::new(
                configuration.output_root(),
                directory.clone(),
            )
        },
    );

    let request = EmissionRequest::try_new(
        product,
        product_kind,
        executable_host,
        selected.profile().identity().clone(),
        RequestedArtifactDestination::FilesystemDirectory(destination),
        required.chain(inspections),
        ReplacementPolicy::ReplaceExisting,
    )
    .unwrap_or_else(|error| panic!("validated build emission request must be valid: {error:?}"))
    .with_storage_profile(configuration.build().as_str());

    match configuration.build_identity().cloned() {
        Some(identity) => request.with_build_identity(identity),
        None => request,
    }
}

fn verify_reusable_build_environment(
    options: &DriverOptions,
    configuration: &DriverProductConfiguration,
    expected: &ProductBuildIdentity,
) -> Result<(), DiagnosticBag> {
    let compiler_path = std::env::current_exe()
        .map_err(|error| reusable_identity_io_diagnostics(PathBuf::from("brayc"), error))?;

    let compiler =
        bray_emitter::path_digest(&compiler_path).map_err(reusable_identity_digest_diagnostics)?;

    if compiler != expected.compiler() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::Compiler,
        ));
    }

    let Some(standard_library_root) = options
        .standard_library_root()
        .or_else(|| options.standard_library_provider_root())
        .map(bray_standard_library::StandardLibraryRoot::path)
    else {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::StandardLibrary,
        ));
    };

    let standard_library = bray_emitter::build_input_path_digest(standard_library_root)
        .map_err(reusable_identity_digest_diagnostics)?;

    let Some(toolchain_root) = standard_library_root.parent() else {
        return Err(DiagnosticBag::single(
            DiagnosticProjectCommandFailure::MissingParent {
                operation: DiagnosticProjectOperation::ReusableBuildIdentity,
                path: standard_library_root.to_path_buf(),
            }
            .diagnostic(DiagnosticId::new(0)),
        ));
    };

    let toolchain = bray_emitter::toolchain_path_digest(toolchain_root)
        .map_err(reusable_identity_digest_diagnostics)?;

    if toolchain != expected.toolchain() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::Toolchain,
        ));
    }

    if standard_library != expected.standard_library() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::StandardLibrary,
        ));
    }

    let Some(DriverRuntimeSelection::Artifact(runtime_metadata)) = configuration.runtime() else {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::Runtime,
        ));
    };

    let runtime_root = runtime_metadata.parent().unwrap_or(runtime_metadata);

    let runtime = bray_emitter::build_input_path_digest(runtime_root)
        .map_err(reusable_identity_digest_diagnostics)?;

    if runtime != expected.runtime() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::Runtime,
        ));
    }

    let protocol = bray_test_protocol::protocol_version();

    if protocol != expected.catalog_protocol() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::CatalogProtocol,
        ));
    }

    if protocol != expected.runner_protocol() {
        return Err(reusable_identity_mismatch_diagnostics(
            options,
            ProductBuildIdentityPart::RunnerProtocol,
        ));
    }

    Ok(())
}

fn reusable_identity_io_diagnostics(path: PathBuf, error: std::io::Error) -> DiagnosticBag {
    DiagnosticBag::single(
        DiagnosticProjectCommandFailure::Io {
            operation: DiagnosticProjectOperation::ReusableBuildIdentity,
            path,
            error: DiagnosticIoErrorKind::from(error.kind()),
        }
        .diagnostic(DiagnosticId::new(0)),
    )
}

fn reusable_identity_digest_diagnostics(
    error: bray_emitter::BuildInputDigestError,
) -> DiagnosticBag {
    let (path, cause) = error.into_parts();

    reusable_identity_io_diagnostics(path, cause)
}

fn reusable_identity_mismatch_diagnostics(
    options: &DriverOptions,
    part: ProductBuildIdentityPart,
) -> DiagnosticBag {
    let product = options.compilation().product();
    let identity = format!("{}/{}", product.package().as_str(), product.name());

    bray_tooling::reusable_build_identity_mismatch_diagnostics(identity, part)
}

fn native_product_failure_result(
    compilation: bray_compilation::Compilation,
    output_format: OutputFormat,
    product: &ProductIdentity,
    target: &str,
    error: &bray_compilation::NativeProductPlanningError,
) -> DriverRunResult {
    let diagnostics = match error {
        bray_compilation::NativeProductPlanningError::StandardLibrary { cause, .. } => compilation
            .check_diagnostics()
            .merged(&compilation.standard_library_load_diagnostics(cause)),
        bray_compilation::NativeProductPlanningError::InvalidRuntimeSelection(selection_error) => {
            let runtime = runtime_selection_diagnostics(selection_error).unwrap_or_else(|| {
                error.diagnostic(product, target).unwrap_or_else(|| {
                    panic!("runtime selection failure must publish an exact diagnostic")
                })
            });

            compilation.check_diagnostics().merged(&runtime)
        }
        error if let Some(diagnostics) = error.diagnostics() => {
            compilation.check_diagnostics().merged(diagnostics)
        }
        error if error.is_cancelled() => compilation.check_diagnostics().clone(),
        error => compilation.check_diagnostics().merged(
            &error.diagnostic(product, target).unwrap_or_else(|| {
                panic!("non-cancelled native product failure must publish an exact diagnostic")
            }),
        ),
    };

    driver_result_from_compilation(compilation, diagnostics, output_format, ExitCode::FAILURE)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticEmissionFailure, DiagnosticKind, DiagnosticTestCatalogFailure,
    };
    use bray_emitter::{ArtifactKind, ArtifactRequirement};
    use bray_symbols::ProductKind;

    use super::{emission_request, required_artifacts, test_catalog_failure};
    use crate::command::{DriverBackend, DriverInspectionArtifact, DriverProductConfiguration};
    use crate::run::run_result;
    use crate::test_support::TemporaryFile;

    #[test]
    fn test_catalog_protocol_failures_keep_their_exact_category() {
        let package = bray_symbols::PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = bray_symbols::ProductIdentity::try_new(package, "test")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostic = test_catalog_failure(
            bray_test_protocol::TestProtocolError::ResourceLimit,
            &product,
        );

        let failure = diagnostic
            .args()
            .iter()
            .find_map(|argument| match argument.value() {
                DiagnosticArgValue::EmissionFailure(failure) => Some(failure),
                _ => None,
            })
            .unwrap_or_else(|| panic!("test-catalog failure must retain emission context"));

        assert_eq!(
            failure,
            &DiagnosticEmissionFailure::TestCatalog(DiagnosticTestCatalogFailure::ResourceLimit)
        );
    }

    #[test]
    fn emission_request_keeps_required_product_and_optional_inspections_typed() {
        let configuration = DriverProductConfiguration::new(
            DriverBackend::Llvm,
            bray_compilation::BuildConfiguration::Development,
            None,
            vec![],
            "out".into(),
            false,
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
            false,
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
    fn test_catalogs_are_required_members_of_test_product_publications() {
        let configuration = DriverProductConfiguration::new(
            DriverBackend::Llvm,
            bray_compilation::BuildConfiguration::Development,
            None,
            vec![],
            "out".into(),
            true,
            vec![],
            vec![],
        );

        let artifacts = required_artifacts(
            ProductKind::Test,
            bray_target::NativeTarget::X86_64LinuxGnu,
            &configuration,
        );

        assert!(artifacts.contains(&bray_target::TargetOutputKind::Executable));
        assert!(artifacts.contains(&bray_target::TargetOutputKind::TestCatalog));
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

        let product = command_product("library");

        for kind in [
            ArtifactKind::PackageInterface,
            ArtifactKind::PackageImplementation,
            ArtifactKind::StaticLibrary,
        ] {
            let path = bray_emitter::resolve_published_artifact(&output, &product, kind, 0)
                .unwrap_or_else(|error| panic!("published artifact must resolve: {error:?}"));

            assert!(path.path().is_file(), "{}", path.path().display());
            assert_eq!(path.path().parent(), Some(output.as_path()));
        }

        std::fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));
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

        let product = command_product("library");

        let interface = bray_emitter::resolve_published_artifact(
            &output,
            &product,
            ArtifactKind::PackageInterface,
            0,
        )
        .unwrap_or_else(|error| panic!("published interface must resolve: {error:?}"));

        assert!(interface.path().is_file());

        assert_eq!(
            bray_emitter::resolve_published_artifact(
                &output,
                &product,
                ArtifactKind::StaticLibrary,
                0,
            )
            .unwrap_err(),
            bray_emitter::PublishedGenerationReadError::ArtifactUnavailable
        );

        std::fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));
    }

    #[test]
    fn release_relocatable_object_build_uses_standalone_native_optimization() {
        let source = TemporaryFile::write(
            "library.bray",
            concat!(
                "module app;\n",
                "\n",
                "func answer() -> i32\n",
                "{\n",
                "    return 42;\n",
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
            OsString::from("--release"),
            OsString::from("--artifact"),
            OsString::from("relocatable-object"),
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

        let object = bray_emitter::resolve_published_artifact(
            &output,
            &command_product("library"),
            ArtifactKind::RelocatableObject,
            0,
        )
        .unwrap_or_else(|error| panic!("published object must resolve: {error:?}"));

        assert!(object.path().is_file());

        std::fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));
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
    fn synchronous_executable_build_requires_a_runtime_artifact() {
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
            OsString::from("--inspect"),
            OsString::from("relocatable-object"),
            source.path().as_os_str().to_os_string(),
        ]);

        assert_eq!(result.exit_code(), ExitCode::FAILURE);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::NativeProductPreparationFailed)
                .count(),
            1,
            "{:#?}",
            result.diagnostics()
        );

        std::fs::remove_dir_all(&output)
            .unwrap_or_else(|error| panic!("build output must be removed: {error:?}"));
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

    fn command_product(name: &str) -> bray_symbols::ProductIdentity {
        let package = bray_symbols::PackageIdentity::try_new("command.line")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        bray_symbols::ProductIdentity::try_new(package, name)
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }
}
