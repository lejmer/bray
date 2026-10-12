use bray_compiler_known::ImplementationHook;
use bray_symbols::{BorrowKind, DependencyReferenceKind, SymbolOrdinal};

pub(crate) const fn implementation_dependency_source(
    implementation: ImplementationHook,
) -> Option<(SymbolOrdinal, Option<BorrowKind>)> {
    match implementation {
        ImplementationHook::StringUtf8
        | ImplementationHook::CallableFromPointer
        | ImplementationHook::PointerFromCallable
        | ImplementationHook::RawPointerOffset
        | ImplementationHook::RawPointerByteOffset
        | ImplementationHook::RawPointerReinterpret => Some((SymbolOrdinal::new(0), None)),
        ImplementationHook::BorrowFrom | ImplementationHook::RawBufferInitializedSlice => {
            Some((SymbolOrdinal::new(0), Some(BorrowKind::Shared)))
        }
        ImplementationHook::BorrowMutFrom | ImplementationHook::RawBufferInitializedSliceMut => {
            Some((SymbolOrdinal::new(0), Some(BorrowKind::Mutable)))
        }
        ImplementationHook::NativeThreadStart => Some((SymbolOrdinal::new(1), None)),
        _ => None,
    }
}

/// Exact address origin, independent of the input that supplies a returned lifetime.
pub(super) const fn implementation_borrow_source(
    implementation: ImplementationHook,
) -> Option<(SymbolOrdinal, DependencyReferenceKind)> {
    match implementation {
        ImplementationHook::AddressOf
        | ImplementationHook::AddressOfMut
        | ImplementationHook::UninitPointer
        | ImplementationHook::UninitPointerMut => {
            Some((SymbolOrdinal::new(0), DependencyReferenceKind::Address))
        }
        ImplementationHook::BorrowFrom | ImplementationHook::BorrowMutFrom => {
            Some((SymbolOrdinal::new(1), DependencyReferenceKind::Borrow))
        }
        _ => None,
    }
}
