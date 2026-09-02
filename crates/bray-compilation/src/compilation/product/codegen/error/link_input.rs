use super::model::NativeLinkInputPlanningError;

pub(super) fn diagnostic_native_link_input_failure(
    error: &NativeLinkInputPlanningError,
) -> bray_diagnostics::DiagnosticNativeLinkInputFailure {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as DiagnosticFailure;

    match error {
        NativeLinkInputPlanningError::UnsupportedStandardLibraryArtifact { path, kind } => {
            DiagnosticFailure::UnsupportedStandardLibraryArtifact {
                path: path.clone(),
                artifact_kind: standard_library_artifact_kind(*kind),
            }
        }
        NativeLinkInputPlanningError::InvalidStandardLibraryArtifact { path, kind, cause } => {
            DiagnosticFailure::InvalidStandardLibraryArtifact {
                path: path.clone(),
                input_kind: link_input_kind(*kind),
                cause: link_input_failure(*cause),
            }
        }
        NativeLinkInputPlanningError::InvalidRequirement {
            name,
            kind,
            provenance,
        } => DiagnosticFailure::InvalidRequirement {
            name: name.clone(),
            link_kind: kind.as_str().to_owned(),
            provenance_kind: link_input_provenance_kind(provenance),
            provenance_identity: link_input_provenance_identity(provenance),
        },
    }
}

const fn standard_library_artifact_kind(
    kind: bray_standard_library::StandardLibraryArtifactKind,
) -> &'static str {
    use bray_standard_library::StandardLibraryArtifactKind as Kind;

    match kind {
        Kind::PackageInterface => "package_interface",
        Kind::PackageImplementation => "package_implementation",
        Kind::DependencyMetadata => "dependency_metadata",
        Kind::RelocatableObject => "relocatable_object",
        Kind::StaticLibrary => "static_library",
        Kind::PlatformServiceLibrary => "platform_service_library",
        Kind::OptimizationArchive => "optimization_archive",
        Kind::SharedLibrary => "shared_library",
        Kind::RuntimeArtifact => "runtime_artifact",
    }
}

const fn link_input_kind(kind: bray_linker::LinkInputKind) -> &'static str {
    use bray_linker::LinkInputKind as Kind;

    match kind {
        Kind::RelocatableObject => "relocatable_object",
        Kind::Bitcode => "bitcode",
        Kind::Archive => "archive",
        Kind::StartupObject => "startup_object",
        Kind::TerminationObject => "termination_object",
        Kind::RuntimeComponent => "runtime_component",
        Kind::NativeLibrary => "native_library",
        Kind::Framework => "framework",
    }
}

const fn link_input_failure(cause: bray_linker::LinkInputBuildError) -> &'static str {
    use bray_linker::LinkInputBuildError as Error;

    match cause {
        Error::EmptyFilePath => "empty_file_path",
        Error::SourceKindMismatch => "source_kind_mismatch",
        Error::WholeArchiveRequiresArchive => "whole_archive_requires_archive",
    }
}

const fn link_input_provenance_kind(provenance: &bray_linker::LinkInputProvenance) -> &'static str {
    use bray_linker::LinkInputProvenance as Provenance;

    match provenance {
        Provenance::Product => "product",
        Provenance::Package(_) => "package",
        Provenance::PlatformProvider(_) => "platform_provider",
        Provenance::TargetProfile => "target_profile",
        Provenance::Runtime(_) => "runtime",
        Provenance::RuntimeDependency(_) => "runtime_dependency",
        Provenance::HostConfiguration => "host_configuration",
    }
}

fn link_input_provenance_identity(provenance: &bray_linker::LinkInputProvenance) -> Option<String> {
    use bray_linker::LinkInputProvenance as Provenance;

    match provenance {
        Provenance::Package(package) | Provenance::PlatformProvider(package) => {
            Some(package.as_str().to_owned())
        }
        Provenance::Runtime(runtime) | Provenance::RuntimeDependency(runtime) => {
            Some(runtime.as_str().to_owned())
        }
        Provenance::Product | Provenance::TargetProfile | Provenance::HostConfiguration => None,
    }
}
