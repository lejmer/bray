use std::sync::Arc;

use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};
use bray_package_interface::{
    ImportedInterfaceSymbolResolver, ImportedSemanticFacts, LoadedInterfaceSurface,
    PackageInterfaceSurface, ValidatedPackageInterface, construct_imported_symbol_skeletons,
};
use bray_symbols::{ImportedInterfaceId, ImportedSymbolSkeleton, PackageIdentity, SymbolId};

use super::diagnostic::{interface_diagnostics, validation_diagnostics};
use super::model::LoadedDependencyInterface;
use crate::fact::{CompilationFactKey, FactQueryError};
use crate::request::DependencyInterfaceInput;

impl super::super::Compilation {
    /// Resolves the canonical compilation-local handle for one selected dependency interface.
    pub fn dependency_interface_id(
        &self,
        package: &PackageIdentity,
        product: &bray_package_interface::InterfaceProductIdentity,
    ) -> Option<ImportedInterfaceId> {
        self.state
            .dependency_interfaces
            .binary_search_by(|input| (input.package(), input.product()).cmp(&(package, product)))
            .ok()
            .and_then(ImportedInterfaceId::try_from_index)
    }

    /// Returns a lazily validated identity surface and its owned diagnostics.
    pub fn dependency_interface_result(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&DiagnosticResult<Option<PackageInterfaceSurface>>> {
        self.loaded_dependency_interface(interface)
            .map(LoadedDependencyInterface::result)
    }

    /// Returns the lazily constructed deterministic imported identity skeleton.
    pub fn imported_symbol_skeleton_result(
        &self,
    ) -> Result<&DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        self.query_fact(
            CompilationFactKey::ImportedSymbolSkeleton,
            &self.state.imported_symbol_skeleton,
            || self.compute_imported_symbol_skeleton(),
        )
    }

    /// Returns lazily decoded and remapped semantic and template facts for one interface.
    pub fn imported_semantic_facts_result(
        &self,
        interface: ImportedInterfaceId,
    ) -> Result<Option<&DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>>, FactQueryError> {
        let Some(index) = interface.to_index() else {
            return Ok(None);
        };

        let Some(cache) = self.state.imported_semantic_facts.get(index) else {
            return Ok(None);
        };

        self.query_fact(
            CompilationFactKey::ImportedSemanticFacts(interface),
            cache,
            || self.compute_imported_semantic_facts(interface),
        )
        .map(Some)
    }

    /// Returns diagnostics owned by all selected compiled dependency interfaces.
    pub fn imported_diagnostics(&self) -> &DiagnosticBag {
        self.fact(
            CompilationFactKey::ImportedDiagnostics,
            &self.state.imported_diagnostics,
            || self.compute_imported_diagnostics(),
        )
    }

    fn loaded_dependency_interface(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&LoadedDependencyInterface> {
        let index = interface.to_index()?;
        let input = self.state.dependency_interfaces.get(index)?;
        let cache = self.state.loaded_dependency_interfaces.get(index)?;

        Some(self.fact(
            CompilationFactKey::DependencyInterface(interface),
            cache,
            || load_dependency_interface(input),
        ))
    }

    fn compute_imported_symbol_skeleton(
        &self,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSymbolSkeleton>>>, FactQueryError> {
        let mut loaded = Vec::with_capacity(self.state.dependency_interfaces.len());
        let mut diagnostics = Vec::new();

        for index in 0..self.state.dependency_interfaces.len() {
            let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                return Ok(DiagnosticResult::new(
                    None,
                    interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid),
                ));
            };

            let Some(fact) = self.loaded_dependency_interface(interface) else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            diagnostics.push(fact.result().diagnostics());

            let (Some(validated), Some(surface)) = (fact.validated(), fact.surface()) else {
                continue;
            };

            loaded.push(LoadedInterfaceSurface::new(
                interface,
                validated.header().content_hash(),
                surface,
            ));
        }

        let diagnostics = DiagnosticBag::merged_all(diagnostics);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let graph = self.symbol_graph()?;
        let Some(first_symbol) = u32::try_from(graph.next_symbol_index())
            .ok()
            .map(SymbolId::new)
        else {
            return Ok(DiagnosticResult::new(
                None,
                interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid),
            ));
        };

