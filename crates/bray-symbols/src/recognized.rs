use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_compiler_known::{
    AvailabilityRule, COMPILER_KNOWN_CATALOG, CompilerKnownCatalog,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationIdentity,
    RecognizedStandardLibraryDeclarationKey, RecognizedStandardLibraryDeclarationOwner,
};

use crate::availability::resolve_owned_availability;
use crate::surface_kind::{DeclarationSurfaceKind, declaration_symbol_kind};
use crate::{
    AnySymbolId, ExactSymbolId, ExternalSymbolKey, ImportedSymbolSkeleton, ModulePathKey,
    PackageIdentity, SymbolName, SymbolOrdinal,
};

/// One exact association between a recognition descriptor and an ordinary imported declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecognizedStandardLibraryDeclarationMatch {
    descriptor: RecognizedStandardLibraryDeclarationId,
    symbol: AnySymbolId,
}

impl RecognizedStandardLibraryDeclarationMatch {
    /// Returns the stable catalog descriptor identity.
    pub const fn descriptor(self) -> RecognizedStandardLibraryDeclarationId {
        self.descriptor
    }

    /// Returns the existing ordinary imported symbol identity.
    pub const fn symbol(self) -> AnySymbolId {
        self.symbol
    }
}

/// An immutable target-filtered recognition view over ordinary imported declarations.
///
/// Recognition associates catalog descriptors with exact stable external identities. It does not
/// alter imported records, ordinary lookup, visibility, or symbol kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecognizedStandardLibraryDeclarations {
    imported: Arc<ImportedSymbolSkeleton>,
    declarations: Box<[RecognizedStandardLibraryDeclarationMatch]>,
    descriptor_symbols: BTreeMap<RecognizedStandardLibraryDeclarationId, AnySymbolId>,
    symbol_descriptors: BTreeMap<AnySymbolId, RecognizedStandardLibraryDeclarationId>,
}

