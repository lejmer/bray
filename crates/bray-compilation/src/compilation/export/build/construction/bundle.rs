use std::sync::Arc;

use bray_package_interface::{
    InterfaceDependency, InterfaceProductKind, PackageInterfaceExportBundle,
    build_package_interface_surface,
};
use bray_symbols::ProductKind;

use super::super::super::PackageInterfaceExportError;
use super::surface::build_identity_surface;
use crate::compilation::Compilation;
use crate::fact::CompilationFactKey;

impl Compilation {
    /// Returns the current library product's interface export, when configured.
    pub fn package_interface_export_bundle(
        &self,
    ) -> Option<&Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError>> {
        self.state.package_interface_export.as_ref()?;

        Some(self.evaluate_query(
            CompilationFactKey::PackageInterfaceExportBundle,
            &self.state.package_interface_export_bundle,
            || {
                let request = self
                    .package_interface_export_request()
                    .unwrap_or_else(|| panic!("export semantics requires its configured request"));

                let span = self.state.fact_runtime.profile().map(|profile| {
                    profile.start(crate::profile::ProfileOperation::InterfaceExport, None)
                });

                let result = self.build_package_interface_export_bundle(request);

                if let Some(span) = span {
                    span.finish(crate::profile::result_outcome(&result));
                }

                result
            },
        ))
    }

    pub(in crate::compilation::export::build) fn build_package_interface_export_bundle(
        &self,
        request: &crate::PackageInterfaceExportRequest,
    ) -> Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError> {
        if self.product_kind() != ProductKind::Library
            || request.identity().kind() != InterfaceProductKind::Library
            || request.identity().package() != self.package_identity()
        {
            return Err(super::super::super::export_contract_error(
                super::super::super::PackageInterfaceExportContract::RequestMismatch,
            ));
        }

        let source_graph = self
            .product_source_graph()
            .map_err(super::super::super::invalid_compilation_fact_error)?;

        if self.source_diagnostics().has_errors()
            || self.syntax_tree_result().diagnostics().has_errors()
            || source_graph.diagnostics().has_errors()
            || self
                .execution_guarantee_diagnostics(source_graph)
                .map_err(super::super::super::invalid_compilation_fact_error)?
                .has_errors()
            || self.imported_diagnostics().has_errors()
        {
            // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let product = self
            .product_semantics()
            .map_err(super::super::super::invalid_compilation_fact_error)?;

        if product.diagnostics().has_errors() || product.value().is_recovered() {
            // rust-style: allow(context-erasing-failure-conversion, reason = "source and semantic diagnostics retain the exact causes")
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let symbols = self
            .symbol_graph()
            .map_err(super::super::super::invalid_compilation_fact_error)?;

        let identity = build_identity_surface(self, symbols, product.value().public_symbols())?;

        let dependencies = self
            .loaded_interface_views(&self.state.cancellation)
            .map_err(super::super::super::invalid_compilation_fact_error)?
            .ok_or_else(|| {
                super::super::super::export_contract_error(
                    super::super::super::PackageInterfaceExportContract::MissingLoadedDependencies,
                )
            })?
            .into_iter()
            .map(|dependency| {
                let identity = dependency.surface().identity();

                InterfaceDependency::new(
                    identity.package().clone(),
                    identity.product().clone(),
                    dependency.content_hash(),
                )
            })
            .collect::<Vec<_>>();

        // Package-interface identities are Arc-backed and retained by the immutable surface.
        let package_identity = request.identity().clone();

        let surface = build_package_interface_surface(
            package_identity,
            dependencies,
            identity.symbols,
            identity.relationships,
            identity.exports,
        )
        .map_err(PackageInterfaceExportError::Surface)?;

        let (semantics, constant_callable_bodies, executable_templates, native_boundaries) =
            super::super::super::semantic::build_semantics(
                self,
                symbols,
                &surface,
                &identity.selected,
                &identity.keys,
            )?;

        let implementation_configuration = self
            .package_implementation_configuration(None)
            .map_err(|cause| {
                PackageInterfaceExportError::InvalidCompilationCause(
                    super::super::super::PackageInterfaceInvalidCompilationCause::CodegenTarget(
                        cause,
                    ),
                )
            })?;

        PackageInterfaceExportBundle::try_new(
            surface,
            semantics,
            request.language_revision(),
            implementation_configuration,
        )
        .and_then(|bundle| bundle.with_constant_callable_bodies(constant_callable_bodies))
        .and_then(|bundle| bundle.with_executable_templates(executable_templates))
        .and_then(|bundle| bundle.with_native_boundaries(native_boundaries))
        .map(Arc::new)
        .map_err(PackageInterfaceExportError::Bundle)
    }
}
