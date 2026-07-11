use std::collections::BTreeMap;

use crate::collection::TypedSymbolRecords;
use crate::record::for_each_declaration_symbol;
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbol, CallableParameterDefaultProviderSymbolId,
    ExternalSymbolKey, MemberLookupResult, ModuleSymbol, ModuleSymbolId, PackageSymbol,
    PackageSymbolId, ReceiverParameterSymbol, ReceiverParameterSymbolId,
    StructFieldDefaultProviderSymbol, StructFieldDefaultProviderSymbolId, SymbolName,
    SymbolProvider, UnionPayloadDefaultProviderSymbol, UnionPayloadDefaultProviderSymbolId,
};

macro_rules! define_imported_symbol_skeleton {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        /// An immutable deterministic provider for symbols reconstructed from package interfaces.
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct ImportedSymbolSkeleton {
            pub(crate) packages: TypedSymbolRecords<PackageSymbolId, PackageSymbol>,
            pub(crate) modules: TypedSymbolRecords<ModuleSymbolId, ModuleSymbol>,
            pub(crate) receiver_parameters:
                TypedSymbolRecords<ReceiverParameterSymbolId, ReceiverParameterSymbol>,
            pub(crate) callable_parameter_default_providers: TypedSymbolRecords<
                CallableParameterDefaultProviderSymbolId,
                CallableParameterDefaultProviderSymbol,
            >,
            pub(crate) struct_field_default_providers: TypedSymbolRecords<
                StructFieldDefaultProviderSymbolId,
                StructFieldDefaultProviderSymbol,
            >,
            pub(crate) union_payload_default_providers: TypedSymbolRecords<
                UnionPayloadDefaultProviderSymbolId,
                UnionPayloadDefaultProviderSymbol,
            >,
            pub(crate) external_index: BTreeMap<ExternalSymbolKey, AnySymbolId>,
            pub(crate) lookups: BTreeMap<AnySymbolId, BTreeMap<SymbolName, AnySymbolId>>,
            $(
                pub(crate) $plural: TypedSymbolRecords<crate::$id, crate::$record>,
            )+
        }

        impl ImportedSymbolSkeleton {
            /// Constructs one canonical skeleton from loaded interface surfaces.
            pub fn try_new(
                first_symbol_id: crate::SymbolId,
                inputs: impl IntoIterator<Item = super::ImportedSymbolSkeletonInput>,
            ) -> Result<Self, super::ImportedSymbolSkeletonBuildError> {
                super::build::build_imported_symbol_skeleton(first_symbol_id, inputs)
            }

            /// Returns imported package records in stable external-key order.
            pub fn packages(&self) -> &[PackageSymbol] {
                self.packages.records()
            }

            /// Returns an imported package through checked exact-ID access.
            pub fn package(&self, id: PackageSymbolId) -> Option<&PackageSymbol> {
                self.packages.get(id)
            }

            /// Returns imported module records in stable external-key order.
            pub fn modules(&self) -> &[ModuleSymbol] {
                self.modules.records()
            }

            /// Returns an imported module through checked exact-ID access.
            pub fn module(&self, id: ModuleSymbolId) -> Option<&ModuleSymbol> {
                self.modules.get(id)
            }

            /// Resolves one exact stable external identity.
            pub fn symbol_by_external_key(
                &self,
                key: &ExternalSymbolKey,
            ) -> Option<AnySymbolId> {
                self.external_index.get(key).copied()
            }

            /// Resolves an exported ordinary name from an imported package or module.
            pub fn lookup(&self, owner: AnySymbolId, name: &str) -> MemberLookupResult<AnySymbolId> {
                let Some(name) = SymbolName::try_new(name) else {
                    return MemberLookupResult::NotFound;
                };

                self.lookups
                    .get(&owner)
                    .and_then(|index| index.get(&name))
                    .copied()
                    .map_or(MemberLookupResult::NotFound, MemberLookupResult::Found)
            }

            /// Returns imported receiver parameters in stable external-key order.
            pub fn receiver_parameters(&self) -> &[ReceiverParameterSymbol] {
                self.receiver_parameters.records()
            }

            /// Returns an imported receiver parameter through checked exact-ID access.
            pub fn receiver_parameter(
                &self,
                id: ReceiverParameterSymbolId,
            ) -> Option<&ReceiverParameterSymbol> {
                self.receiver_parameters.get(id)
            }

            /// Returns imported callable-default providers in stable external-key order.
            pub fn callable_parameter_default_providers(
                &self,
            ) -> &[CallableParameterDefaultProviderSymbol] {
                self.callable_parameter_default_providers.records()
            }

            /// Returns an imported callable-default provider through checked exact-ID access.
            pub fn callable_parameter_default_provider(
                &self,
                id: CallableParameterDefaultProviderSymbolId,
            ) -> Option<&CallableParameterDefaultProviderSymbol> {
                self.callable_parameter_default_providers.get(id)
            }

            /// Returns imported struct-field default providers in stable external-key order.
            pub fn struct_field_default_providers(&self) -> &[StructFieldDefaultProviderSymbol] {
                self.struct_field_default_providers.records()
            }

            /// Returns an imported struct-field provider through checked exact-ID access.
            pub fn struct_field_default_provider(
                &self,
                id: StructFieldDefaultProviderSymbolId,
            ) -> Option<&StructFieldDefaultProviderSymbol> {
                self.struct_field_default_providers.get(id)
            }

            /// Returns imported union-payload default providers in stable external-key order.
            pub fn union_payload_default_providers(
                &self,
            ) -> &[UnionPayloadDefaultProviderSymbol] {
                self.union_payload_default_providers.records()
            }

            /// Returns an imported union-payload provider through checked exact-ID access.
            pub fn union_payload_default_provider(
                &self,
                id: UnionPayloadDefaultProviderSymbolId,
            ) -> Option<&UnionPayloadDefaultProviderSymbol> {
                self.union_payload_default_providers.get(id)
            }

            $(
                #[doc = concat!("Returns imported `", stringify!($variant), "` records in stable external-key order.")]
                pub fn $plural(&self) -> &[crate::$record] {
                    self.$plural.records()
                }

                #[doc = concat!("Returns an imported `", stringify!($variant), "` through checked exact-ID access.")]
                pub fn $singular(&self, id: crate::$id) -> Option<&crate::$record> {
                    self.$plural.get(id)
                }
            )+
        }

        $(
            impl SymbolProvider<crate::$id> for ImportedSymbolSkeleton {
                fn symbol(&self, id: crate::$id) -> Option<&crate::$record> {
                    self.$singular(id)
                }
            }
        )+
    };
}

