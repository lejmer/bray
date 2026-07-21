use std::sync::Arc;

use bray_declarations::DeclarationKind;
use bray_package_interface::{
    ExportRelationshipInput, ExportSymbolInput, InterfaceSemanticFacts,
    PackageInterfaceExportBuildError, PackageInterfaceExportBundle,
    PackageInterfaceExportSurfaceError, SymbolRelationshipKind, build_package_interface_surface,
};
use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, ModuleOwnerId, ModulePathKey, SymbolKind, SymbolOrigin,
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
    /// A reachable using or export declaration has no completed public lookup fact.
    IncompletePublicLookupFacts(DeclarationKind),
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

        if request.identity().package() != self.package_identity() {
            return Err(PackageInterfaceExportError::InvalidCompilation);
        }

        // Package-interface identities are Arc-backed and the frozen surface owns its snapshot.
        let identity = request.identity().clone();

        let surface = build_package_interface_surface(identity, [], selected, relationships, [])
            .map_err(PackageInterfaceExportError::Surface)?;

        PackageInterfaceExportBundle::try_new(
            surface,
            InterfaceSemanticFacts::new(),
            request.language_revision(),
        )
        .map(Arc::new)
        .map_err(PackageInterfaceExportError::Bundle)
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
            if matches!(
                declaration.kind(),
                DeclarationKind::Using | DeclarationKind::Export
            ) && declaration_module_is_public(compilation, symbols, declaration)
            {
                return Err(PackageInterfaceExportError::IncompletePublicLookupFacts(
                    declaration.kind(),
                ));
            }

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

fn declaration_module_is_public(
    compilation: &Compilation,
    symbols: &bray_symbols::SymbolGraph,
    declaration: &bray_declarations::DeclarationRecord,
) -> bool {
    let Some(container) = compilation
        .declaration_table()
        .container(declaration.owning_container())
    else {
        return false;
    };

    let Some(path) = container.module_path() else {
        return false;
    };

    let Some(path) = ModulePathKey::try_new(path.segments().iter().map(String::as_str)) else {
        return false;
    };

    let Some(package) = symbols.roots().packages().first().copied() else {
        return false;
    };

    symbols
        .module_by_path(ModuleOwnerId::from(package), &path)
        .is_some_and(|module| {
            module.origin() == SymbolOrigin::Source && module.visibility().is_public()
        })
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

    fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
        match compilation.package_interface_export_bundle() {
            Some(Ok(bundle)) => bundle,
            Some(Err(error)) => panic!("test library interface must build: {error:?}"),
            None => panic!("test compilation must configure a library interface"),
        }
    }

    fn compilation(source: &str) -> Compilation {
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

        let source = SourceInput::virtual_text(
            SourceIdentity::new(1),
            "test.bray",
            SourceVersion::new(0),
            source,
        );

        Compilation::load(
            CompilationRequest::new(package, vec![source]).with_package_interface_export(export),
        )
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
    }
}
