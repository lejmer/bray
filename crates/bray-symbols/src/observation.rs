use bray_declarations::DeclarationId;

use crate::record::for_each_declaration_symbol;
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbol, MemberEntry, MemberVisibility,
    ModuleSymbol, PackageSymbol, ReceiverParameterSymbol, StructFieldDefaultProviderSymbol,
    SymbolGraph, SymbolOrigin, UnionPayloadDefaultProviderSymbol,
};

macro_rules! define_symbol_observations {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        impl SymbolGraph {
            /// Iterates over every compilation-wide symbol in deterministic category and ID order.
            ///
            /// This flat tooling view does not imply semantic parent-child relationships.
            pub fn symbols(&self) -> impl Iterator<Item = AnySymbolId> + '_ {
                std::iter::once(self.compiler_known_environment().id().into())
                    .chain(self.packages().iter().map(|symbol| symbol.id().into()))
                    .chain(self.modules().iter().map(|symbol| symbol.id().into()))
                    $(
                        .chain(self.$plural().iter().map(|symbol| symbol.id().into()))
                    )+
                    .chain(
                        self.receiver_parameters()
                            .iter()
                            .map(|symbol| symbol.id().into()),
                    )
                    .chain(
                        self.callable_parameter_default_providers()
                            .iter()
                            .map(|symbol| symbol.id().into()),
                    )
                    .chain(
                        self.struct_field_default_providers()
                            .iter()
                            .map(|symbol| symbol.id().into()),
                    )
                    .chain(
                        self.union_payload_default_providers()
                            .iter()
                            .map(|symbol| symbol.id().into()),
                    )
            }

            /// Returns the source declaration that introduced one exact symbol.
            pub fn symbol_declaration(&self, symbol: AnySymbolId) -> Option<DeclarationId> {
                match symbol {
                    $(AnySymbolId::$variant(id) => self.$singular(id)?.declaration(),)+
                    _ => None,
                }
            }

            /// Returns how one exact symbol entered this compilation.
            pub fn symbol_origin(&self, symbol: AnySymbolId) -> Option<SymbolOrigin> {
                match symbol {
                    AnySymbolId::CompilerKnownEnvironment(id) =>
                        (self.compiler_known_environment().id() == id)
                            .then_some(SymbolOrigin::CompilerKnown),
                    AnySymbolId::Package(id) => self.package(id).map(PackageSymbol::origin),
                    AnySymbolId::Module(id) => self.module(id).map(ModuleSymbol::origin),
                    AnySymbolId::CallableParameterDefaultProvider(id) => self
                        .callable_parameter_default_provider(id)
                        .map(CallableParameterDefaultProviderSymbol::origin),
                    AnySymbolId::StructFieldDefaultProvider(id) => self
                        .struct_field_default_provider(id)
                        .map(StructFieldDefaultProviderSymbol::origin),
                    AnySymbolId::UnionPayloadDefaultProvider(id) => self
                        .union_payload_default_provider(id)
                        .map(UnionPayloadDefaultProviderSymbol::origin),
                    AnySymbolId::ReceiverParameter(id) => {
                        self.receiver_parameter(id).map(ReceiverParameterSymbol::origin)
                    }
                    $(AnySymbolId::$variant(id) => self.$singular(id).map(crate::$record::origin),)+
                }
            }
        }
    };
}

for_each_declaration_symbol!(define_symbol_observations);

impl SymbolGraph {
    /// Returns declaration-level visibility when the symbol has one.
    pub fn symbol_visibility(&self, symbol: AnySymbolId) -> Option<MemberVisibility> {
        match symbol {
            AnySymbolId::Module(id) => self.module(id).map(ModuleSymbol::visibility),
            AnySymbolId::CompilerKnownEnvironment(_) | AnySymbolId::Package(_) => None,
            _ => self.member_entry(symbol).map(MemberEntry::visibility),
        }
    }
}
