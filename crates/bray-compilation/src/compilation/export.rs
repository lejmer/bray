use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_package_interface::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    ExportedLookupKind, InterfaceSemanticFacts, PackageInterfaceExportBuildError,
    PackageInterfaceExportBundle, PackageInterfaceExportSurfaceError, SymbolRelationshipKind,
    build_package_interface_surface,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, ModuleSurfaceFact, ModuleSymbolId, SymbolFactRequest,
    SymbolKind, SymbolOrigin,
};

use super::Compilation;
use crate::fact::CompilationFactKey;

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageInterfaceExportError {
    /// Source, syntax, declaration, or imported-interface errors make the product invalid.
    InvalidCompilation,
    /// A recovered public module cannot supply a stable external identity.
    RecoveredPublicModule,
    /// A reachable public declaration does not have a complete serializable fact set.
    IncompletePublicDeclarationFacts(SymbolKind),
    /// Canonical identity-surface validation rejected the selected graph.
    Surface(PackageInterfaceExportSurfaceError),
    /// Semantic or support-graph validation rejected the export bundle.
    Bundle(PackageInterfaceExportBuildError),
}

impl Compilation {
    /// Returns the current library product's interface export, when configured.
    pub fn package_interface_export_bundle(
        &self,
    ) -> Option<&Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError>> {
        let request = self.state.package_interface_export.as_ref()?;

        Some(self.fact(
            CompilationFactKey::PackageInterfaceExportBundle,
            &self.state.package_interface_export_bundle,
            || self.build_package_interface_export_bundle(request),
        ))
    }

