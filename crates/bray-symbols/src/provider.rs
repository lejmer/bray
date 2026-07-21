use crate::record::{
    CallableParameterDefaultProviderSymbol, ModuleSymbol, PackageSymbol, ReceiverParameterSymbol,
    StructFieldDefaultProviderSymbol, UnionPayloadDefaultProviderSymbol,
    for_each_declaration_symbol,
};
use crate::{
    AnySymbolId, CallableParameterDefaultProviderSymbolId, ExactSymbolId, ModuleSymbolId,
    PackageSymbolId, ReceiverParameterSymbolId, StructFieldDefaultProviderSymbolId, SymbolKey,
    UnionPayloadDefaultProviderSymbolId,
};

/// Associates one exact symbol ID category with its canonical immutable record type.
///
/// The association is independent of symbol origin. Source, imported, compiler-known,
/// compiler-provided, and synthesized providers therefore expose the same record type for a
/// given exact ID category.
pub trait SymbolRecordId: ExactSymbolId {
    /// The canonical immutable record addressed by this exact ID type.
    type Record;
}

/// Provides checked read-only access to one exact category of compilation-wide symbols.
///
/// An unknown ID returns `None`. Provider implementations must support concurrent read-only
/// requests.
pub trait SymbolProvider<I>: Send + Sync
where
    I: SymbolRecordId,
{
    /// Returns the symbol record addressed by `id` when this provider owns that identity.
    fn symbol(&self, id: I) -> Option<&I::Record>;
}

macro_rules! define_symbol_key_from_provider {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        pub(crate) fn symbol_key_from_provider<P>(
            provider: &P,
            symbol: AnySymbolId,
        ) -> Option<&SymbolKey>
        where
            P: SymbolProvider<PackageSymbolId>
                + SymbolProvider<ModuleSymbolId>
                + SymbolProvider<CallableParameterDefaultProviderSymbolId>
                + SymbolProvider<StructFieldDefaultProviderSymbolId>
                + SymbolProvider<UnionPayloadDefaultProviderSymbolId>
                + SymbolProvider<ReceiverParameterSymbolId>
                $(+ SymbolProvider<crate::$id>)+
                + ?Sized,
        {
            match symbol {
                AnySymbolId::Package(id) => SymbolProvider::<PackageSymbolId>::symbol(provider, id)
                    .map(PackageSymbol::key),
                AnySymbolId::Module(id) => SymbolProvider::<ModuleSymbolId>::symbol(provider, id)
                    .map(ModuleSymbol::key),
                AnySymbolId::CallableParameterDefaultProvider(id) =>
                    SymbolProvider::<CallableParameterDefaultProviderSymbolId>::symbol(provider, id)
                        .map(CallableParameterDefaultProviderSymbol::key),
                AnySymbolId::StructFieldDefaultProvider(id) =>
                    SymbolProvider::<StructFieldDefaultProviderSymbolId>::symbol(provider, id)
                        .map(StructFieldDefaultProviderSymbol::key),
                AnySymbolId::UnionPayloadDefaultProvider(id) =>
                    SymbolProvider::<UnionPayloadDefaultProviderSymbolId>::symbol(provider, id)
                        .map(UnionPayloadDefaultProviderSymbol::key),
                AnySymbolId::ReceiverParameter(id) =>
                    SymbolProvider::<ReceiverParameterSymbolId>::symbol(provider, id)
                        .map(ReceiverParameterSymbol::key),
                $(AnySymbolId::$variant(id) =>
                    SymbolProvider::<crate::$id>::symbol(provider, id).map(crate::$record::key),)+
                AnySymbolId::CompilerKnownEnvironment(_) => None,
            }
        }
    };
}

for_each_declaration_symbol!(define_symbol_key_from_provider);
