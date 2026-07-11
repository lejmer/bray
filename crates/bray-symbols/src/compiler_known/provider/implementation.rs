use std::collections::BTreeMap;

use bray_compiler_known::{
    COMPILER_KNOWN_CATALOG, CatalogScopeLocation, CompilerKnownCatalog,
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId, CompilerKnownDeclarationKey,
    CompilerKnownDeclarationOwner, CompilerKnownScopeKey,
};

use super::super::{
    CompilerKnownDeclarationFact, CompilerKnownSymbolBuildError, CompilerKnownSymbolFactKey,
};
use super::validation::{resolve_declaration_symbol_kinds, validate_scope_id};
use crate::allocator::SymbolIdAllocator;
use crate::build::declaration_symbol_id;
use crate::collection::TypedSymbolRecords;
use crate::record::{
    CompilerKnownEnvironmentSymbol, DeclarationSymbolIdentity, ModuleSymbol, ModuleSymbolInput,
    for_each_declaration_symbol,
};
use crate::relationship::{ModuleRelationships, RelationshipIndex};
use crate::{
    AnySymbolId, CompilerKnownEnvironmentSymbolId, ExactSymbolId, ModuleOwnerId, ModulePathKey,
    ModuleSymbolId, SymbolId, SymbolKey, SymbolKind, SymbolOrigin, SymbolProvider, SymbolRootKey,
};

/// The symbol identity materialized for one compiler-known scope descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilerKnownScopeSymbolId {
    /// The ambient scope is the compiler-known environment root itself.
    Environment(CompilerKnownEnvironmentSymbolId),
    /// A path scope is an ordinary compiler-known module.
    Module(ModuleSymbolId),
}

impl CompilerKnownScopeSymbolId {
    fn into_any(self) -> AnySymbolId {
        match self {
            Self::Environment(id) => id.into(),
            Self::Module(id) => id.into(),
        }
    }
}

macro_rules! define_compiler_known_records {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        enum CompilerKnownRecord {
            $($variant(crate::$record),)+
        }

        impl CompilerKnownRecord {
            fn new(
                id: AnySymbolId,
                identity: DeclarationSymbolIdentity,
                relationships: &RelationshipIndex,
            ) -> Option<Self> {
                match id {
                    $(
                        AnySymbolId::$variant(id) => {
                            crate::$record::new(id, identity, relationships).map(Self::$variant)
                        }
                    )+
                    _ => None,
                }
            }
        }

        $(
            impl CompilerKnownSymbolProvider {
                pub(crate) fn $plural(&self) -> Vec<crate::$record> {
                    self.records
                        .values()
                        .filter_map(|record| match record {
                            CompilerKnownRecord::$variant(record) => Some(record.clone()),
                            _ => None,
                        })
                        .collect()
                }
            }

            impl SymbolProvider<crate::$id> for CompilerKnownSymbolProvider {
                fn symbol(&self, id: crate::$id) -> Option<&crate::$record> {
                    match self.records.get(&id.symbol_id())? {
                        CompilerKnownRecord::$variant(record) => Some(record),
                        _ => None,
                    }
                }
            }
        )+
    };
}

for_each_declaration_symbol!(define_compiler_known_records);

/// Immutable compilation-local provider for generated compiler-known symbols and facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownSymbolProvider {
    catalog: &'static CompilerKnownCatalog,
    environment: CompilerKnownEnvironmentSymbol,
    modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
    scope_symbols: BTreeMap<CompilerKnownScopeKey, CompilerKnownScopeSymbolId>,
    declaration_symbols: BTreeMap<CompilerKnownDeclarationKey, AnySymbolId>,
    declaration_descriptors: BTreeMap<SymbolId, CompilerKnownDeclarationId>,
    records: BTreeMap<SymbolId, CompilerKnownRecord>,
    next_symbol_index: usize,
}

type DeclarationSymbolMaps = (
    BTreeMap<CompilerKnownDeclarationKey, AnySymbolId>,
    BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
);

