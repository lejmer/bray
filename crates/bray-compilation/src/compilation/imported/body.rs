use std::sync::Arc;

use bray_bound_tree::CheckedTemplate;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::ImportedInterfaceSymbolResolver;
use bray_symbols::ImportedSymbolFactAddress;

use super::diagnostic::implementation_body_diagnostics;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl super::super::Compilation {
    pub(in crate::compilation) fn imported_constant_callable_body_with_cancellation(
        &self,
        address: ImportedSymbolFactAddress,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<Arc<CheckedTemplate>>>>, FactQueryError> {
        let cell = self.state.imported_constant_callable_bodies.cell(address)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImportedConstantCallableBody(address),
            cancellation,
            || {
                self.compute_imported_constant_callable_body(address, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_imported_constant_callable_body(
        &self,
        address: ImportedSymbolFactAddress,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<Arc<CheckedTemplate>>>, FactQueryError> {
        let input = self
            .dependency_interface_input(address.interface())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let loaded = self
            .loaded_dependency_interface_with_cancellation(address.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let Some(artifact) = input.implementation_artifact() else {
            return Ok(DiagnosticResult::new(
                None,
                implementation_body_diagnostics(input),
            ));
        };

        if artifact.interface_content_hash() != validated.header().content_hash()
            || artifact.language_revision() != validated.header().language_revision()
        {
            return Ok(DiagnosticResult::new(
                None,
                implementation_body_diagnostics(input),
            ));
        }

        let graph_result = self
            .imported_semantic_graph_result_with_cancellation(address.interface(), cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(graph) = graph_result.value() else {
            return Ok(DiagnosticResult::new(
                None,
                graph_result.diagnostics().clone(),
            ));
        };

        let body = match artifact.constant_callable_body(
            address.symbol(),
            surface,
            input.validation_policy().limits(),
        ) {
            Ok(Some(body)) => body,
            Ok(None) | Err(_) => {
                return Ok(DiagnosticResult::new(
                    None,
                    implementation_body_diagnostics(input),
                ));
            }
        };

        let skeleton = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::new(None, skeleton.diagnostics().clone()));
        };

        cancellation.check()?;

        let interfaces = self
            .loaded_interface_views(cancellation)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(current) = interfaces
            .iter()
            .copied()
            .find(|loaded| loaded.interface() == address.interface())
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let resolver = ImportedInterfaceSymbolResolver::try_new(current, interfaces, skeleton)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let template = match graph.intern_checked_template(body.template(), &resolver) {
            Ok(template) => template,
            Err(_) => {
                return Ok(DiagnosticResult::new(
                    None,
                    DiagnosticBag::merged_all([
                        graph_result.diagnostics(),
                        &implementation_body_diagnostics(input),
                    ]),
                ));
            }
        };

        Ok(DiagnosticResult::new(
            Some(Arc::new(template)),
            graph_result.diagnostics().clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::CheckedTemplateKind;
    use bray_diagnostics::DiagnosticKind;
    use bray_package_interface::{
        InterfaceCheckedTemplate, InterfaceConstantCallableBody, InterfaceLanguageRevision,
        InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
        ValidatedPackageInterface, encode_package_interface,
        test_support::package_interface_export_bundle,
    };
    use bray_symbols::{ImportedSymbolFactAddress, SymbolKind};

    use crate::test_support::compilation_with_dependencies;
    use crate::{Compilation, DependencyInterfaceInput};

    #[test]
    fn imported_constant_bodies_are_loaded_once_by_exact_symbol_address() {
        let (compilation, owner) = imported_body_compilation(true);

        let address = constant_body_address(&compilation, owner);

        let first = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("imported constant body must load: {error:?}"));

        let second = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("imported constant body must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let Some(body) = first.value() else {
            panic!("matching implementation artifact must publish the requested body");
        };

        assert_eq!(body.kind(), CheckedTemplateKind::ConstantCallableBody);
    }

    #[test]
    fn missing_imported_constant_bodies_publish_structured_diagnostics() {
        let (compilation, owner) = imported_body_compilation(false);

        let address = constant_body_address(&compilation, owner);

        let result = compilation
            .imported_constant_callable_body_with_cancellation(
                address,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("missing imported body must recover: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::InterfaceConstantCallableBodyUnavailable)
                .count(),
            1
        );
    }

    fn imported_body_compilation(
        include_artifact: bool,
    ) -> (Compilation, bray_symbols::InterfaceSymbolId) {
        let bundle = package_interface_export_bundle();

        let encoded = encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

        let interface = ValidatedPackageInterface::try_new(encoded.bytes(), policy)
            .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"));

        let owner = bundle
            .surface()
            .symbols()
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.id())
            .unwrap_or_else(|| panic!("test interface must export a function"));

        let template = bundle
            .semantic_facts()
            .checked_templates()
            .first()
            .unwrap_or_else(|| panic!("test interface must publish a checked template"));

        let template = InterfaceCheckedTemplate::new(
            CheckedTemplateKind::ConstantCallableBody,
            template.inputs().iter().cloned(),
            template.nodes().iter().cloned(),
            template.temporaries().iter().copied(),
            template.result(),
            template.behavior().clone(),
        );

        let artifact = PackageImplementationArtifact::try_new(
            &interface,
            bundle.surface(),
            bundle.semantic_facts(),
            [InterfaceConstantCallableBody::new(owner, template)],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("test implementation artifact must validate: {error:?}"));

        let mut dependency = DependencyInterfaceInput::new(
            bundle.surface().identity().package().clone(),
            bundle.surface().identity().product().clone(),
            "dependency.brayi",
            Arc::<[u8]>::from(encoded.bytes()),
            policy,
        );

        if include_artifact {
            dependency =
                dependency.with_implementation_artifact("dependency.brayimpl", Arc::new(artifact));
        }

        (
            compilation_with_dependencies("module example.current;", [dependency]),
            owner,
        )
    }

    fn constant_body_address(
        compilation: &Compilation,
        owner: bray_symbols::InterfaceSymbolId,
    ) -> ImportedSymbolFactAddress {
        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

        let skeleton = skeleton
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("valid dependency must publish a symbol skeleton"));

        let function = skeleton
            .functions()
            .iter()
            .find(|function| {
                function
                    .imported_fact_key()
                    .is_some_and(|key| key.symbol() == owner)
            })
            .unwrap_or_else(|| panic!("imported function must retain its interface address"));

        skeleton
            .imported_fact_address(function.id().into())
            .unwrap_or_else(|| panic!("imported function must publish a fact address"))
    }
}
