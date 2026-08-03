use std::collections::BTreeMap;
use std::sync::Arc;

use bray_declarations::{DeclarationId, ModulePartId, SyntaxAnchor};
use bray_syntax::SyntaxTree;

use crate::collection::TypedSymbolRecords;
use crate::provider::symbol_key_from_provider;
use crate::record::{
    CallableParameterDefaultProviderSymbol, CompilerKnownEnvironmentSymbol, ModuleSymbol,
    PackageSymbol, ReceiverParameterSymbol, StructFieldDefaultProviderSymbol,
    UnionPayloadDefaultProviderSymbol, for_each_declaration_symbol,
};
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, CompilerKnownEnvironmentSymbolId,
    CompilerKnownSymbolProvider, MemberLookupIndex, MemberLookupResult, MemberVisibility,
    ModuleOwnerId, ModulePathKey, ModuleSymbolId, PackageIdentity, PackageSymbolId,
    ReceiverParameterSymbolId, StructFieldDefaultProviderSymbolId, SymbolGraphBuildError,
    SymbolProvider, SymbolRecordId, SymbolRootId, UnionPayloadDefaultProviderSymbolId,
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

    /// Iterates over every root while preserving its exact symbol category.
    pub fn iter(&self) -> impl Iterator<Item = SymbolRootId> + '_ {
        std::iter::once(SymbolRootId::from(self.compiler_known))
            .chain(self.packages.iter().copied().map(SymbolRootId::from))
    }
}

macro_rules! map_callable_symbol {
    ($graph:expr, $callable:expr, $map:expr) => {
        match $callable {
            crate::CallableSymbolId::Function(id) => $graph.function(id).map($map),
            crate::CallableSymbolId::TypeMember(id) => $graph.type_callable_member(id).map($map),
            crate::CallableSymbolId::TraitMember(id) => $graph.trait_callable_member(id).map($map),
            crate::CallableSymbolId::TraitFulfillment(id) => {
                $graph.trait_callable_fulfillment(id).map($map)
            }
            crate::CallableSymbolId::Constructor(id) => $graph.constructor(id).map($map),
            crate::CallableSymbolId::Finalizer(id) => $graph.finalizer(id).map($map),
            crate::CallableSymbolId::Destructor(id) => $graph.destructor(id).map($map),
            crate::CallableSymbolId::ScopeEnter(id) => $graph.scope_enter(id).map($map),
            crate::CallableSymbolId::ScopeExit(id) => $graph.scope_exit(id).map($map),
            crate::CallableSymbolId::TraitFinalizer(id) => {
                $graph.trait_finalizer_requirement(id).map($map)
            }
            crate::CallableSymbolId::TraitDestructor(id) => {
                $graph.trait_destructor_requirement(id).map($map)
            }
            crate::CallableSymbolId::TraitScopeEnter(id) => {
                $graph.trait_scope_enter_requirement(id).map($map)
            }
            crate::CallableSymbolId::TraitScopeExit(id) => {
                $graph.trait_scope_exit_requirement(id).map($map)
            }
            crate::CallableSymbolId::TraitScopeEnterFulfillment(id) => {
                $graph.trait_scope_enter_fulfillment(id).map($map)
            }
            crate::CallableSymbolId::TraitScopeExitFulfillment(id) => {
                $graph.trait_scope_exit_fulfillment(id).map($map)
            }
        }
    };
}