        match construct_imported_symbol_skeletons(first_symbol, loaded) {
            Ok(symbols) => Ok(DiagnosticResult::new(Some(Arc::new(symbols)), diagnostics)),
            Err(_) => Ok(DiagnosticResult::new(
                None,
                interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid),
            )),
        }
    }

    fn compute_imported_semantic_facts(
        &self,
        interface: ImportedInterfaceId,
    ) -> Result<DiagnosticResult<Option<Arc<ImportedSemanticFacts>>>, FactQueryError> {
        let Some(loaded) = self.loaded_dependency_interface(interface) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let (Some(validated), Some(surface)) = (loaded.validated(), loaded.surface()) else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let skeleton = self.imported_symbol_skeleton_result()?;
        let Some(skeleton) = skeleton.value() else {
            return Ok(DiagnosticResult::without_diagnostics(None));
        };

        let decoded = match validated.decode_semantic_facts(surface) {
            Ok(decoded) => decoded,
            Err(error) => {
                return Ok(DiagnosticResult::new(None, validation_diagnostics(error)));
            }
        };

        let interfaces = self
            .loaded_interface_views()
            .ok_or(FactQueryError::InfrastructureFailure)?;
        let Some(current) = interfaces
            .iter()
            .copied()
            .find(|loaded| loaded.interface() == interface)
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };
        let resolver = ImportedInterfaceSymbolResolver::try_new(current, interfaces, skeleton)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;
        let semantic_values = self.semantic_value_store()?;

        match decoded.intern(semantic_values, &resolver) {
            Ok(facts) => Ok(DiagnosticResult::without_diagnostics(Some(Arc::new(facts)))),
            Err(_) => Ok(DiagnosticResult::new(
                None,
                interface_diagnostics(DiagnosticKind::InterfaceSemanticFactsInvalid),
            )),
        }
    }

    fn loaded_interface_views(&self) -> Option<Vec<LoadedInterfaceSurface<'_>>> {
        (0..self.state.dependency_interfaces.len())
            .map(|index| {
                let interface = ImportedInterfaceId::try_from_index(index)?;
                let loaded = self.loaded_dependency_interface(interface)?;
                let validated = loaded.validated()?;
                let surface = loaded.surface()?;

                Some(LoadedInterfaceSurface::new(
                    interface,
                    validated.header().content_hash(),
                    surface,
                ))
            })
            .collect()
    }

    fn compute_imported_diagnostics(&self) -> DiagnosticBag {
        let skeleton = match self.imported_symbol_skeleton_result() {
            Ok(result) => result,
            Err(error) => panic!("imported symbol diagnostics failed: {error:?}"),
        };

        let mut diagnostics = vec![skeleton.diagnostics()];

        if skeleton.value().is_some() {
            for index in 0..self.state.dependency_interfaces.len() {
                let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
                    return interface_diagnostics(DiagnosticKind::InterfaceDependencyGraphInvalid);
                };

                match self.imported_semantic_facts_result(interface) {
                    Ok(Some(result)) => diagnostics.push(result.diagnostics()),
                    Ok(None) => {}
                    Err(error) => panic!("imported semantic diagnostics failed: {error:?}"),
                }
            }
        }

        DiagnosticBag::merged_all(diagnostics)
    }
}

