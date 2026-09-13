use bray_compiler_known::ImplementationHook;
use bray_symbols::{BorrowKind, SymbolOrdinal};

pub(crate) const fn implementation_dependency_source(
    implementation: ImplementationHook,
) -> Option<(SymbolOrdinal, Option<BorrowKind>)> {
    match implementation {
        ImplementationHook::StringUtf8
        | ImplementationHook::CallableFromPointer
        | ImplementationHook::PointerFromCallable => Some((SymbolOrdinal::new(0), None)),
        ImplementationHook::BorrowFrom => Some((SymbolOrdinal::new(0), Some(BorrowKind::Shared))),
        ImplementationHook::BorrowMutFrom => {
            Some((SymbolOrdinal::new(0), Some(BorrowKind::Mutable)))
        }
        ImplementationHook::NativeThreadStart => Some((SymbolOrdinal::new(1), None)),
        _ => None,
    }
}
