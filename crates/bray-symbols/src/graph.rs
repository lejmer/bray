use std::collections::BTreeMap;

use bray_declarations::{DeclarationId, SyntaxAnchor};

use crate::collection::TypedSymbolRecords;
use crate::record::{
    CallableParameterDefaultProviderSymbol, CompilerKnownEnvironmentSymbol, ModuleSymbol,
    PackageSymbol, ReceiverParameterSymbol, SourceSymbolIdentity, StructFieldDefaultProviderSymbol,
    UnionPayloadDefaultProviderSymbol, for_each_source_symbol,
};
use crate::relationship::{ModuleRelationships, RelationshipIndex};
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CompilerKnownEnvironmentSymbolId,
    ModuleOwnerId, ModulePathKey, ModuleSymbolId, PackageIdentity, PackageSymbolId,
    ReceiverParameterSymbolId, StructFieldDefaultProviderSymbolId, SymbolGraphBuildError,
    SymbolKind, SymbolProvider, SymbolRecordId, SymbolRootId, UnionPayloadDefaultProviderSymbolId,
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
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        /// An immutable deterministic identity graph for compilation-wide surface symbols.
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct SymbolGraph {
            roots: SymbolGraphRoots,
            compiler_known: CompilerKnownEnvironmentSymbol,
            packages: TypedSymbolRecords<PackageSymbolId, PackageSymbol>,
            modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
            module_index: BTreeMap<ModuleOwnerId, BTreeMap<ModulePathKey, ModuleSymbolId>>,
            declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            completion_children: BTreeMap<AnySymbolId, Box<[AnySymbolId]>>,
            callable_parameter_default_providers: TypedSymbolRecords<
                CallableParameterDefaultProviderSymbolId,
                CallableParameterDefaultProviderSymbol,
            >,
            struct_field_default_providers: TypedSymbolRecords<
                StructFieldDefaultProviderSymbolId,
                StructFieldDefaultProviderSymbol,
            >,
            union_payload_default_providers: TypedSymbolRecords<
                UnionPayloadDefaultProviderSymbolId,
                UnionPayloadDefaultProviderSymbol,
            >,
            receiver_parameters:
                TypedSymbolRecords<ReceiverParameterSymbolId, ReceiverParameterSymbol>,
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

            pub(crate) fn completion_children(&self, symbol: AnySymbolId) -> &[AnySymbolId] {
                self.completion_children
                    .get(&symbol)
                    .map_or(&[], Box::as_ref)
            }

            pub(crate) fn contains_symbol(&self, symbol: AnySymbolId) -> bool {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) => self.compiler_known.id() == id,
                    AnySymbolId::Package(id) => self.packages.get(id).is_some(),
                    AnySymbolId::Module(id) => self.modules.get(id).is_some(),
                    AnySymbolId::CallableParameterDefaultProvider(id) => {
                        self.callable_parameter_default_providers.get(id).is_some()
                    }
                    AnySymbolId::StructFieldDefaultProvider(id) => {
                        self.struct_field_default_providers.get(id).is_some()
                    }
                    AnySymbolId::UnionPayloadDefaultProvider(id) => {
                        self.union_payload_default_providers.get(id).is_some()
                    }
                    AnySymbolId::ReceiverParameter(id) => {
                        self.receiver_parameters.get(id).is_some()
                    }
                    $(AnySymbolId::$variant(id) => self.$plural.get(id).is_some(),)+
                }
            }

            /// Returns synthesized receiver parameters in stable ID order.
            pub fn receiver_parameters(&self) -> &[ReceiverParameterSymbol] {
                self.receiver_parameters.records()
            }

            /// Returns a receiver parameter through checked exact-ID access.
            pub fn receiver_parameter(
                &self,
                id: ReceiverParameterSymbolId,
            ) -> Option<&ReceiverParameterSymbol> {
                self.receiver_parameters.get(id)
            }

            /// Returns synthesized callable-parameter default providers in stable ID order.
            pub fn callable_parameter_default_providers(
                &self,
            ) -> &[CallableParameterDefaultProviderSymbol] {
                self.callable_parameter_default_providers.records()
            }

            /// Returns a callable-parameter provider through checked exact-ID access.
            pub fn callable_parameter_default_provider(
                &self,
                id: CallableParameterDefaultProviderSymbolId,
            ) -> Option<&CallableParameterDefaultProviderSymbol> {
                self.callable_parameter_default_providers.get(id)
            }

            /// Returns synthesized struct-field default providers in stable ID order.
            pub fn struct_field_default_providers(&self) -> &[StructFieldDefaultProviderSymbol] {
                self.struct_field_default_providers.records()
            }

            /// Returns a struct-field provider through checked exact-ID access.
            pub fn struct_field_default_provider(
                &self,
                id: StructFieldDefaultProviderSymbolId,
            ) -> Option<&StructFieldDefaultProviderSymbol> {
                self.struct_field_default_providers.get(id)
            }

            /// Returns synthesized union-payload default providers in stable ID order.
            pub fn union_payload_default_providers(
                &self,
            ) -> &[UnionPayloadDefaultProviderSymbol] {
                self.union_payload_default_providers.records()
            }

            /// Returns a union-payload provider through checked exact-ID access.
            pub fn union_payload_default_provider(
                &self,
                id: UnionPayloadDefaultProviderSymbolId,
            ) -> Option<&UnionPayloadDefaultProviderSymbol> {
                self.union_payload_default_providers.get(id)
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

        $(
            impl SymbolRecordId for crate::$id {
                type Record = crate::$record;
            }

            impl SymbolProvider<crate::$id> for SymbolGraph {
                fn symbol(&self, id: crate::$id) -> Option<&crate::$record> {
                    self.$singular(id)
                }
            }
        )+

        pub(crate) struct SymbolGraphBuilder {
            roots: SymbolGraphRoots,
            compiler_known: CompilerKnownEnvironmentSymbol,
            packages: Vec<PackageSymbol>,
            modules: Vec<ModuleSymbol>,
            declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            pending_sources: Vec<(AnySymbolId, SourceSymbolIdentity)>,
            relationship_index: RelationshipIndex,
            callable_parameter_default_providers: Vec<CallableParameterDefaultProviderSymbol>,
            struct_field_default_providers: Vec<StructFieldDefaultProviderSymbol>,
            union_payload_default_providers: Vec<UnionPayloadDefaultProviderSymbol>,
            receiver_parameters: Vec<ReceiverParameterSymbol>,
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
                    pending_sources: Vec::new(),
                    relationship_index: RelationshipIndex::default(),
                    callable_parameter_default_providers: Vec::new(),
                    struct_field_default_providers: Vec::new(),
                    union_payload_default_providers: Vec::new(),
                    receiver_parameters: Vec::new(),
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
                if !matches!(id, $(AnySymbolId::$variant(_))|+) {
                    return Err(id.kind());
                }

                self.relationship_index
                    .add_symbol(id, identity.containing_symbol());
                self.pending_sources.push((id, identity));
                Ok(())
            }

            pub(crate) fn push_receiver(&mut self, receiver: ReceiverParameterSymbol) {
                self.relationship_index
                    .add_symbol(receiver.id().into(), receiver.owner().into_any());
                self.receiver_parameters.push(receiver);
            }

            pub(crate) fn add_runtime_default(
                &mut self,
                owner: AnySymbolId,
                syntax: SyntaxAnchor,
            ) {
                self.relationship_index.add_runtime_default(owner, syntax);
            }

            pub(crate) fn add_overload_arms(
                &mut self,
                owner: AnySymbolId,
                arms: Box<[SyntaxAnchor]>,
            ) {
                self.relationship_index.add_overload_arms(owner, arms);
            }

            pub(crate) fn push_default_provider(&mut self, provider: DefaultProviderRecord) {
                match provider {
                    DefaultProviderRecord::CallableParameter(record) => {
                        self.relationship_index
                            .add_provider(record.subject().into(), record.id().into());
                        self.callable_parameter_default_providers.push(record);
                    }
                    DefaultProviderRecord::StructField(record) => {
                        self.relationship_index
                            .add_provider(record.subject().into(), record.id().into());
                        self.struct_field_default_providers.push(record);
                    }
                    DefaultProviderRecord::UnionPayload(record) => {
                        self.relationship_index
                            .add_provider(record.subject().into(), record.id().into());
                        self.union_payload_default_providers.push(record);
                    }
                }
            }

            pub(crate) fn map_declaration(
                &mut self,
                declaration: DeclarationId,
                symbol: AnySymbolId,
            ) {
                self.declaration_index.insert(declaration, symbol);
            }

            pub(crate) fn finish(
                mut self,
            ) -> Result<SymbolGraph, (DeclarationId, SymbolKind)> {
                for (erased_id, identity) in std::mem::take(&mut self.pending_sources) {
                    let declaration = identity.declaration();
                    let kind = erased_id.kind();

                    match erased_id {
                        $(
                            AnySymbolId::$variant(id) => {
                                let Some(record) = crate::$record::new(
                                    id,
                                    identity,
                                    &self.relationship_index,
                                ) else {
                                    return Err((declaration, kind));
                                };

                                self.$plural.push(record);
                            }
                        )+
                        _ => return Err((declaration, kind)),
                    }
                }

                let packages = TypedSymbolRecords::new(self.packages, PackageSymbol::id);
                let modules = self
                    .modules
                    .into_iter()
                    .map(|module| {
                        let relationships = ModuleRelationships::new(
                            module.id().into(),
                            &self.relationship_index,
                        );
                        module.with_relationships(relationships)
                    })
                    .collect();
                let modules = TypedSymbolRecords::new(modules, ModuleSymbol::id);
                let callable_parameter_default_providers = TypedSymbolRecords::new(
                    self.callable_parameter_default_providers,
                    CallableParameterDefaultProviderSymbol::id,
                );
                let struct_field_default_providers = TypedSymbolRecords::new(
                    self.struct_field_default_providers,
                    StructFieldDefaultProviderSymbol::id,
                );
                let union_payload_default_providers = TypedSymbolRecords::new(
                    self.union_payload_default_providers,
                    UnionPayloadDefaultProviderSymbol::id,
                );
                let receiver_parameters = TypedSymbolRecords::new(
                    self.receiver_parameters,
                    ReceiverParameterSymbol::id,
                );

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

                let mut completion_children = self.relationship_index.into_completion_children();

                completion_children.insert(
                    self.compiler_known.id().into(),
                    self.compiler_known.modules().iter().copied().map(Into::into).collect(),
                );

                for package in packages.records() {
                    completion_children.insert(
                        package.id().into(),
                        package.modules().iter().copied().map(Into::into).collect(),
                    );
                }

                Ok(SymbolGraph {
                    roots: self.roots,
                    compiler_known: self.compiler_known,
                    packages,
                    modules,
                    module_index,
                    declaration_index: self.declaration_index,
                    completion_children,
                    callable_parameter_default_providers,
                    struct_field_default_providers,
                    union_payload_default_providers,
                    receiver_parameters,
                    $(
                        $plural: TypedSymbolRecords::new(self.$plural, crate::$record::id),
                    )+
                })
            }
        }
    };
}