macro_rules! define_symbol_graph {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        /// An immutable deterministic identity graph for compilation-wide surface symbols.
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct SymbolGraph {
            pub(super) roots: SymbolGraphRoots,
            pub(super) next_symbol_index: usize,
            pub(super) compiler_known: Arc<CompilerKnownSymbolProvider>,
            pub(super) packages: TypedSymbolRecords<PackageSymbolId, PackageSymbol>,
            pub(super) modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
            pub(super) module_index:
                BTreeMap<ModuleOwnerId, BTreeMap<ModulePathKey, ModuleSymbolId>>,
            pub(super) module_part_index: BTreeMap<ModulePartId, ModuleSymbolId>,
            pub(super) member_indexes: BTreeMap<AnySymbolId, MemberLookupIndex<AnySymbolId>>,
            pub(super) symbol_index: BTreeMap<crate::SymbolKey, AnySymbolId>,
            pub(super) declaration_index: BTreeMap<DeclarationId, AnySymbolId>,
            pub(super) completion_children: BTreeMap<AnySymbolId, Box<[AnySymbolId]>>,
            pub(super) callable_parameter_default_providers: TypedSymbolRecords<
                CallableParameterDefaultProviderSymbolId,
                CallableParameterDefaultProviderSymbol,
            >,
            pub(super) struct_field_default_providers: TypedSymbolRecords<
                StructFieldDefaultProviderSymbolId,
                StructFieldDefaultProviderSymbol,
            >,
            pub(super) union_payload_default_providers: TypedSymbolRecords<
                UnionPayloadDefaultProviderSymbolId,
                UnionPayloadDefaultProviderSymbol,
            >,
            pub(super) receiver_parameters:
                TypedSymbolRecords<ReceiverParameterSymbolId, ReceiverParameterSymbol>,
            $(
                pub(super) $plural: TypedSymbolRecords<crate::$id, crate::$record>,
            )+
        }

        impl SymbolGraph {
            /// Constructs the deterministic identity skeleton for one source package.
            ///
            /// `declarations` and `syntax` must describe the same immutable source snapshot.
            pub fn build_source(
                package_identity: PackageIdentity,
                declarations: &bray_declarations::DeclarationTable,
                syntax: &SyntaxTree,
            ) -> Result<Self, SymbolGraphBuildError> {
                let compiler_known = Arc::new(CompilerKnownSymbolProvider::build()?);

                Self::build_source_with_provider(
                    package_identity,
                    declarations,
                    syntax,
                    compiler_known,
                )
            }

            /// Constructs one source symbol graph from a canonical compiler-known provider.
            ///
            /// `declarations` and `syntax` must describe the same immutable source snapshot.
            pub fn build_source_with_provider(
                package_identity: PackageIdentity,
                declarations: &bray_declarations::DeclarationTable,
                syntax: &SyntaxTree,
                compiler_known: Arc<CompilerKnownSymbolProvider>,
            ) -> Result<Self, SymbolGraphBuildError> {
                crate::build::build_source_symbol_graph(
                    package_identity,
                    declarations,
                    syntax,
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

            /// Returns the logical module containing one source module contribution.
            pub fn module_for_part(&self, part: ModulePartId) -> Option<&ModuleSymbol> {
                self.module(*self.module_part_index.get(&part)?)
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

            /// Returns one source or compiler-known member's ordinary name.
            pub fn member_name(&self, member: AnySymbolId) -> Option<&crate::SymbolName> {
                let owner = self.containing_symbol(member)?;

                self.member_indexes
                    .get(&owner)
                    .and_then(|index| index.name(member))
                    .or_else(|| self.compiler_known.member_name(owner, member))
                    .or_else(|| match member {
                        AnySymbolId::CallableParameter(parameter) => self
                            .callable_parameter(parameter)
                            .and_then(crate::CallableParameterSymbol::generated_name),
                        AnySymbolId::PredicateParameter(parameter) => self
                            .predicate_parameter(parameter)
                            .and_then(crate::PredicateParameterSymbol::generated_name),
                        _ => None,
                    })
            }

            /// Returns one source or compiler-known member's immutable lookup entry.
            pub fn member_entry(
                &self,
                member: AnySymbolId,
            ) -> Option<&crate::MemberEntry<AnySymbolId>> {
                let owner = self.containing_symbol(member)?;

                self.member_indexes
                    .get(&owner)
                    .and_then(|index| index.entry(member))
                    .or_else(|| self.compiler_known.member_entry(owner, member))
            }

            /// Returns the semantic identity introduced by a declaration when it creates one.
            pub fn symbol_for_declaration(&self, declaration: DeclarationId) -> Option<AnySymbolId> {
                self.declaration_index.get(&declaration).copied()
            }

            /// Returns the source syntax anchor that introduced one exact symbol.
            pub fn declaration_syntax_anchor(&self, symbol: AnySymbolId) -> Option<SyntaxAnchor> {
                match symbol {
                    $(AnySymbolId::$variant(id) => self.$singular(id)?.syntax_anchor(),)+
                    _ => None,
                }
            }

            /// Returns one exact symbol's stable semantic key.
            pub fn symbol_key(&self, symbol: AnySymbolId) -> Option<&crate::SymbolKey> {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) =>
                        SymbolProvider::<CompilerKnownEnvironmentSymbolId>::symbol(
                            self.compiler_known.as_ref(),
                            id,
                        ).map(CompilerKnownEnvironmentSymbol::key),
                    _ => symbol_key_from_provider(self, symbol),
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

            /// Returns one callable's written parameters and optional receiver.
            pub fn callable_parameters_and_receiver(
                &self,
                callable: crate::CallableSymbolId,
            ) -> Option<(
                &[crate::CallableParameterSymbolId],
                Option<ReceiverParameterSymbolId>,
            )> {
                map_callable_symbol!(self, callable, |symbol| (
                    symbol.parameters(),
                    symbol.receiver()
                ))
            }

            /// Returns how one callable entered the compilation.
            pub fn callable_origin(
                &self,
                callable: crate::CallableSymbolId,
            ) -> Option<crate::SymbolOrigin> {
                map_callable_symbol!(self, callable, |symbol| symbol.origin())
            }

            /// Returns one predicate definition's parameters in declaration order.
            pub fn predicate_definition_parameters(
                &self,
                predicate: crate::PredicateDefinitionSymbolId,
            ) -> Option<&[crate::PredicateParameterSymbolId]> {
                match predicate {
                    crate::PredicateDefinitionSymbolId::Predicate(id) => {
                        self.predicate(id).map(|symbol| symbol.parameters())
                    }
                    crate::PredicateDefinitionSymbolId::TraitMember(id) => self
                        .trait_predicate_member(id)
                        .map(|symbol| symbol.parameters()),
                    crate::PredicateDefinitionSymbolId::TraitFulfillment(id) => self
                        .trait_predicate_fulfillment(id)
                        .map(|symbol| symbol.parameters()),
                }
            }

            /// Returns whether this graph owns an exact stable symbol key.
            pub fn contains_symbol_key(&self, key: &crate::SymbolKey) -> bool {
                self.symbol_index.contains_key(key)
            }

            /// Returns the exact compilation-local symbol identified by a stable key.
            pub fn symbol_for_key(&self, key: &crate::SymbolKey) -> Option<AnySymbolId> {
                self.symbol_index.get(key).copied()
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

            /// Returns one symbol's directly owned declaration-surface children in stable order.
            pub fn declaration_children(&self, symbol: AnySymbolId) -> &[AnySymbolId] {
                self.completion_children
                    .get(&symbol)
                    .map(Box::as_ref)
                    .unwrap_or_else(|| self.compiler_known.completion_children(symbol))
            }

            pub(crate) fn completion_children(&self, symbol: AnySymbolId) -> &[AnySymbolId] {
                self.declaration_children(symbol)
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

        let syntax = bray_syntax::SyntaxTree::compilation_unit([]);

        let graph = match SymbolGraph::build_source(package, &table, &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        };

        let Some(function) = graph
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare one function");
        };

        let Some(parameter) = graph
            .callable_parameters()
            .iter()
            .find(|parameter| parameter.origin() == SymbolOrigin::Source)
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
            graph.symbol_for_key(function.key()),
            Some(function.id().into())
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

        assert_eq!(
            graph.symbol_origin(function.id().into()),
            Some(SymbolOrigin::Source)
        );

        assert_eq!(
            graph.symbol_declaration(function.id().into()),
            function.declaration()
        );

        assert_eq!(
            graph.symbols().filter(|symbol| *symbol == provider).count(),
            1
        );
    }
}
