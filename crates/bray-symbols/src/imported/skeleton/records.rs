use std::collections::BTreeMap;

use crate::collection::TypedSymbolRecords;
use crate::record::{ImportedSymbolBacking, ModuleSymbolInput, for_each_source_symbol};
use crate::relationship::{ModuleRelationships, RelationshipIndex, callable_owner};
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbol, ExternalSymbolKeyData,
    ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError, ModuleOwnerId, ModuleSymbol,
    PackageSymbol, ReceiverParameterSymbol, StructFieldDefaultProviderSymbol, SymbolKey,
    SymbolOrigin, SymbolRootKey, UnionPayloadDefaultProviderSymbol,
};

use super::build::AssignedIdentity;

macro_rules! define_record_vectors {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        #[derive(Default)]
        pub(super) struct RecordVectors {
            packages: Vec<PackageSymbol>,
            modules: Vec<ModuleSymbol>,
            receiver_parameters: Vec<ReceiverParameterSymbol>,
            callable_parameter_default_providers: Vec<CallableParameterDefaultProviderSymbol>,
            struct_field_default_providers: Vec<StructFieldDefaultProviderSymbol>,
            union_payload_default_providers: Vec<UnionPayloadDefaultProviderSymbol>,
            $($plural: Vec<crate::$record>,)+
        }

        impl RecordVectors {
            fn push_declaration(
                &mut self,
                assigned: AssignedIdentity,
                container: AnySymbolId,
                index: &RelationshipIndex,
            ) -> Result<(), ImportedSymbolSkeletonBuildError> {
                let key = imported_symbol_key(&assigned.identity);
                let backing = ImportedSymbolBacking::new(assigned.interface, assigned.identity.id());

                match assigned.id {
                    $(
                        AnySymbolId::$variant(id) => {
                            let Some(record) = crate::$record::new_imported(
                                id,
                                key,
                                container,
                                backing,
                                index,
                            ) else {
                                return Err(
                                    ImportedSymbolSkeletonBuildError::InvalidRecordRelationships(
                                        assigned.id,
                                    ),
                                );
                            };

                            self.$plural.push(record);
                        }
                    )+
                    _ => {
                        return Err(ImportedSymbolSkeletonBuildError::UnsupportedSymbolKind(
                            assigned.id.kind(),
                        ));
                    }
                }

                Ok(())
            }

            pub(super) fn finish(
                self,
                external_index: BTreeMap<crate::ExternalSymbolKey, AnySymbolId>,
                lookups: BTreeMap<AnySymbolId, BTreeMap<crate::SymbolName, AnySymbolId>>,
                relationship_index: &RelationshipIndex,
            ) -> ImportedSymbolSkeleton {
                let modules = self
                    .modules
                    .into_iter()
                    .map(|module| {
                        let relationships =
                            ModuleRelationships::new(module.id().into(), relationship_index);
                        module.with_relationships(relationships)
                    })
                    .collect();

                ImportedSymbolSkeleton {
                    packages: TypedSymbolRecords::new(self.packages, PackageSymbol::id),
                    modules: TypedSymbolRecords::new(modules, ModuleSymbol::id),
                    receiver_parameters: TypedSymbolRecords::new(
                        self.receiver_parameters,
                        ReceiverParameterSymbol::id,
                    ),
                    callable_parameter_default_providers: TypedSymbolRecords::new(
                        self.callable_parameter_default_providers,
                        CallableParameterDefaultProviderSymbol::id,
                    ),
                    struct_field_default_providers: TypedSymbolRecords::new(
                        self.struct_field_default_providers,
                        StructFieldDefaultProviderSymbol::id,
                    ),
                    union_payload_default_providers: TypedSymbolRecords::new(
                        self.union_payload_default_providers,
                        UnionPayloadDefaultProviderSymbol::id,
                    ),
                    external_index,
                    lookups,
                    $($plural: TypedSymbolRecords::new(self.$plural, crate::$record::id),)+
                }
            }
        }
    };
}

for_each_source_symbol!(define_record_vectors);

macro_rules! push_default_provider {
    (
        $records:ident,
        $assigned:ident,
        $container:ident,
        $provider_subjects:ident,
        $id:ident,
        $key:ident,
        $backing:ident,
        $subject_variant:ident,
        $collection:ident,
        $record:ident
    ) => {{
        let Some(AnySymbolId::$subject_variant(subject)) =
            $provider_subjects.get(&$assigned.id).copied()
        else {
            return invalid_record($assigned.id);
        };

        let Some(container) = $container else {
            return missing_containment($assigned.id);
        };

        $records.$collection.push($record::new_imported(
            $id, $key, container, subject, $backing,
        ));
    }};
}