impl CompilerKnownSymbolProvider {
    /// Constructs one compilation-local provider from checked-in generated catalog tables.
    pub fn build() -> Result<Self, CompilerKnownSymbolBuildError> {
        Self::build_from_catalog(&COMPILER_KNOWN_CATALOG)
    }

    fn build_from_catalog(
        catalog: &'static CompilerKnownCatalog,
    ) -> Result<Self, CompilerKnownSymbolBuildError> {
        let mut allocator = SymbolIdAllocator::new();

        let environment_id = CompilerKnownEnvironmentSymbolId::from_symbol_id(allocator.next()?);
        let (scope_symbols, modules) = build_scopes(catalog, environment_id, &mut allocator)?;
        let declaration_kinds = resolve_declaration_symbol_kinds(catalog)?;

        let (declaration_symbols, descriptor_symbols) =
            allocate_declarations(catalog, &declaration_kinds, &mut allocator)?;

        let (records, relationships) = build_declarations(
            catalog,
            &scope_symbols,
            &descriptor_symbols,
            &declaration_symbols,
        )?;

        let modules = modules
            .into_iter()
            .map(|module| {
                let module_relationships =
                    ModuleRelationships::new(module.id().into(), &relationships);

                module.with_relationships(module_relationships)
            })
            .collect();

        let modules = TypedSymbolRecords::new(modules, ModuleSymbol::id);
        let module_ids = modules.records().iter().map(ModuleSymbol::id).collect();

        let environment = CompilerKnownEnvironmentSymbol::new(
            environment_id,
            SymbolKey::compiler_known_environment(),
            module_ids,
            ModuleRelationships::new(environment_id.into(), &relationships),
        );

        Ok(Self {
            catalog,
            environment,
            modules,
            scope_symbols,
            declaration_symbols,
            declaration_descriptors: descriptor_symbols
                .iter()
                .map(|(descriptor, symbol)| (symbol.symbol_id(), *descriptor))
                .collect(),
            records,
            next_symbol_index: allocator.next_index(),
        })
    }

    /// Returns the dedicated compiler-known environment root.
    pub const fn environment(&self) -> &CompilerKnownEnvironmentSymbol {
        &self.environment
    }

    /// Returns compiler-known module records in canonical scope order.
    pub fn modules(&self) -> &[ModuleSymbol] {
        self.modules.records()
    }

    /// Returns the symbol represented by a stable compiler-known scope key.
    pub fn scope_symbol(&self, key: &CompilerKnownScopeKey) -> Option<CompilerKnownScopeSymbolId> {
        self.scope_symbols.get(key).copied()
    }

    /// Returns the complete stable scope-key map for compilation infrastructure.
    pub const fn scope_symbols(
        &self,
    ) -> &BTreeMap<CompilerKnownScopeKey, CompilerKnownScopeSymbolId> {
        &self.scope_symbols
    }

    /// Returns the exact ordinary symbol identity represented by a stable declaration key.
    pub fn declaration_symbol<I: ExactSymbolId>(
        &self,
        key: &CompilerKnownDeclarationKey,
    ) -> Option<I> {
        I::try_from_any(*self.declaration_symbols.get(key)?)
    }

    pub(in crate::compiler_known) fn untyped_declaration_symbol(
        &self,
        key: &CompilerKnownDeclarationKey,
    ) -> Option<AnySymbolId> {
        self.declaration_symbols.get(key).copied()
    }

    /// Returns the complete stable declaration-key map for compilation infrastructure.
    pub const fn declaration_symbols(&self) -> &BTreeMap<CompilerKnownDeclarationKey, AnySymbolId> {
        &self.declaration_symbols
    }

    /// Creates a typed route to lazy facts when a key has the requested ordinary symbol kind.
    pub fn fact_key<I: ExactSymbolId>(
        &self,
        key: &CompilerKnownDeclarationKey,
    ) -> Option<CompilerKnownSymbolFactKey<I>> {
        let symbol = self.declaration_symbol::<I>(key)?;
        let declaration = *self.declaration_descriptors.get(&symbol.symbol_id())?;

        Some(CompilerKnownSymbolFactKey::new(declaration))
    }

