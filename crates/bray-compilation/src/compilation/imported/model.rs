use bray_diagnostics::DiagnosticResult;
use bray_package_interface::{PackageInterfaceSurface, ValidatedPackageInterface};

#[derive(Debug, Hash)]
pub(in crate::compilation) struct LoadedDependencyInterface {
    validated: Option<ValidatedPackageInterface>,
    result: DiagnosticResult<Option<PackageInterfaceSurface>>,
}

impl LoadedDependencyInterface {
    pub(super) fn new(
        validated: ValidatedPackageInterface,
        surface: PackageInterfaceSurface,
    ) -> Self {
        Self {
            validated: Some(validated),
            result: DiagnosticResult::new(Some(surface), bray_diagnostics::DiagnosticBag::new()),
        }
    }

    pub(super) const fn invalid(diagnostics: bray_diagnostics::DiagnosticBag) -> Self {
        Self {
            validated: None,
            result: DiagnosticResult::new(None, diagnostics),
        }
    }

    pub(super) const fn validated(&self) -> Option<&ValidatedPackageInterface> {
        self.validated.as_ref()
    }

    pub(super) const fn result(&self) -> &DiagnosticResult<Option<PackageInterfaceSurface>> {
        &self.result
    }

    pub(super) const fn surface(&self) -> Option<&PackageInterfaceSurface> {
        self.result.value().as_ref()
    }
}