for_each_declaration_symbol!(define_imported_symbol_skeleton);

macro_rules! impl_provider {
    ($id:ty, $record:ty, $access:ident) => {
        impl SymbolProvider<$id> for ImportedSymbolSkeleton {
            fn symbol(&self, id: $id) -> Option<&$record> {
                self.$access(id)
            }
        }
    };
}

impl_provider!(PackageSymbolId, PackageSymbol, package);
impl_provider!(ModuleSymbolId, ModuleSymbol, module);
impl_provider!(
    ReceiverParameterSymbolId,
    ReceiverParameterSymbol,
    receiver_parameter
);
impl_provider!(
    CallableParameterDefaultProviderSymbolId,
    CallableParameterDefaultProviderSymbol,
    callable_parameter_default_provider
);
impl_provider!(
    StructFieldDefaultProviderSymbolId,
    StructFieldDefaultProviderSymbol,
    struct_field_default_provider
);
impl_provider!(
    UnionPayloadDefaultProviderSymbolId,
    UnionPayloadDefaultProviderSymbol,
    union_payload_default_provider
);

#[cfg(test)]
mod tests {
    use crate::{
        AnySymbolId, FunctionSymbol, FunctionSymbolId, ImportedInterfaceId, ImportedSymbolSkeleton,
        MemberLookupResult, SymbolOrigin, SymbolProvider,
    };

    use super::super::test_support::{build_skeleton, interface_fixture};

    #[test]
    fn containment_lookup_and_origin_neutral_provider_access_are_exact() {
        let fixture = interface_fixture(3, "example.package", "run");

        let skeleton = build_skeleton([fixture.input]);

        let Some(AnySymbolId::Package(package_id)) =
            skeleton.symbol_by_external_key(&fixture.package_key)
        else {
            panic!("package key must remap to a package symbol");
        };

        let Some(AnySymbolId::Module(module_id)) =
            skeleton.symbol_by_external_key(&fixture.module_key)
        else {
            panic!("module key must remap to a module symbol");
        };

        let Some(AnySymbolId::Function(function_id)) =
            skeleton.symbol_by_external_key(&fixture.function_key)
        else {
            panic!("function key must remap to a function symbol");
        };

        let Some(package) = skeleton.package(package_id) else {
            panic!("remapped package ID must resolve");
        };

        let Some(module) = skeleton.module(module_id) else {
            panic!("remapped module ID must resolve");
        };

        let Some(function) = provided_function(&skeleton, function_id) else {
            panic!("ordinary function provider contract must resolve imported records");
        };

        assert_eq!(package.modules(), [module_id]);
        assert_eq!(module.owner(), package_id.into());
        assert_eq!(module.functions(), [function_id]);
        assert_eq!(function.containing_symbol(), module_id.into());
        assert_eq!(function.origin(), SymbolOrigin::Imported);
        assert_eq!(function.declaration(), None);

        assert_eq!(
            function.imported_fact_key().map(|key| key.interface()),
            Some(ImportedInterfaceId::new(3))
        );

        assert_eq!(
            skeleton.lookup(module_id.into(), "run"),
            MemberLookupResult::Found(function_id.into())
        );

        assert_eq!(
            skeleton.lookup(package_id.into(), "run"),
            MemberLookupResult::NotFound
        );
    }

    #[test]
    fn imported_skeletons_and_records_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImportedSymbolSkeleton>();
        assert_send_sync::<FunctionSymbol>();
    }

    fn provided_function(
        provider: &impl SymbolProvider<FunctionSymbolId>,
        id: FunctionSymbolId,
    ) -> Option<&FunctionSymbol> {
        provider.symbol(id)
    }
}