    /// Resolves immutable descriptor-backed facts for one validated typed request.
    pub fn declaration_fact<I: ExactSymbolId>(
        &self,
        key: CompilerKnownSymbolFactKey<I>,
    ) -> Option<CompilerKnownDeclarationFact<'static>> {
        let descriptor = self.catalog.compiler_known_declaration(key.declaration())?;
        let symbol = self.declaration_symbols.get(descriptor.key())?;

        if symbol.kind() != I::KIND {
            return None;
        }

        let surface = self.catalog.declaration_surface(descriptor.surface())?;

        Some(CompilerKnownDeclarationFact::new(descriptor, surface))
    }

    pub(crate) const fn next_symbol_index(&self) -> usize {
        self.next_symbol_index
    }

    pub(in crate::compiler_known) const fn catalog(&self) -> &'static CompilerKnownCatalog {
        self.catalog
    }
}

impl SymbolProvider<CompilerKnownEnvironmentSymbolId> for CompilerKnownSymbolProvider {
    fn symbol(
        &self,
        id: CompilerKnownEnvironmentSymbolId,
    ) -> Option<&CompilerKnownEnvironmentSymbol> {
        (self.environment.id() == id).then_some(&self.environment)
    }
}

impl SymbolProvider<ModuleSymbolId> for CompilerKnownSymbolProvider {
    fn symbol(&self, id: ModuleSymbolId) -> Option<&ModuleSymbol> {
        self.modules.get(id)
    }
}

fn build_scopes(
    catalog: &CompilerKnownCatalog,
    environment: CompilerKnownEnvironmentSymbolId,
    allocator: &mut SymbolIdAllocator,
) -> Result<
    (
        BTreeMap<CompilerKnownScopeKey, CompilerKnownScopeSymbolId>,
        Vec<ModuleSymbol>,
    ),
    CompilerKnownSymbolBuildError,
> {
    let mut scope_symbols = BTreeMap::new();
    let mut modules = Vec::new();

    let mut has_ambient = false;

    for (index, scope) in catalog.compiler_known_scopes().iter().enumerate() {
        validate_scope_id(index, scope.id())?;

        let symbol = match scope.location() {
            CatalogScopeLocation::Ambient => {
                if has_ambient {
                    return Err(CompilerKnownSymbolBuildError::DuplicateAmbientScope {
                        duplicate: scope.id(),
                    });
                }

                has_ambient = true;
                CompilerKnownScopeSymbolId::Environment(environment)
            }
            CatalogScopeLocation::Module(path) => {
                let Some(path) = ModulePathKey::try_new(path.segments()) else {
                    return Err(CompilerKnownSymbolBuildError::InvalidModulePath {
                        scope: scope.id(),
                    });
                };

                let id = ModuleSymbolId::from_symbol_id(allocator.next()?);
                let key = SymbolKey::module(SymbolRootKey::CompilerKnownEnvironment, path.clone());

                modules.push(ModuleSymbol::new(ModuleSymbolInput {
                    id,
                    key,
                    owner: ModuleOwnerId::from(environment),
                    path,
                    origin: SymbolOrigin::CompilerKnown,
                    declarations: Box::new([]),
                    module_parts: Box::new([]),
                    is_recovered: false,
                }));

                CompilerKnownScopeSymbolId::Module(id)
            }
        };

        scope_symbols.insert(scope.key().clone(), symbol);
    }

    if !has_ambient {
        return Err(CompilerKnownSymbolBuildError::MissingAmbientScope);
    }

    Ok((scope_symbols, modules))
}

fn allocate_declarations(
    catalog: &CompilerKnownCatalog,
    declaration_kinds: &BTreeMap<CompilerKnownDeclarationId, SymbolKind>,
    allocator: &mut SymbolIdAllocator,
) -> Result<DeclarationSymbolMaps, CompilerKnownSymbolBuildError> {
    let mut stable_symbols = BTreeMap::new();
    let mut descriptor_symbols = BTreeMap::new();

    for descriptor in catalog.compiler_known_declarations() {
        let Some(kind) = declaration_kinds.get(&descriptor.id()).copied() else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let raw_id = allocator.next()?;
        let Some(symbol) = declaration_symbol_id(raw_id, kind) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        stable_symbols.insert(descriptor.key().clone(), symbol);
        descriptor_symbols.insert(descriptor.id(), symbol);
    }

    Ok((stable_symbols, descriptor_symbols))
}