pub(super) fn build_records(
    assigned: Vec<AssignedIdentity>,
    containers: &BTreeMap<AnySymbolId, Option<AnySymbolId>>,
    provider_subjects: &BTreeMap<AnySymbolId, AnySymbolId>,
    relationship_index: &RelationshipIndex,
) -> Result<RecordVectors, ImportedSymbolSkeletonBuildError> {
    let mut records = RecordVectors::default();

    for assigned in assigned {
        let container = containers.get(&assigned.id).copied().flatten();
        let key = imported_symbol_key(&assigned.identity);
        let backing = ImportedSymbolBacking::new(assigned.interface, assigned.identity.id());

        match assigned.id {
            AnySymbolId::Package(id) => {
                push_package(&mut records, assigned, id, key, relationship_index)
            }
            AnySymbolId::Module(id) => {
                let Some(AnySymbolId::Package(owner)) = container else {
                    return invalid_record(assigned.id);
                };

                let ExternalSymbolKeyData::Module { path, .. } = assigned.identity.key().data()
                else {
                    return Err(ImportedSymbolSkeletonBuildError::UnsupportedSymbolKind(
                        assigned.id.kind(),
                    ));
                };

                records.modules.push(ModuleSymbol::new(ModuleSymbolInput {
                    id,
                    key,
                    owner: ModuleOwnerId::from(owner),
                    path: path.clone(),
                    origin: SymbolOrigin::Imported,
                    declarations: Box::new([]),
                    module_parts: Box::new([]),
                    is_recovered: false,
                }));
            }
            AnySymbolId::ReceiverParameter(id) => {
                let Some(owner) = container.and_then(callable_owner) else {
                    return invalid_record(assigned.id);
                };

                records
                    .receiver_parameters
                    .push(ReceiverParameterSymbol::new_imported(
                        id, key, owner, backing,
                    ));
            }
            AnySymbolId::CallableParameterDefaultProvider(id) => {
                push_default_provider!(
                    records,
                    assigned,
                    container,
                    provider_subjects,
                    id,
                    key,
                    backing,
                    CallableParameter,
                    callable_parameter_default_providers,
                    CallableParameterDefaultProviderSymbol
                );
            }
            AnySymbolId::StructFieldDefaultProvider(id) => {
                push_default_provider!(
                    records,
                    assigned,
                    container,
                    provider_subjects,
                    id,
                    key,
                    backing,
                    StructField,
                    struct_field_default_providers,
                    StructFieldDefaultProviderSymbol
                );
            }
            AnySymbolId::UnionPayloadDefaultProvider(id) => {
                push_default_provider!(
                    records,
                    assigned,
                    container,
                    provider_subjects,
                    id,
                    key,
                    backing,
                    UnionPayloadField,
                    union_payload_default_providers,
                    UnionPayloadDefaultProviderSymbol
                );
            }
            _ => {
                let Some(container) = container else {
                    return missing_containment(assigned.id);
                };

                records.push_declaration(assigned, container, relationship_index)?;
            }
        }
    }

    Ok(records)
}

fn push_package(
    records: &mut RecordVectors,
    assigned: AssignedIdentity,
    id: crate::PackageSymbolId,
    key: SymbolKey,
    relationship_index: &RelationshipIndex,
) {
    let modules = relationship_index
        .children(assigned.id)
        .iter()
        .filter_map(|member| match member {
            AnySymbolId::Module(id) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();

    records.packages.push(PackageSymbol::new(
        id,
        key,
        assigned.identity.key().package_identity().clone(),
        SymbolOrigin::Imported,
        modules,
    ));
}

fn imported_symbol_key(identity: &crate::ImportedSymbolIdentity) -> SymbolKey {
    match identity.key().data() {
        ExternalSymbolKeyData::Package(package) => SymbolKey::package(package.clone()),
        ExternalSymbolKeyData::Module { package, path } => SymbolKey::module(
            SymbolRootKey::Package(package.package_identity().clone()),
            path.clone(),
        ),
        ExternalSymbolKeyData::Declaration { .. } | ExternalSymbolKeyData::Synthesized { .. } => {
            SymbolKey::external(identity.key().clone())
        }
    }
}

fn invalid_record<T>(symbol: AnySymbolId) -> Result<T, ImportedSymbolSkeletonBuildError> {
    Err(ImportedSymbolSkeletonBuildError::InvalidRecordRelationships(symbol))
}

fn missing_containment<T>(symbol: AnySymbolId) -> Result<T, ImportedSymbolSkeletonBuildError> {
    Err(ImportedSymbolSkeletonBuildError::MissingContainment(symbol))
}