    fn build_package_interface_export_bundle(
        &self,
        request: &crate::PackageInterfaceExportRequest,
    ) -> Result<Arc<PackageInterfaceExportBundle>, PackageInterfaceExportError> {
        if self.source_diagnostics().has_errors()
            || self.syntax_tree_result().diagnostics().has_errors()
            || self.declaration_diagnostics().has_errors()
            || self.imported_diagnostics().has_errors()
        {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        let symbols = self
            .symbol_graph()
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        reject_unavailable_public_declarations(self, symbols)?;

        // Package identities and external keys are Arc-backed values retained by the bundle.
        let package_key = ExternalSymbolKey::package(self.package_identity().clone());

        let mut selected = vec![ExportSymbolInput::new(package_key.clone(), None)];
        let mut relationships = Vec::new();
        let mut module_keys = BTreeMap::new();

        for (ordinal, module) in symbols
            .modules()
            .iter()
            .filter(|module| {
                module.origin() == SymbolOrigin::Source && module.visibility().is_public()
            })
            .enumerate()
        {
            if module.is_recovered() {
                return Err(PackageInterfaceExportError::RecoveredPublicModule);
            }

            let module_key = ExternalSymbolKey::module(package_key.clone(), module.path().clone())
                .ok_or(PackageInterfaceExportError::RecoveredPublicModule)?;

            selected.push(ExportSymbolInput::new(
                module_key.clone(),
                Some(package_key.clone()),
            ));

            module_keys.insert(module.id(), module_key.clone());

            let ordinal = u32::try_from(ordinal)
                .map_err(|_| PackageInterfaceExportSurfaceError::SymbolCountOverflow)
                .map_err(PackageInterfaceExportError::Surface)?;

            relationships.push(ExportRelationshipInput::new(
                SymbolRelationshipKind::PackageModule,
                package_key.clone(),
                module_key,
                ordinal,
            ));
        }

        let exports = self.public_module_re_exports(&module_keys)?;

        if request.identity().package() != self.package_identity() {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        // Package-interface identities are Arc-backed and the frozen surface owns its snapshot.
        let identity = request.identity().clone();

        let surface =
            build_package_interface_surface(identity, [], selected, relationships, exports)
                .map_err(PackageInterfaceExportError::Surface)?;

        PackageInterfaceExportBundle::try_new(
            surface,
            InterfaceSemanticFacts::new(),
            request.language_revision(),
        )
        .map(Arc::new)
        .map_err(PackageInterfaceExportError::Bundle)
    }

    fn public_module_re_exports(
        &self,
        module_keys: &BTreeMap<ModuleSymbolId, ExternalSymbolKey>,
    ) -> Result<Vec<ExportLookupInput>, PackageInterfaceExportError> {
        let facts = self
            .binder_facts(&self.state.cancellation)
            .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

        let mut exports = Vec::new();

        for (module, owner_key) in module_keys {
            let surface = facts
                .symbol_fact(SymbolFactRequest::<ModuleSurfaceFact>::new(*module))
                .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

            if surface.diagnostics().has_errors() {
                return Err(PackageInterfaceExportError::InvalidCompilation);
            }

            for edge in surface
                .value()
                .re_exports()
                .iter()
                .filter(|edge| edge.visibility().is_public())
            {
                let AnySymbolId::Module(target) = edge.target() else {
                    return Err(
                        PackageInterfaceExportError::IncompletePublicDeclarationFacts(
                            edge.target().kind(),
                        ),
                    );
                };

                let Some(target_key) = module_keys.get(&target) else {
                    return Err(
                        PackageInterfaceExportError::IncompletePublicDeclarationFacts(
                            SymbolKind::Module,
                        ),
                    );
                };

                // Export inputs share Arc-backed keys and names with the immutable source facts.
                exports.push(ExportLookupInput::new(
                    owner_key.clone(),
                    edge.name().clone(),
                    ExportedLookupKind::ReExport,
                    ExportSymbolReferenceInput::Local(target_key.clone()),
                ));
            }
        }

        Ok(exports)
    }
}

fn reject_unavailable_public_declarations(
    compilation: &Compilation,
    symbols: &bray_symbols::SymbolGraph,
) -> Result<(), PackageInterfaceExportError> {
    for declaration in compilation.declaration_table().declarations() {
        if declaration.surface().is_internal() {
            continue;
        }

        let Some(symbol) = symbols.symbol_for_declaration(declaration.id()) else {
            continue;
        };

        if symbol_is_publicly_reachable(compilation, symbols, symbol) {
            return Err(
                PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind()),
            );
        }
    }

    Ok(())
}

fn symbol_is_publicly_reachable(
    compilation: &Compilation,
    symbols: &bray_symbols::SymbolGraph,
    mut symbol: AnySymbolId,
) -> bool {
    loop {
        if let Some(declaration) = symbols
            .symbol_key(symbol)
            .and_then(bray_symbols::SymbolKey::source_declaration_id)
            .and_then(|declaration| compilation.declaration_table().declaration(declaration))
            && declaration.surface().is_internal()
        {
            return false;
        }

        if let AnySymbolId::Module(module) = symbol {
            return symbols.module(module).is_some_and(|module| {
                module.origin() == SymbolOrigin::Source && module.visibility().is_public()
            });
        }

        let Some(owner) = symbols.containing_symbol(symbol) else {
            return false;
        };

        symbol = owner;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, PackageInterfaceExportBundle,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, SymbolKind};

    use super::PackageInterfaceExportError;
    use crate::{Compilation, CompilationRequest, PackageInterfaceExportRequest};

    #[test]
    fn module_only_library_exports_are_lazy_cached_facts() {
        let compilation = compilation("module app;");

        assert!(
            compilation
                .state
                .package_interface_export_bundle
                .get()
                .is_none()
        );

        let first = export(&compilation);
        let second = export(&compilation);

        assert!(std::ptr::eq(first, second));
        assert_eq!(first.surface().symbols().symbols().len(), 2);

        assert!(
            compilation
                .state
                .package_interface_export_bundle
                .get()
                .is_some()
        );
    }

    #[test]
    fn public_declarations_without_serializable_completed_facts_are_rejected() {
        let compilation =
            compilation(concat!("module app;\n", "\n", "func run()\n", "{\n", "}\n",));

        assert_eq!(
            compilation.package_interface_export_bundle(),
            Some(&Err(
                PackageInterfaceExportError::IncompletePublicDeclarationFacts(SymbolKind::Function)
            ))
        );
    }

    #[test]
    fn internal_owner_chains_do_not_enter_the_public_export_selection() {
        let compilation = compilation(concat!(
            "module app;\n",
            "internal struct Hidden\n",
            "{\n",
            "    func method()\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let bundle = export(&compilation);

        assert_eq!(bundle.surface().symbols().symbols().len(), 2);
    }

    #[test]
    fn public_module_re_exports_enter_the_interface_lookup_surface() {
        let compilation =
            compilation_from_sources(["module a;\n", concat!("module b;\n", "\n", "export a;\n",)]);

        let bundle = export(&compilation);
        let [edge] = bundle.surface().exports() else {
            panic!(
                "expected one module re-export: {:?}",
                bundle.surface().exports()
            );
        };

        assert_eq!(edge.name().as_str(), "a");
        assert_eq!(
            edge.kind(),
            bray_package_interface::ExportedLookupKind::ReExport
        );
    }

    fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
        match compilation.package_interface_export_bundle() {
            Some(Ok(bundle)) => bundle,
            Some(Err(error)) => panic!("test library interface must build: {error:?}"),
            None => panic!("test compilation must configure a library interface"),
        }
    }

    fn compilation(source: &str) -> Compilation {
        compilation_from_sources([source])
    }

    fn compilation_from_sources<const N: usize>(sources: [&str; N]) -> Compilation {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
            package.clone(),
            product,
            bray_package_interface::InterfaceProductKind::Library,
            "public-v1",
        )
        .unwrap_or_else(|| panic!("test export identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let sources = sources.into_iter().enumerate().map(|(index, source)| {
            let index = u32::try_from(index)
                .unwrap_or_else(|_| panic!("test source count must fit source identities"));

            SourceInput::virtual_text(
                SourceIdentity::new(index),
                format!("test-{index}.bray"),
                SourceVersion::new(0),
                source,
            )
        });

        let request = CompilationRequest::new(package, sources.collect())
            .with_package_interface_export(export);

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }
}