fn build_declarations(
    catalog: &CompilerKnownCatalog,
    scopes: &BTreeMap<CompilerKnownScopeKey, CompilerKnownScopeSymbolId>,
    descriptor_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
    stable_symbols: &BTreeMap<CompilerKnownDeclarationKey, AnySymbolId>,
) -> Result<
    (BTreeMap<SymbolId, CompilerKnownRecord>, RelationshipIndex),
    CompilerKnownSymbolBuildError,
> {
    let mut relationships = RelationshipIndex::default();
    let mut identities = Vec::new();

    for descriptor in catalog.compiler_known_declarations() {
        let Some(symbol) = descriptor_symbols.get(&descriptor.id()).copied() else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let owner = declaration_owner(catalog, descriptor, scopes, descriptor_symbols)?;

        let origin = if descriptor.implementation_hook().is_some() {
            SymbolOrigin::CompilerProvided
        } else {
            SymbolOrigin::CompilerKnown
        };

        let Some(key) =
            SymbolKey::compiler_known_declaration(descriptor.key().clone(), symbol.kind())
        else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationKind {
                declaration: descriptor.id(),
                catalog_kind: descriptor.kind(),
                owner_kind: owner.kind(),
            });
        };

        let Some(surface) = catalog.declaration_surface(descriptor.surface()) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        if surface.kind() != descriptor.kind()
            || stable_symbols.get(descriptor.key()) != Some(&symbol)
        {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        }

        relationships.add_symbol(symbol, owner);

        identities.push((
            descriptor.id(),
            symbol,
            DeclarationSymbolIdentity::compiler_known(
                key,
                owner,
                descriptor.id(),
                descriptor.surface(),
                origin,
            ),
        ));
    }

    let mut records = BTreeMap::new();

    for (declaration, symbol, identity) in identities {
        let Some(record) = CompilerKnownRecord::new(symbol, identity, &relationships) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface { declaration });
        };

        records.insert(symbol.symbol_id(), record);
    }

    Ok((records, relationships))
}

