use bray_compiler_known::ImplementationHook;
use bray_symbols::{BorrowKind, DependencyReferenceKind, DependencySubjectRoot, SymbolOrdinal};

pub(crate) const fn implementation_dependency_source(
    implementation: ImplementationHook,
) -> Option<(DependencySubjectRoot, Option<BorrowKind>)> {
    match implementation {
        ImplementationHook::StringUtf8
        | ImplementationHook::CallableFromPointer
        | ImplementationHook::PointerFromCallable
        | ImplementationHook::RawPointerOffset
        | ImplementationHook::RawPointerByteOffset
        | ImplementationHook::RawPointerReinterpret => Some((
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            None,
        )),
        ImplementationHook::BorrowFrom | ImplementationHook::RawBufferInitializedSlice => Some((
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            Some(BorrowKind::Shared),
        )),
        ImplementationHook::BorrowMutFrom | ImplementationHook::RawBufferInitializedSliceMut => {
            Some((
                DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
                Some(BorrowKind::Mutable),
            ))
        }
        ImplementationHook::NativeThreadStart => Some((
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(1)),
            None,
        )),
        ImplementationHook::FutureStart => Some((DependencySubjectRoot::Receiver, None)),
        _ => None,
    }
}

/// Exact address origin, independent of the input that supplies a returned lifetime.
pub(super) fn implementation_borrow(
    implementation: ImplementationHook,
) -> Option<bray_symbols::DependencyBorrowRelation> {
    let (parameter, kind, projections): (_, _, &[bray_symbols::DependencyProjection]) =
        match implementation {
            ImplementationHook::AddressOf | ImplementationHook::AddressOfMut => {
                (0, DependencyReferenceKind::Address, &[])
            }
            ImplementationHook::UninitPointer | ImplementationHook::UninitPointerMut => (
                0,
                DependencyReferenceKind::Address,
                &[bray_symbols::DependencyProjection::UninitPayload],
            ),
            ImplementationHook::BorrowFrom | ImplementationHook::BorrowMutFrom => {
                (1, DependencyReferenceKind::Borrow, &[])
            }
            _ => return None,
        };

    Some(
        bray_symbols::DependencyBorrowRelation::new(
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
                SymbolOrdinal::new(parameter),
            )),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            kind,
        )
        .with_target_projections(projections.iter().copied()),
    )
}