fn load_dependency_interface(input: &DependencyInterfaceInput) -> LoadedDependencyInterface {
    let validated =
        match ValidatedPackageInterface::try_new(input.shared_bytes(), input.validation_policy()) {
            Ok(validated) => validated,
            Err(error) => {
                return LoadedDependencyInterface::invalid(validation_diagnostics(error));
            }
        };

    let surface = match validated.decode_identity_surface() {
        Ok(surface) => surface,
        Err(error) => {
            return LoadedDependencyInterface::invalid(validation_diagnostics(error));
        }
    };

    if surface.identity().package() != input.package() {
        return LoadedDependencyInterface::invalid(interface_diagnostics(
            DiagnosticKind::InterfacePackageIdentityMismatch,
        ));
    }

    if surface.identity().product() != input.product() {
        return LoadedDependencyInterface::invalid(interface_diagnostics(
            DiagnosticKind::InterfaceProductIdentityMismatch,
        ));
    }

    LoadedDependencyInterface::new(validated, surface)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use crate::test_support::diagnostic_kinds;
    use crate::{Compilation, CompilationRequest, DependencyInterfaceInput};

    #[test]
    fn dependency_interface_validation_is_lazy_cached_and_diagnostic_backed() {
        let compilation = compilation([dependency("example.alpha", "main")]);

        assert!(
            compilation.state.loaded_dependency_interfaces[0]
                .get()
                .is_none()
        );
        assert!(compilation.state.imported_symbol_skeleton.get().is_none());

        let interface = interface_id(&compilation, "example.alpha", "main");
        let first = compilation
            .dependency_interface_result(interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));
        let second = compilation
            .dependency_interface_result(interface)
            .unwrap_or_else(|| panic!("selected dependency interface must have a result"));

        assert!(std::ptr::eq(first, second));
        assert!(first.value().is_none());
        assert_eq!(
            diagnostic_kinds(first.diagnostics()),
            [DiagnosticKind::InterfaceInvalidMagic]
        );

        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("skeleton query must complete: {error:?}"));

        assert!(skeleton.value().is_none());
        assert_eq!(
            diagnostic_kinds(compilation.imported_diagnostics()),
            [DiagnosticKind::InterfaceInvalidMagic]
        );
        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::InterfaceInvalidMagic)
        );
    }

    #[test]
    fn interface_handles_ignore_dependency_input_order() {
        let forward = compilation([
            dependency("example.alpha", "main"),
            dependency("example.beta", "test"),
        ]);
        let reversed = compilation([
            dependency("example.beta", "test"),
            dependency("example.alpha", "main"),
        ]);

        assert_eq!(
            interface_id(&forward, "example.alpha", "main"),
            interface_id(&reversed, "example.alpha", "main")
        );
        assert_eq!(
            interface_id(&forward, "example.beta", "test"),
            interface_id(&reversed, "example.beta", "test")
        );
    }

    #[test]
    fn concurrent_imported_skeleton_requests_publish_one_result() {
        let compilation = compilation([dependency("example.alpha", "main")]);

        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| compilation.imported_symbol_skeleton_result());
            let second = scope.spawn(|| compilation.imported_symbol_skeleton_result());

            [first, second].map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| panic!("imported skeleton query thread must not panic"))
                    .unwrap_or_else(|error| {
                        panic!("imported skeleton query must complete: {error:?}")
                    })
            })
        });

        assert!(std::ptr::eq(results[0], results[1]));
    }

    fn compilation(
        dependencies: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Compilation {
        let request = CompilationRequest::new(
            package("example.current"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(1),
                "main.bray",
                SourceVersion::new(1),
                "module example.current;",
            )],
        )
        .with_dependency_interfaces(dependencies);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }

    fn dependency(package_name: &str, product_name: &str) -> DependencyInterfaceInput {
        DependencyInterfaceInput::new(
            package(package_name),
            product(product_name),
            Arc::<[u8]>::from(vec![0; 112]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }

    fn interface_id(
        compilation: &Compilation,
        package_name: &str,
        product_name: &str,
    ) -> bray_symbols::ImportedInterfaceId {
        compilation
            .dependency_interface_id(&package(package_name), &product(product_name))
            .unwrap_or_else(|| panic!("selected dependency interface must have an ID"))
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        InterfaceProductIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }
}