fn declaration_owner(
    catalog: &CompilerKnownCatalog,
    descriptor: &CompilerKnownDeclarationDescriptor,
    scopes: &BTreeMap<CompilerKnownScopeKey, CompilerKnownScopeSymbolId>,
    declarations: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
) -> Result<AnySymbolId, CompilerKnownSymbolBuildError> {
    match descriptor.owner() {
        CompilerKnownDeclarationOwner::Scope(scope_id) => {
            let Some(scope) = catalog.compiler_known_scope(scope_id) else {
                return Err(CompilerKnownSymbolBuildError::MissingScopeOwner {
                    declaration: descriptor.id(),
                    scope: scope_id,
                });
            };

            let Some(symbol) = scopes.get(scope.key()) else {
                return Err(CompilerKnownSymbolBuildError::MissingScopeOwner {
                    declaration: descriptor.id(),
                    scope: scope_id,
                });
            };

            Ok(symbol.into_any())
        }
        CompilerKnownDeclarationOwner::Declaration(owner) => declarations
            .get(&owner)
            .copied()
            .ok_or(CompilerKnownSymbolBuildError::MissingDeclarationOwner {
                declaration: descriptor.id(),
                owner,
            }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::compiler_known::test_support::{
        build_provider, declaration_key, scope_key, struct_id,
    };
    use crate::{
        CompilerKnownScopeSymbolId, FunctionSymbolId, StructSymbolId, SymbolKind, SymbolOrigin,
        SymbolProvider,
    };

    #[test]
    fn generated_catalog_builds_deterministic_typed_symbols() {
        let first = build_provider();
        let second = build_provider();

        assert_eq!(first, second);
        assert_eq!(first.environment().id().symbol_id().raw(), 0);
        assert_eq!(first.modules().len(), 2);

        let bool_key = declaration_key("Bool");

        let Some(bool_id) = first.declaration_symbol::<StructSymbolId>(&bool_key) else {
            panic!("Bool must have a compiler-known symbol");
        };

        assert_eq!(bool_id.kind(), SymbolKind::Struct);
        assert_eq!(bool_id.symbol_id().raw(), 3);
    }

    #[test]
    fn stable_scope_and_declaration_keys_recover_only_exact_categories() {
        let provider = build_provider();
        let ambient = scope_key("Ambient");

        assert_eq!(
            provider.scope_symbol(&ambient),
            Some(CompilerKnownScopeSymbolId::Environment(
                provider.environment().id()
            ))
        );

        assert_eq!(provider.scope_symbol(&scope_key("Missing")), None);
        assert_eq!(provider.scope_symbols().len(), 3);

        let bool_key = declaration_key("Bool");

        assert!(provider.fact_key::<StructSymbolId>(&bool_key).is_some());
        assert_eq!(provider.fact_key::<FunctionSymbolId>(&bool_key), None);

        assert_eq!(
            provider.declaration_symbol::<StructSymbolId>(&declaration_key("Missing")),
            None
        );

        assert_eq!(provider.declaration_symbols().len(), 12);
    }

    #[test]
    fn ordinary_records_retain_catalog_origin_and_typed_containment() {
        let provider = build_provider();

        let bool_key = declaration_key("Bool");
        let raw_pointer_key = declaration_key("RawPointer");
        let memory_copy_key = declaration_key("MemoryCopy");

        let bool_id = struct_id(&provider, &bool_key);
        let raw_pointer_id = struct_id(&provider, &raw_pointer_key);

        let Some(bool_symbol) = SymbolProvider::<StructSymbolId>::symbol(&provider, bool_id) else {
            panic!("Bool struct record must resolve");
        };

        let Some(raw_pointer) = SymbolProvider::<StructSymbolId>::symbol(&provider, raw_pointer_id)
        else {
            panic!("RawPointer struct record must resolve");
        };

        assert_eq!(bool_symbol.origin(), SymbolOrigin::CompilerKnown);
        assert_eq!(raw_pointer.fields().len(), 1);
        assert_eq!(bool_symbol.declaration(), None);

        assert!(bool_symbol.compiler_known_declaration().is_some());
        assert!(bool_symbol.compiler_known_surface().is_some());
        assert!(provider.environment().structures().contains(&bool_id));

        assert_eq!(
            provider.environment().modules(),
            [provider.modules()[0].id(), provider.modules()[1].id()]
        );

        let Some(function_id) = provider.declaration_symbol::<FunctionSymbolId>(&memory_copy_key)
        else {
            panic!("MemoryCopy must have a symbol");
        };

        let Some(memory_copy) = SymbolProvider::<FunctionSymbolId>::symbol(&provider, function_id)
        else {
            panic!("MemoryCopy function record must resolve");
        };

        assert_eq!(memory_copy.origin(), SymbolOrigin::CompilerProvided);
    }

    #[test]
    fn concurrent_provider_construction_and_fact_reads_are_deterministic() {
        let expected = Arc::new(build_provider());
        let workers = (0..4)
            .map(|_| {
                let expected = Arc::clone(&expected);

                std::thread::spawn(move || {
                    let actual = build_provider();

                    assert_eq!(actual, *expected);

                    let key = declaration_key("Bool");

                    let Some(fact_key) = actual.fact_key::<StructSymbolId>(&key) else {
                        panic!("Bool fact key must resolve concurrently");
                    };

                    let Some(fact) = actual.declaration_fact(fact_key) else {
                        panic!("Bool facts must resolve concurrently");
                    };

                    assert_eq!(fact.descriptor().key(), &key);
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            if let Err(error) = worker.join() {
                panic!("parallel compiler-known provider failed: {error:?}");
            }
        }
    }
}
