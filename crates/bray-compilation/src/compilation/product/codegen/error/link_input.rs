use super::model::NativeLinkInputPlanningError;

pub(super) fn diagnostic_native_link_input_failure(
    error: &NativeLinkInputPlanningError,
) -> bray_diagnostics::DiagnosticNativeLinkInputFailure {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as DiagnosticFailure;

    match error {
        NativeLinkInputPlanningError::InvalidRequirement {
            name,
            kind,
            provenance,
        } => DiagnosticFailure::InvalidRequirement {
            // The diagnostic outlives this borrowed planning error and owns the native name.
            name: name.clone(),
            link_kind: kind.as_str().to_owned(),
            provenance_kind: link_input_provenance_kind(provenance),
            provenance_identity: link_input_provenance_identity(provenance),
        },
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
