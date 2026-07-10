use std::collections::BTreeMap;

use bray_declarations::DeclarationId;

use crate::collection::TypedSymbolRecords;
use crate::record::{
    CompilerKnownEnvironmentSymbol, ModuleSymbol, PackageSymbol, SourceSymbolIdentity,
    for_each_source_symbol,
};
use crate::{
    AnySymbolId, CompilerKnownEnvironmentSymbolId, ModuleOwnerId, ModulePathKey, ModuleSymbolId,
    PackageIdentity, PackageSymbolId, SymbolGraphBuildError, SymbolKind, SymbolRootId,
};

/// The deterministic roots of an immutable compilation-wide symbol graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolGraphRoots {
    compiler_known: CompilerKnownEnvironmentSymbolId,
    packages: Box<[PackageSymbolId]>,
}

impl SymbolGraphRoots {
    pub(crate) fn new(
        compiler_known: CompilerKnownEnvironmentSymbolId,
        packages: Box<[PackageSymbolId]>,
    ) -> Self {
        Self {
            compiler_known,
            packages,
        }
    }

    /// Returns the single compiler-known environment root.
    pub const fn compiler_known(&self) -> CompilerKnownEnvironmentSymbolId {
        self.compiler_known
    }

    /// Returns package roots in stable identity order.
    pub fn packages(&self) -> &[PackageSymbolId] {
        &self.packages
    }

    /// Iterates over every root without erasing the stored kind-specific records.
    pub fn iter(&self) -> impl Iterator<Item = SymbolRootId> + '_ {
        std::iter::once(SymbolRootId::from(self.compiler_known))
            .chain(self.packages.iter().copied().map(SymbolRootId::from))
    }
}

macro_rules! define_symbol_graph {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident;)+) => {
        /// An immutable deterministic identity graph for compilation-wide surface symbols.
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct SymbolGraph {
            roots: SymbolGraphRoots,
            compiler_known: CompilerKnownEnvironmentSymbol,
            packages: TypedSymbolRecords<PackageSymbolId, PackageSymbol>,
            modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
            module_index: BTreeMap<ModuleOwnerId, BTreeMap<ModulePathKey, ModuleSymbolId>>,
            declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            $(
                $plural: TypedSymbolRecords<crate::$id, crate::$record>,
            )+
        }

        impl SymbolGraph {
            /// Constructs the deterministic identity skeleton for one source package.
            pub fn build_source(
                package_identity: PackageIdentity,
                declarations: &bray_declarations::DeclarationTable,
            ) -> Result<Self, SymbolGraphBuildError> {
                crate::build::build_source_symbol_graph(package_identity, declarations)
            }

            /// Returns the forest roots.
            pub const fn roots(&self) -> &SymbolGraphRoots {
                &self.roots
            }

            /// Returns the single compiler-known environment record.
            pub const fn compiler_known_environment(&self) -> &CompilerKnownEnvironmentSymbol {
                &self.compiler_known
            }

            /// Returns package records in stable identity order.
            pub fn packages(&self) -> &[PackageSymbol] {
                self.packages.records()
            }

            /// Returns a package record through checked exact-ID access.
            pub fn package(&self, id: PackageSymbolId) -> Option<&PackageSymbol> {
                self.packages.get(id)
            }

            /// Returns module records in stable identity order.
            pub fn modules(&self) -> &[ModuleSymbol] {
                self.modules.records()
            }

            /// Returns a module record through checked exact-ID access.
            pub fn module(&self, id: ModuleSymbolId) -> Option<&ModuleSymbol> {
                self.modules.get(id)
            }

            /// Returns a module by exact owner and full logical path.
            pub fn module_by_path(
                &self,
                owner: ModuleOwnerId,
                path: &ModulePathKey,
            ) -> Option<&ModuleSymbol> {
                self.module(*self.module_index.get(&owner)?.get(path)?)
            }

            /// Returns the semantic identity introduced by a declaration when it creates one.
            pub fn symbol_for_declaration(&self, declaration: DeclarationId) -> Option<AnySymbolId> {
                self.declaration_index.get(&declaration).copied()
            }

            $(
                #[doc = concat!("Returns all `", stringify!($variant), "` records in stable identity order.")]
                pub fn $plural(&self) -> &[crate::$record] {
                    self.$plural.records()
                }

                #[doc = concat!("Returns a `", stringify!($variant), "` record through checked exact-ID access.")]
                pub fn $singular(&self, id: crate::$id) -> Option<&crate::$record> {
                    self.$plural.get(id)
                }
            )+
        }

        pub(crate) struct SymbolGraphBuilder {
            roots: SymbolGraphRoots,
            compiler_known: CompilerKnownEnvironmentSymbol,
            packages: Vec<PackageSymbol>,
            modules: Vec<ModuleSymbol>,
            declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            $(
                $plural: Vec<crate::$record>,
            )+
        }

        impl SymbolGraphBuilder {
            pub(crate) fn new(
                roots: SymbolGraphRoots,
                compiler_known: CompilerKnownEnvironmentSymbol,
                packages: Vec<PackageSymbol>,
                modules: Vec<ModuleSymbol>,
            ) -> Self {
                Self {
                    roots,
                    compiler_known,
                    packages,
                    modules,
                    declaration_index: BTreeMap::new(),
                    $(
                        $plural: Vec::new(),
                    )+
                }
            }

            pub(crate) fn push_source(
                &mut self,
                id: AnySymbolId,
                identity: SourceSymbolIdentity,
            ) -> Result<(), SymbolKind> {
                match id {
                    $(
                        AnySymbolId::$variant(id) => {
                            self.$plural.push(crate::$record::new(id, identity));
                            Ok(())
                        }
                    )+
                    _ => Err(id.kind()),
                }
            }

            pub(crate) fn map_declaration(
                &mut self,
                declaration: DeclarationId,
                symbol: AnySymbolId,
            ) {
                self.declaration_index.insert(declaration, symbol);
            }

            pub(crate) fn finish(self) -> SymbolGraph {
                let packages = TypedSymbolRecords::new(self.packages, PackageSymbol::id);
                let modules = TypedSymbolRecords::new(self.modules, ModuleSymbol::id);

                let module_index = modules
                    .records()
                    .iter()
                    .fold(BTreeMap::new(), |mut index, module| {
                        // Module paths share immutable segment storage with their records.
                        index
                            .entry(module.owner())
                            .or_insert_with(BTreeMap::new)
                            .insert(module.path().clone(), module.id());
                        index
                    });

                SymbolGraph {
                    roots: self.roots,
                    compiler_known: self.compiler_known,
                    packages,
                    modules,
                    module_index,
                    declaration_index: self.declaration_index,
                    $(
                        $plural: TypedSymbolRecords::new(self.$plural, crate::$record::id),
                    )+
                }
            }
        }
    };
}

for_each_source_symbol!(define_symbol_graph);

#[cfg(test)]
mod tests {
    use crate::SymbolGraph;

    #[test]
    fn graph_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolGraph>();
        assert_send_sync::<super::SymbolGraphRoots>();
    }
}