macro_rules! impl_symbol_graph_provider {
    ($id:ty, $record:ty, $access:ident) => {
        impl SymbolRecordId for $id {
            type Record = $record;
        }

        impl SymbolProvider<$id> for SymbolGraph {
            fn symbol(&self, id: $id) -> Option<&$record> {
                self.$access(id)
            }
        }
    };
}

impl SymbolRecordId for CompilerKnownEnvironmentSymbolId {
    type Record = CompilerKnownEnvironmentSymbol;
}

impl SymbolProvider<CompilerKnownEnvironmentSymbolId> for SymbolGraph {
    fn symbol(
        &self,
        id: CompilerKnownEnvironmentSymbolId,
    ) -> Option<&CompilerKnownEnvironmentSymbol> {
        (self.compiler_known.id() == id).then_some(&self.compiler_known)
    }
}

impl_symbol_graph_provider!(PackageSymbolId, PackageSymbol, package);
impl_symbol_graph_provider!(ModuleSymbolId, ModuleSymbol, module);
impl_symbol_graph_provider!(
    ReceiverParameterSymbolId,
    ReceiverParameterSymbol,
    receiver_parameter
);
impl_symbol_graph_provider!(
    CallableParameterDefaultProviderSymbolId,
    CallableParameterDefaultProviderSymbol,
    callable_parameter_default_provider
);
impl_symbol_graph_provider!(
    StructFieldDefaultProviderSymbolId,
    StructFieldDefaultProviderSymbol,
    struct_field_default_provider
);
impl_symbol_graph_provider!(
    UnionPayloadDefaultProviderSymbolId,
    UnionPayloadDefaultProviderSymbol,
    union_payload_default_provider
);

pub(crate) enum DefaultProviderRecord {
    CallableParameter(CallableParameterDefaultProviderSymbol),
    StructField(StructFieldDefaultProviderSymbol),
    UnionPayload(UnionPayloadDefaultProviderSymbol),
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
