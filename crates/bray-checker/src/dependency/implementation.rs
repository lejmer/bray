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
        | ImplementationHook::RawPointerByteAddress
        | ImplementationHook::RawPointerReinterpret
        | ImplementationHook::FunctionPointerReinterpret
        | ImplementationHook::FunctionPointerFromData
        | ImplementationHook::FunctionPointerToData => Some((
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
            Some(bray_symbols::DependencySubject::root(
                bray_symbols::DependencySubjectRoot::Parameter(SymbolOrdinal::new(parameter)),
            )),
            bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result),
            kind,
        )
        .with_target_projections(projections.iter().copied()),
    )
}

/// Publishes the carried-value and reference contracts of selected generator cursor operations.
/// Copied elements retain their original dependencies independently of the cursor's owner borrow.
pub fn generator_result_dependencies(
    symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    implementation: ImplementationHook,
) -> Option<bray_symbols::DependencyContractTemplateData> {
    use bray_symbols::{
        DependencyBorrowRelation, DependencyContractTemplateData, DependencyProjection,
        DependencyRequirement, DependencyRequirementKind, DependencyStorageEffect,
        DependencySubject,
    };

    if !implementation.is_generator_iteration() {
        return None;
    }

    let field = |name| {
        let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
            .expect("generator contract uses a catalog key");

        DependencyProjection::ProductField(
            symbols
                .declaration_symbol(&key)
                .expect("generator cursor retains its values field"),
        )
    };

    let projections = match implementation {
        ImplementationHook::GeneratorSharedNext => vec![
            field("GeneratorSharedCursorValues"),
            DependencyProjection::OwnedTarget,
        ],
        ImplementationHook::GeneratorMoveNext => vec![field("GeneratorMoveCursorValues")],
        _ => Vec::new(),
    };

    let source = DependencySubject::new(DependencySubjectRoot::Receiver, projections);

    let mut template = DependencyContractTemplateData::new([DependencyRequirement::direct(
        source,
        DependencyRequirementKind::ValueDependencies,
    )])
    .with_effects(DependencyStorageEffect::capture_free_call());

    if implementation == ImplementationHook::GeneratorSharedIterate {
        template = template.with_borrows([DependencyBorrowRelation::new(
            Some(DependencySubject::root(DependencySubjectRoot::Receiver)),
            DependencySubject::new(
                DependencySubjectRoot::Result,
                [field("GeneratorSharedCursorValues")],
            ),
            DependencyReferenceKind::Borrow,
        )]);
    }

    Some(template)
}
