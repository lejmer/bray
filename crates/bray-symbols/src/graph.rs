use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_declarations::{DeclarationId, SyntaxAnchor};

use crate::collection::TypedSymbolRecords;
use crate::record::{
    CallableParameterDefaultProviderSymbol, CompilerKnownEnvironmentSymbol,
    DeclarationSymbolIdentity, ModuleSymbol, PackageSymbol, ReceiverParameterSymbol,
    StructFieldDefaultProviderSymbol, UnionPayloadDefaultProviderSymbol,
    for_each_declaration_symbol,
};
use crate::relationship::{ModuleRelationships, RelationshipIndex};
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CompilerKnownEnvironmentSymbolId,
    CompilerKnownSymbolProvider, MemberEntry, MemberLookupIndex, MemberLookupResult,
    MemberVisibility, ModuleOwnerId, ModulePathKey, ModuleSymbolId, PackageIdentity,
    PackageSymbolId, ReceiverParameterSymbolId, StructFieldDefaultProviderSymbolId,
    SymbolGraphBuildError, SymbolKind, SymbolProvider, SymbolRecordId, SymbolRootId,
    UnionPayloadDefaultProviderSymbolId,
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
            next_symbol_index: usize,
            compiler_known: Arc<CompilerKnownSymbolProvider>,
            packages: TypedSymbolRecords<PackageSymbolId, PackageSymbol>,
            modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
            module_index: BTreeMap<ModuleOwnerId, BTreeMap<ModulePathKey, ModuleSymbolId>>,
            member_indexes: BTreeMap<AnySymbolId, MemberLookupIndex<AnySymbolId>>,
            symbol_keys: BTreeSet<crate::SymbolKey>,
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
                let compiler_known = Arc::new(CompilerKnownSymbolProvider::build()?);

                Self::build_source_with_provider(package_identity, declarations, compiler_known)
            }

            /// Constructs one source symbol graph from a canonical compiler-known provider.
            pub fn build_source_with_provider(
                package_identity: PackageIdentity,
                declarations: &bray_declarations::DeclarationTable,
                compiler_known: Arc<CompilerKnownSymbolProvider>,
            ) -> Result<Self, SymbolGraphBuildError> {
                crate::build::build_source_symbol_graph(
                    package_identity,
                    declarations,
                    compiler_known,
                )
            }

            /// Returns the forest roots.
            pub const fn roots(&self) -> &SymbolGraphRoots {
                &self.roots
            }

            /// Returns the first compact symbol index not occupied by this graph.
            pub const fn next_symbol_index(&self) -> usize {
                self.next_symbol_index
            }

            /// Returns the single compiler-known environment record.
            pub fn compiler_known_environment(&self) -> &CompilerKnownEnvironmentSymbol {
                self.compiler_known.environment()
            }

            /// Returns the catalog-backed compiler-known symbol and fact provider.
            pub fn compiler_known_provider(&self) -> &CompilerKnownSymbolProvider {
                self.compiler_known.as_ref()
            }

            pub(crate) fn available_compiler_known_symbols(
                &self,
                rule_is_available: impl FnMut(bray_compiler_known::AvailabilityRule) -> bool,
            ) -> crate::AvailableCompilerKnownSymbols {
                Arc::clone(&self.compiler_known).available_symbols(rule_is_available)
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

            /// Resolves one ordinary member with access to internal declarations.
            pub fn lookup_member(
                &self,
                owner: AnySymbolId,
                name: &str,
            ) -> MemberLookupResult<AnySymbolId> {
                self.lookup_member_with_access(owner, name, |_, _| true)
            }

            /// Resolves one ordinary member through caller-provided visibility policy.
            pub fn lookup_member_with_access(
                &self,
                owner: AnySymbolId,
                name: &str,
                is_accessible: impl FnMut(AnySymbolId, MemberVisibility) -> bool,
            ) -> MemberLookupResult<AnySymbolId> {
                match self.member_indexes.get(&owner) {
                    Some(index) => index.lookup_with_access(name, is_accessible),
                    None => self.compiler_known.lookup_member_with_access(
                        owner,
                        name,
                        is_accessible,
                    ),
                }
            }

            /// Returns the semantic identity introduced by a declaration when it creates one.
            pub fn symbol_for_declaration(&self, declaration: DeclarationId) -> Option<AnySymbolId> {
                self.declaration_index.get(&declaration).copied()
            }

            /// Returns one exact symbol's stable semantic key.
            pub fn symbol_key(&self, symbol: AnySymbolId) -> Option<&crate::SymbolKey> {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) =>
                        SymbolProvider::<CompilerKnownEnvironmentSymbolId>::symbol(
                            self.compiler_known.as_ref(),
                            id,
                        ).map(CompilerKnownEnvironmentSymbol::key),
                    AnySymbolId::Package(id) => self.package(id).map(PackageSymbol::key),
                    AnySymbolId::Module(id) => self.module(id).map(ModuleSymbol::key),
                    AnySymbolId::CallableParameterDefaultProvider(id) => self
                        .callable_parameter_default_provider(id)
                        .map(CallableParameterDefaultProviderSymbol::key),
                    AnySymbolId::StructFieldDefaultProvider(id) => self
                        .struct_field_default_provider(id)
                        .map(StructFieldDefaultProviderSymbol::key),
                    AnySymbolId::UnionPayloadDefaultProvider(id) => self
                        .union_payload_default_provider(id)
                        .map(UnionPayloadDefaultProviderSymbol::key),
                    AnySymbolId::ReceiverParameter(id) => {
                        self.receiver_parameter(id).map(ReceiverParameterSymbol::key)
                    }
                    $(AnySymbolId::$variant(id) => self.$singular(id).map(crate::$record::key),)+
                }
            }

            /// Returns the synthesized runtime-default provider for one defaultable symbol.
            pub fn runtime_default_provider(
                &self,
                symbol: AnySymbolId,
            ) -> Option<AnySymbolId> {
                match symbol {
                    AnySymbolId::CallableParameter(id) => {
                        self.callable_parameter(id)?.default_provider().map(Into::into)
                    }
                    AnySymbolId::StructField(id) => {
                        self.struct_field(id)?.default_provider().map(Into::into)
                    }
                    AnySymbolId::UnionPayloadField(id) => {
                        self.union_payload_field(id)?.default_provider().map(Into::into)
                    }
                    _ => None,
                }
            }

            /// Returns whether this graph owns an exact stable symbol key.
            pub fn contains_symbol_key(&self, key: &crate::SymbolKey) -> bool {
                self.symbol_keys.contains(key)
            }

            /// Returns the immediate semantic container of one symbol when it has one.
            pub fn containing_symbol(&self, symbol: AnySymbolId) -> Option<AnySymbolId> {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(_) | AnySymbolId::Package(_) => None,
                    AnySymbolId::Module(id) => {
                        self.module(id).map(|module| module.owner().into_any())
                    }
                    AnySymbolId::CallableParameterDefaultProvider(id) => self
                        .callable_parameter_default_provider(id)
                        .map(CallableParameterDefaultProviderSymbol::containing_symbol),
                    AnySymbolId::StructFieldDefaultProvider(id) => self
                        .struct_field_default_provider(id)
                        .map(StructFieldDefaultProviderSymbol::containing_symbol),
                    AnySymbolId::UnionPayloadDefaultProvider(id) => self
                        .union_payload_default_provider(id)
                        .map(UnionPayloadDefaultProviderSymbol::containing_symbol),
                    AnySymbolId::ReceiverParameter(id) => self
                        .receiver_parameter(id)
                        .map(|receiver| receiver.owner().into_any()),
                    $(AnySymbolId::$variant(id) => self.$singular(id).map(crate::$record::containing_symbol),)+
                }
            }

            /// Returns the logical module containing one declaration or synthesized symbol.
            pub fn containing_module(&self, mut symbol: AnySymbolId) -> Option<&ModuleSymbol> {
                loop {
                    if let AnySymbolId::Module(module) = symbol {
                        return self.module(module);
                    }

                    symbol = self.containing_symbol(symbol)?;
                }
            }

            /// Returns whether an exact symbol's introducing syntax contains parser recovery.
            pub fn symbol_is_recovered(&self, symbol: AnySymbolId) -> Option<bool> {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) =>
                        (self.compiler_known.environment().id() == id).then_some(false),
                    AnySymbolId::Package(id) => self.packages.get(id).map(|_| false),
                    AnySymbolId::Module(id) => self.modules.get(id).map(ModuleSymbol::is_recovered),
                    AnySymbolId::CallableParameterDefaultProvider(id) => self
                        .callable_parameter_default_providers
                        .get(id)
                        .map(|_| false),
                    AnySymbolId::StructFieldDefaultProvider(id) => self
                        .struct_field_default_providers
                        .get(id)
                        .map(|_| false),
                    AnySymbolId::UnionPayloadDefaultProvider(id) => self
                        .union_payload_default_providers
                        .get(id)
                        .map(|_| false),
                    AnySymbolId::ReceiverParameter(id) => {
                        self.receiver_parameters.get(id).map(|_| false)
                    }
                    $(AnySymbolId::$variant(id) => self.$singular(id).map(crate::$record::is_recovered),)+
                }
            }

            pub(crate) fn completion_children(&self, symbol: AnySymbolId) -> &[AnySymbolId] {
                self.completion_children
                    .get(&symbol)
                    .map(Box::as_ref)
                    .unwrap_or_else(|| self.compiler_known.completion_children(symbol))
            }

            pub(crate) fn contains_symbol(&self, symbol: AnySymbolId) -> bool {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) => {
                        self.compiler_known.environment().id() == id
                    }
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
                    self.$plural
                        .get(id)
                        .or_else(|| {
                            SymbolProvider::<crate::$id>::symbol(
                                self.compiler_known.as_ref(),
                                id,
                            )
                        })
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
            compiler_known: Arc<CompilerKnownSymbolProvider>,
            packages: Vec<PackageSymbol>,
            modules: Vec<ModuleSymbol>,
            declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            pending_sources: Vec<(AnySymbolId, DeclarationSymbolIdentity)>,
            member_entries: BTreeMap<AnySymbolId, Vec<MemberEntry<AnySymbolId>>>,
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
                compiler_known: Arc<CompilerKnownSymbolProvider>,
                packages: Vec<PackageSymbol>,
                modules: Vec<ModuleSymbol>,
            ) -> Self {
                $(let $plural = compiler_known.$plural();)+
                let receiver_parameters = compiler_known.receivers();

                Self {
                    roots,
                    compiler_known,
                    packages,
                    modules,
                    declaration_index: BTreeMap::new(),
                    pending_sources: Vec::new(),
                    member_entries: BTreeMap::new(),
                    relationship_index: RelationshipIndex::default(),
                    callable_parameter_default_providers: Vec::new(),
                    struct_field_default_providers: Vec::new(),
                    union_payload_default_providers: Vec::new(),
                    receiver_parameters,
                    $(
                        $plural,
                    )+
                }
            }

            pub(crate) fn push_source(
                &mut self,
                id: AnySymbolId,
                identity: DeclarationSymbolIdentity,
            ) -> Result<(), SymbolKind> {
                if !matches!(id, $(AnySymbolId::$variant(_))|+) {
                    return Err(id.kind());
                }

                self.relationship_index
                    .add_symbol(id, identity.containing_symbol());

                self.pending_sources.push((id, identity));

                Ok(())
            }

            pub(crate) fn add_member(
                &mut self,
                owner: AnySymbolId,
                member: MemberEntry<AnySymbolId>,
            ) {
                self.member_entries.entry(owner).or_default().push(member);
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
                next_symbol_index: usize,
            ) -> Result<SymbolGraph, (DeclarationId, SymbolKind)> {
                for (erased_id, identity) in std::mem::take(&mut self.pending_sources) {
                    let Some(declaration) = identity.source_declaration() else {
                        continue;
                    };

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

                $(
                    let $plural = TypedSymbolRecords::new(self.$plural, crate::$record::id);
                )+

                // Stable keys are Arc-backed and cheap to retain in the ownership-validation index.
                let symbol_keys = std::iter::once(self.compiler_known.environment().key())
                    .chain(packages.records().iter().map(PackageSymbol::key))
                    .chain(modules.records().iter().map(ModuleSymbol::key))
                    .chain(
                        callable_parameter_default_providers
                            .records()
                            .iter()
                            .map(CallableParameterDefaultProviderSymbol::key),
                    )
                    .chain(
                        struct_field_default_providers
                            .records()
                            .iter()
                            .map(StructFieldDefaultProviderSymbol::key),
                    )
                    .chain(
                        union_payload_default_providers
                            .records()
                            .iter()
                            .map(UnionPayloadDefaultProviderSymbol::key),
                    )
                    .chain(
                        receiver_parameters
                            .records()
                            .iter()
                            .map(ReceiverParameterSymbol::key),
                    )
                    $(.chain($plural.records().iter().map(crate::$record::key)))+
                    .cloned()
                    .collect();

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

                let member_indexes = self
                    .member_entries
                    .into_iter()
                    .map(|(owner, entries)| {
                        let index = match MemberLookupIndex::new(entries) {
                            Ok(index) => index,
                            Err(_) => {
                                panic!("symbol graph member entries must have distinct identities")
                            }
                        };

                        (owner, index)
                    })
                    .collect();

                let mut completion_children = self.relationship_index.into_completion_children();

                for package in packages.records() {
                    completion_children.insert(
                        package.id().into(),
                        package.modules().iter().copied().map(Into::into).collect(),
                    );
                }

                Ok(SymbolGraph {
                    roots: self.roots,
                    next_symbol_index,
                    compiler_known: self.compiler_known,
                    packages,
                    modules,
                    module_index,
                    member_indexes,
                    symbol_keys,
                    declaration_index: self.declaration_index,
                    completion_children,
                    callable_parameter_default_providers,
                    struct_field_default_providers,
                    union_payload_default_providers,
                    receiver_parameters,
                    $($plural,)+
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
        SymbolProvider::<CompilerKnownEnvironmentSymbolId>::symbol(self.compiler_known.as_ref(), id)
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

for_each_declaration_symbol!(define_symbol_graph);

#[cfg(test)]
mod tests {
    use crate::{AnySymbolId, PackageIdentity, SymbolGraph, SymbolOrigin};

    #[test]
    fn graph_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolGraph>();
        assert_send_sync::<super::SymbolGraphRoots>();
    }

    #[test]
    fn erased_symbol_access_resolves_keys_owners_and_runtime_default_providers() {
        let table = crate::test_support::declaration_table(&[concat!(
            "module app;\n",
            "func main(value: i32 = 1)\n",
            "{\n",
            "}\n",
        )]);

        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity must be valid");
        };

        let graph = match SymbolGraph::build_source(package, &table) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let Some(function) = graph
            .functions()
            .iter()
            .find(|function| function.origin() == crate::SymbolOrigin::Source)
        else {
            panic!("test source must declare one function");
        };

        let Some(parameter) = graph
            .callable_parameters()
            .iter()
            .find(|parameter| parameter.origin() == crate::SymbolOrigin::Source)
        else {
            panic!("test source must declare one callable parameter");
        };

        let Some(source_module) = graph
            .modules()
            .iter()
            .find(|module| module.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must produce one source module");
        };

        let Some(compiler_known_module) = graph
            .modules()
            .iter()
            .find(|module| module.origin() == SymbolOrigin::CompilerKnown)
        else {
            panic!("generated catalog must produce a compiler-known module");
        };

        let Some(provider_id) = parameter.default_provider() else {
            panic!("defaulted parameter must retain its provider ID");
        };

        let Some(provider) = graph.runtime_default_provider(parameter.id().into()) else {
            panic!("defaulted parameter must have a runtime-default provider");
        };

        assert_eq!(
            graph.symbol_key(AnySymbolId::from(function.id())),
            Some(function.key())
        );

        assert_eq!(
            graph.containing_symbol(source_module.id().into()),
            Some(source_module.owner().into_any())
        );
        assert_eq!(
            graph.containing_symbol(compiler_known_module.id().into()),
            Some(compiler_known_module.owner().into_any())
        );

        assert_eq!(provider, provider_id.into());
        assert_eq!(
            graph.symbol_key(provider),
            graph
                .callable_parameter_default_provider(provider_id)
                .map(crate::CallableParameterDefaultProviderSymbol::key)
        );
    }
}