impl RecognizedStandardLibraryDeclarations {
    fn build(
        imported: Arc<ImportedSymbolSkeleton>,
        standard_library_package: &PackageIdentity,
        mut rule_is_available: impl FnMut(AvailabilityRule) -> bool,
    ) -> Self {
        let catalog = &COMPILER_KNOWN_CATALOG;
        let descriptor_keys = recognized_external_keys(catalog, standard_library_package);

        let direct_availability = catalog
            .recognized_standard_library_declarations()
            .iter()
            .map(|descriptor| {
                (
                    descriptor.id(),
                    rule_is_available(descriptor.availability_rule()),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let mut resolved_availability = BTreeMap::new();
        let mut resolving_availability = BTreeSet::new();

        let mut declarations = Vec::new();

        for descriptor in catalog.recognized_standard_library_declarations() {
            let available = resolve_owned_availability(
                descriptor.id(),
                &direct_availability,
                &mut resolved_availability,
                &mut resolving_availability,
                |declaration| {
                    let descriptor =
                        catalog.recognized_standard_library_declaration(declaration)?;

                    match descriptor.owner() {
                        RecognizedStandardLibraryDeclarationOwner::Scope(_) => None,
                        RecognizedStandardLibraryDeclarationOwner::Declaration(owner) => {
                            Some(owner)
                        }
                    }
                },
            );

            if !available {
                continue;
            }

            let Some(external_key) = descriptor_keys.get(&descriptor.id()) else {
                continue;
            };

            let Some(symbol) = imported.symbol_by_external_key(external_key) else {
                continue;
            };

            declarations.push(RecognizedStandardLibraryDeclarationMatch {
                descriptor: descriptor.id(),
                symbol,
            });
        }

        let descriptor_symbols = declarations
            .iter()
            .map(|matched| (matched.descriptor(), matched.symbol()))
            .collect();

        let symbol_descriptors = declarations
            .iter()
            .map(|matched| (matched.symbol(), matched.descriptor()))
            .collect();

        Self {
            imported,
            declarations: declarations.into_boxed_slice(),
            descriptor_symbols,
            symbol_descriptors,
        }
    }

    /// Returns the unchanged imported symbol provider underlying this recognition view.
    pub fn imported(&self) -> &ImportedSymbolSkeleton {
        &self.imported
    }

    /// Returns recognized declarations in canonical catalog order.
    pub fn declarations(&self) -> &[RecognizedStandardLibraryDeclarationMatch] {
        &self.declarations
    }

    /// Returns the descriptor associated with an exact ordinary imported symbol.
    pub fn descriptor(
        &self,
        symbol: AnySymbolId,
    ) -> Option<RecognizedStandardLibraryDeclarationId> {
        self.symbol_descriptors.get(&symbol).copied()
    }

    /// Returns a recognized declaration by stable catalog key and exact symbol category.
    pub fn declaration_symbol<I: ExactSymbolId>(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
    ) -> Option<I> {
        let descriptor =
            COMPILER_KNOWN_CATALOG.recognized_standard_library_declaration_by_key(key)?;

        let symbol = self.descriptor_symbols.get(&descriptor.id()).copied()?;

        I::try_from_any(symbol)
    }
}

impl ImportedSymbolSkeleton {
    /// Recognizes available standard-library declarations through exact imported identities.
    ///
    /// `standard_library_package` must be the package identity already validated by package and
    /// interface loading. No package or declaration is recognized from spelling or lookup names.
    pub fn recognize_standard_library(
        self: Arc<Self>,
        standard_library_package: &PackageIdentity,
        rule_is_available: impl FnMut(AvailabilityRule) -> bool,
    ) -> RecognizedStandardLibraryDeclarations {
        RecognizedStandardLibraryDeclarations::build(
            self,
            standard_library_package,
            rule_is_available,
        )
    }
}

fn recognized_external_keys(
    catalog: &CompilerKnownCatalog,
    package: &PackageIdentity,
) -> BTreeMap<RecognizedStandardLibraryDeclarationId, ExternalSymbolKey> {
    let mut keys = BTreeMap::new();
    let mut resolving = BTreeSet::new();

    for descriptor in catalog.recognized_standard_library_declarations() {
        let _ = resolve_recognized_external_key(
            descriptor.id(),
            catalog,
            package,
            &mut keys,
            &mut resolving,
        );
    }

    keys
}

fn resolve_recognized_external_key(
    declaration: RecognizedStandardLibraryDeclarationId,
    catalog: &CompilerKnownCatalog,
    package: &PackageIdentity,
    keys: &mut BTreeMap<RecognizedStandardLibraryDeclarationId, ExternalSymbolKey>,
    resolving: &mut BTreeSet<RecognizedStandardLibraryDeclarationId>,
) -> Option<ExternalSymbolKey> {
    if let Some(key) = keys.get(&declaration) {
        // External keys are cheap shared handles and callers need an owned ancestry component.
        return Some(key.clone());
    }

    if !resolving.insert(declaration) {
        return None;
    }

    let key = (|| {
        let descriptor = catalog.recognized_standard_library_declaration(declaration)?;

        let owner = match descriptor.owner() {
            RecognizedStandardLibraryDeclarationOwner::Scope(scope) => {
                let scope = catalog.recognized_standard_library_scope(scope)?;
                let module_path = ModulePathKey::try_new(scope.path().segments())?;

                // External keys own their complete semantic ancestry, so these small immutable
                // keys must be copied into the next construction layer.
                let package_key = ExternalSymbolKey::package(package.clone());
                ExternalSymbolKey::module(package_key, module_path)?
            }
            RecognizedStandardLibraryDeclarationOwner::Declaration(owner) => {
                resolve_recognized_external_key(owner, catalog, package, keys, resolving)?
            }
        };

        let kind = declaration_symbol_kind(
            DeclarationSurfaceKind::from(descriptor.kind()),
            owner.kind(),
        );

        match descriptor.identity() {
            RecognizedStandardLibraryDeclarationIdentity::Name(name) => {
                ExternalSymbolKey::named(owner, kind, SymbolName::try_new(name.as_ref())?)
            }
            RecognizedStandardLibraryDeclarationIdentity::Ordinal(ordinal) => {
                ExternalSymbolKey::ordinal(owner, kind, SymbolOrdinal::new(*ordinal))
            }
        }
    })();

    resolving.remove(&declaration);

    let key = key?;

    // External keys recursively own semantic ancestry, so the cache retains one shared copy.
    keys.insert(declaration, key.clone());

    Some(key)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_compiler_known::{
        AvailabilityRule, RecognizedStandardLibraryDeclarationId,
        RecognizedStandardLibraryDeclarationKey,
    };

    use super::{RecognizedStandardLibraryDeclarationMatch, RecognizedStandardLibraryDeclarations};

    use crate::imported::test_support::{
        build_skeleton, interface_fixture_at_module, interface_fixture_with_lookup,
        package_identity,
    };
    use crate::{
        AnySymbolId, FunctionSymbolId, StructSymbolId, SymbolKind, SymbolOrigin, SymbolProvider,
    };

    #[test]
    fn exact_imported_identity_is_recognized_without_changing_the_symbol() {
        let fixture = interface_fixture_at_module(1, "bray.std", ["std"], "convert");
        let function_key = fixture.function_key.clone();
        let imported = Arc::new(build_skeleton([fixture.input]));
        let standard_library_package = package_identity("bray.std");

        let recognized = Arc::clone(&imported)
            .recognize_standard_library(&standard_library_package, |rule| {
                rule == AvailabilityRule::Always
            });

        let Some(AnySymbolId::Function(function)) = imported.symbol_by_external_key(&function_key)
        else {
            panic!("test function must retain its imported identity");
        };

        let Some(symbol) = SymbolProvider::<FunctionSymbolId>::symbol(imported.as_ref(), function)
        else {
            panic!("recognized function must remain an ordinary imported symbol");
        };

        assert_eq!(recognized.declarations().len(), 1);

        assert_eq!(
            recognized.descriptor(function.into()),
            Some(RecognizedStandardLibraryDeclarationId::new(0))
        );

        assert_eq!(function.kind(), SymbolKind::Function);
        assert_eq!(symbol.origin(), SymbolOrigin::Imported);
        assert_eq!(recognized.imported(), imported.as_ref());

        assert_eq!(
            recognized.declaration_symbol::<StructSymbolId>(&recognized_key("StandardConvert")),
            None
        );
    }

    #[test]
    fn selected_package_identity_excludes_same_shaped_declarations_from_other_packages() {
        let standard = interface_fixture_at_module(1, "bray.std", ["std"], "convert");
        let imitation = interface_fixture_at_module(2, "user.package", ["std"], "convert");
        let standard_key = standard.function_key.clone();
        let imitation_key = imitation.function_key.clone();
        let imported = Arc::new(build_skeleton([standard.input, imitation.input]));
        let standard_library_package = package_identity("bray.std");

        let recognized =
            Arc::clone(&imported).recognize_standard_library(&standard_library_package, |_| true);

        assert_eq!(recognized.declarations().len(), 1);

        let key = recognized_key("StandardConvert");

        let Some(symbol) = recognized.declaration_symbol::<FunctionSymbolId>(&key) else {
            panic!("selected standard-library identity must resolve");
        };

        assert_eq!(
            imported.symbol_by_external_key(&standard_key),
            Some(symbol.into())
        );

        let Some(imitation) = imported.symbol_by_external_key(&imitation_key) else {
            panic!("same-shaped user declaration must remain imported");
        };

        assert_eq!(recognized.descriptor(imitation), None);
    }

    #[test]
    fn exported_lookup_names_do_not_participate_in_recognition() {
        let fixture = interface_fixture_with_lookup(1, "bray.std", ["std"], "convert", "renamed");
        let imported = Arc::new(build_skeleton([fixture.input]));
        let standard_library_package = package_identity("bray.std");

        let recognized =
            Arc::clone(&imported).recognize_standard_library(&standard_library_package, |_| true);

        assert_eq!(recognized.declarations().len(), 1);
    }

    #[test]
    fn unavailable_recognized_declarations_are_excluded_deterministically() {
        let fixture = interface_fixture_at_module(1, "bray.std", ["std"], "convert");
        let imported = Arc::new(build_skeleton([fixture.input]));
        let standard_library_package = package_identity("bray.std");

        let unavailable =
            Arc::clone(&imported).recognize_standard_library(&standard_library_package, |_| false);

        let first =
            Arc::clone(&imported).recognize_standard_library(&standard_library_package, |_| true);

        let second =
            Arc::clone(&imported).recognize_standard_library(&standard_library_package, |_| true);

        assert!(unavailable.declarations().is_empty());
        assert_eq!(first, second);
        assert!(std::ptr::eq(first.imported(), second.imported()));
    }

    #[test]
    fn recognition_views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<RecognizedStandardLibraryDeclarations>();
        assert_send_sync::<RecognizedStandardLibraryDeclarationMatch>();
    }

    fn recognized_key(value: &str) -> RecognizedStandardLibraryDeclarationKey {
        match RecognizedStandardLibraryDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test recognized key must be valid"),
        }
    }
}
