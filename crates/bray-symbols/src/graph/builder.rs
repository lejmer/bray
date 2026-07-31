use std::collections::BTreeMap;
use std::sync::Arc;

use bray_declarations::{DeclarationId, SyntaxAnchor};

use super::{SymbolGraph, SymbolGraphRoots};
use crate::collection::TypedSymbolRecords;
use crate::record::{
    CallableParameterDefaultProviderSymbol, DeclarationSymbolIdentity, ModuleSymbol,
    PackageSymbol, ReceiverParameterSymbol, StructFieldDefaultProviderSymbol,
    UnionPayloadDefaultProviderSymbol, for_each_declaration_symbol,
};
use crate::relationship::{ModuleRelationships, RelationshipIndex};
use crate::{
    AnySymbolId, CompilerKnownSymbolProvider, MemberEntry, MemberLookupIndex, SymbolKind,
};

macro_rules! define_symbol_graph_builder {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
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

            pub(crate) fn push_inferred_parameter(
                &mut self,
                id: crate::GenericParameterSymbolId,
                identity: DeclarationSymbolIdentity,
            ) -> Result<(), SymbolKind> {
                let erased = match id {
                    crate::GenericParameterSymbolId::Type(id) => AnySymbolId::from(id),
                    crate::GenericParameterSymbolId::Const(id) => AnySymbolId::from(id),
                };

                self.relationship_index
                    .add_symbol(erased, identity.containing_symbol());

                match id {
                    crate::GenericParameterSymbolId::Type(id) => {
                        let Some(record) = crate::GenericTypeParameterSymbol::new(
                            id,
                            identity,
                            &self.relationship_index,
                        ) else {
                            return Err(erased.kind());
                        };

                        self.generic_type_parameters.push(record);
                    }
                    crate::GenericParameterSymbolId::Const(id) => {
                        let Some(record) = crate::GenericConstParameterSymbol::new(
                            id,
                            identity,
                            &self.relationship_index,
                        ) else {
                            return Err(erased.kind());
                        };

                        self.generic_const_parameters.push(record);
                    }
                }

                Ok(())
            }

            pub(crate) fn source_member_entries(
                &self,
                owner: AnySymbolId,
            ) -> &[MemberEntry<AnySymbolId>] {
                self.member_entries.get(&owner).map_or(&[], Vec::as_slice)
            }

            pub(crate) fn relationship_children(&self, owner: AnySymbolId) -> &[AnySymbolId] {
                self.relationship_index.children(owner)
            }

            pub(crate) fn compiler_known_provider(&self) -> &CompilerKnownSymbolProvider {
                self.compiler_known.as_ref()
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

            pub(crate) fn allow_member_mutation(&mut self, member: AnySymbolId) {
                self.relationship_index.allow_mutation(member);
            }

            pub(crate) fn allow_positional_member(&mut self, member: AnySymbolId) {
                self.relationship_index.allow_positional(member);
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

                let symbol_index = std::iter::once((
                    self.compiler_known.environment().key().clone(),
                    self.compiler_known.environment().id().into(),
                ))
                    .chain(
                        packages
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    .chain(
                        modules
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    .chain(
                        callable_parameter_default_providers
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    .chain(
                        struct_field_default_providers
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    .chain(
                        union_payload_default_providers
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    .chain(
                        receiver_parameters
                            .records()
                            .iter()
                            .map(|symbol| (symbol.key().clone(), symbol.id().into())),
                    )
                    $(.chain($plural.records().iter().map(|symbol| {
                        (symbol.key().clone(), symbol.id().into())
                    })))+
                    .collect();

                let module_index = modules.records().iter().fold(
                    BTreeMap::new(),
                    |mut index, module| {
                        index
                            .entry(module.owner())
                            .or_insert_with(BTreeMap::new)
                            .insert(module.path().clone(), module.id());

                        index
                    },
                );

                let module_part_index = modules
                    .records()
                    .iter()
                    .flat_map(|module| {
                        module
                            .module_parts()
                            .iter()
                            .copied()
                            .map(move |part| (part, module.id()))
                    })
                    .collect();

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
                    module_part_index,
                    member_indexes,
                    symbol_index,
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

pub(crate) enum DefaultProviderRecord {
    CallableParameter(CallableParameterDefaultProviderSymbol),
    StructField(StructFieldDefaultProviderSymbol),
    UnionPayload(UnionPayloadDefaultProviderSymbol),
}

for_each_declaration_symbol!(define_symbol_graph_builder);
