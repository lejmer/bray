use std::collections::BTreeMap;

use bray_bound_tree::{
    CheckedMemoryOperationKind, CheckedMemoryOperations, MemoryAddressKind, MemoryCopyKind,
    MemoryLayoutQueryKind, MemoryOffsetUnit, MemoryReadKind,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{GenericArgument, TypeId};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    type_is_copyable,
};

pub(super) fn classify_operation<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    generic_arguments: &[GenericArgument],
    expression: bray_bound_tree::BoundExpressionId,
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<CheckedMemoryOperationKind>, CheckerOutcome<CheckedMemoryOperations>>
where
    C: CheckerRequestContext + ?Sized,
{
    if crate::atomic::atomic_hook(hook) {
        return crate::atomic::classify_atomic_operation(
            request,
            hook,
            generic_arguments,
            expression,
            diagnostics,
        );
    }

    let types = memory_type_arguments(generic_arguments)?;

    if let Some(kind) =
        crate::target_control::classify_operation(request, hook, &types, read_kinds, diagnostics)?
    {
        return Ok(Some(kind));
    }

    classify_core_operation(request, hook, &types, read_kinds, diagnostics)
}

fn memory_type_arguments(
    arguments: &[GenericArgument],
) -> Result<Vec<TypeId>, CheckerOutcome<CheckedMemoryOperations>> {
    arguments
        .iter()
        .map(|argument| match argument {
            GenericArgument::Type(ty) => Ok(*ty),
            GenericArgument::Constant(_) => Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )),
        })
        .collect()
}

fn classify_core_operation<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<CheckedMemoryOperationKind>, CheckerOutcome<CheckedMemoryOperations>>
where
    C: CheckerRequestContext + ?Sized,
{
    let one = || {
        let [ty] = types else {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        };

        Ok(*ty)
    };

    let kind = match hook {
        ImplementationHook::UninitNew => CheckedMemoryOperationKind::UninitNew { element: one()? },
        ImplementationHook::UninitPointer => CheckedMemoryOperationKind::UninitPointer {
            kind: MemoryAddressKind::Shared,
            element: one()?,
        },
        ImplementationHook::UninitPointerMut => CheckedMemoryOperationKind::UninitPointer {
            kind: MemoryAddressKind::Mutable,
            element: one()?,
        },
        ImplementationHook::UninitWrite => {
            CheckedMemoryOperationKind::UninitWrite { element: one()? }
        }
        ImplementationHook::UninitAssumeInitialized => {
            CheckedMemoryOperationKind::UninitAssumeInitialized { element: one()? }
        }
        ImplementationHook::UninitMove => {
            CheckedMemoryOperationKind::UninitMove { element: one()? }
        }
        ImplementationHook::BorrowFrom | ImplementationHook::BorrowMutFrom => {
            let [pointee, _authority] = types else {
                return Err(CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ));
            };

            CheckedMemoryOperationKind::BorrowFrom {
                kind: if hook == ImplementationHook::BorrowFrom {
                    MemoryAddressKind::Shared
                } else {
                    MemoryAddressKind::Mutable
                },
                pointee: *pointee,
            }
        }
        ImplementationHook::AddressOf => CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Shared,
            pointee: one()?,
        },
        ImplementationHook::AddressOfMut => CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Mutable,
            pointee: one()?,
        },
        ImplementationHook::RawPointerNull => CheckedMemoryOperationKind::Null { pointee: one()? },
        ImplementationHook::RawPointerIsNull => {
            CheckedMemoryOperationKind::IsNull { pointee: one()? }
        }
        ImplementationHook::RawPointerOffset => CheckedMemoryOperationKind::Offset {
            unit: MemoryOffsetUnit::Element,
            pointee: one()?,
        },
        ImplementationHook::RawPointerByteOffset => CheckedMemoryOperationKind::Offset {
            unit: MemoryOffsetUnit::Byte,
            pointee: one()?,
        },
        ImplementationHook::RawPointerReinterpret => {
            let [target, source] = types else {
                return Err(CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ));
            };

            CheckedMemoryOperationKind::Reinterpret {
                source: *source,
                target: *target,
            }
        }
        ImplementationHook::CallbackState => {
            CheckedMemoryOperationKind::CallbackState { state: one()? }
        }
        ImplementationHook::RawPointerRead => {
            let pointee = one()?;

            let read_kind = memory_read_kind(request, pointee, read_kinds, diagnostics)?;

            CheckedMemoryOperationKind::Read {
                pointee,
                kind: read_kind,
            }
        }
        ImplementationHook::RawPointerWrite => {
            CheckedMemoryOperationKind::Write { pointee: one()? }
        }
        ImplementationHook::MemoryCopy => CheckedMemoryOperationKind::Copy {
            pointee: one()?,
            kind: MemoryCopyKind::NonOverlapping,
        },
        ImplementationHook::MemoryCopyOverlapping => CheckedMemoryOperationKind::Copy {
            pointee: one()?,
            kind: MemoryCopyKind::Overlapping,
        },
        ImplementationHook::MemorySizeOf => CheckedMemoryOperationKind::LayoutQuery {
            ty: one()?,
            kind: MemoryLayoutQueryKind::Size,
        },
        ImplementationHook::MemoryAlignOf => CheckedMemoryOperationKind::LayoutQuery {
            ty: one()?,
            kind: MemoryLayoutQueryKind::Alignment,
        },
        ImplementationHook::MemoryStrideOf => CheckedMemoryOperationKind::LayoutQuery {
            ty: one()?,
            kind: MemoryLayoutQueryKind::Stride,
        },
        ImplementationHook::MemoryLayoutOf => CheckedMemoryOperationKind::LayoutQuery {
            ty: one()?,
            kind: MemoryLayoutQueryKind::Layout,
        },
        ImplementationHook::RawAllocate
        | ImplementationHook::RawDeallocate
        | ImplementationHook::Allocate
        | ImplementationHook::Deallocate
        | ImplementationHook::RawBufferCapacity
        | ImplementationHook::RawBufferInitializedCount
        | ImplementationHook::RawBufferPointer
        | ImplementationHook::RawBufferInitializedSlice
        | ImplementationHook::RawBufferInitializedSliceMut
        | ImplementationHook::RawBufferSparePointer
        | ImplementationHook::RawBufferSetInitializedCount
        | ImplementationHook::RawBufferRelease
        | ImplementationHook::RawBufferReplace
        | ImplementationHook::RawBufferRelocate
        | ImplementationHook::ByteBufferFill
        | ImplementationHook::ByteSliceCopy
        | ImplementationHook::ByteBufferRead
        | ImplementationHook::SliceLength => {
            return classify_allocation_and_buffer_operation(hook, types);
        }
        ImplementationHook::VolatileLoad
        | ImplementationHook::VolatileStore
        | ImplementationHook::DeviceVolatileLoad
        | ImplementationHook::DeviceVolatileStore
        | ImplementationHook::PointerExposeAddress
        | ImplementationHook::PointerFromExposedAddress
        | ImplementationHook::PointerAddressEqual
        | ImplementationHook::PointerAddressLess
        | ImplementationHook::CompilerFence
        | ImplementationHook::HardwareFence
        | ImplementationHook::CatastrophicAbort
        | ImplementationHook::DebuggerTrap
        | ImplementationHook::UnreachableTermination
        | ImplementationHook::SpinLoopHint
        | ImplementationHook::TargetFeatureEnabled
        | ImplementationHook::InlineAssembly
        | ImplementationHook::DivergingInlineAssembly
        | ImplementationHook::BranchingInlineAssembly
        | ImplementationHook::AtomicInitialize
        | ImplementationHook::AtomicLoad
        | ImplementationHook::AtomicStore
        | ImplementationHook::AtomicExchange
        | ImplementationHook::AtomicCompareExchange
        | ImplementationHook::AtomicCompareExchangeWeak
        | ImplementationHook::AtomicFetchAdd
        | ImplementationHook::AtomicFetchSub
        | ImplementationHook::AtomicFetchAnd
        | ImplementationHook::AtomicFetchOr
        | ImplementationHook::AtomicFetchXor
        | ImplementationHook::AtomicFence
        | ImplementationHook::AtomicCompilerFence
        | ImplementationHook::AtomicWait
        | ImplementationHook::AtomicNotifyOne
        | ImplementationHook::AtomicNotifyAll => {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        }
        ImplementationHook::FutureStart
        | ImplementationHook::TaskJoin
        | ImplementationHook::TaskCancel
        | ImplementationHook::BlockingExecution
        | ImplementationHook::ComputeExecution
        | ImplementationHook::MainThreadExecution
        | ImplementationHook::CurrentRunCancellationObservation
        | ImplementationHook::CurrentRunCancellationPropagation
        | ImplementationHook::TaskYield
        | ImplementationHook::StringScalarCount
        | ImplementationHook::StringIsEmpty
        | ImplementationHook::StringEquals
        | ImplementationHook::StringScalarAt
        | ImplementationHook::StringScalarSlice
        | ImplementationHook::StringUtf8
        | ImplementationHook::StringFromUtf8
        | ImplementationHook::CharacterScalarValue
        | ImplementationHook::CharacterFromScalarValue
        | ImplementationHook::CharacterUtf8Length
        | ImplementationHook::CharacterUtf8Byte
        | ImplementationHook::CharacterIsAlphabetic
        | ImplementationHook::CharacterIsNumeric
        | ImplementationHook::CharacterIsWhitespace
        | ImplementationHook::NumericTruncate
        | ImplementationHook::TestingFail => return Ok(None),
    };

    Ok(Some(kind))
}

fn classify_allocation_and_buffer_operation(
    hook: ImplementationHook,
    types: &[TypeId],
) -> Result<Option<CheckedMemoryOperationKind>, CheckerOutcome<CheckedMemoryOperations>> {
    let one = || {
        let [ty] = types else {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        };

        Ok(*ty)
    };

    let kind = match hook {
        ImplementationHook::RawAllocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::RawAllocate
        }
        ImplementationHook::RawDeallocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::RawDeallocate
        }
        ImplementationHook::Allocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::Allocate
        }
        ImplementationHook::Deallocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::Deallocate
        }
        ImplementationHook::RawBufferCapacity => {
            one()?;

            CheckedMemoryOperationKind::RawBufferCapacity
        }
        ImplementationHook::RawBufferInitializedCount => {
            one()?;

            CheckedMemoryOperationKind::RawBufferInitializedCount
        }
        ImplementationHook::RawBufferPointer => {
            one()?;

            CheckedMemoryOperationKind::RawBufferPointer
        }
        ImplementationHook::RawBufferInitializedSlice => {
            one()?;

            CheckedMemoryOperationKind::RawBufferInitializedSlice
        }
        ImplementationHook::RawBufferInitializedSliceMut => {
            one()?;

            CheckedMemoryOperationKind::RawBufferInitializedSliceMut
        }
        ImplementationHook::RawBufferSparePointer => {
            CheckedMemoryOperationKind::RawBufferSparePointer { element: one()? }
        }
        ImplementationHook::RawBufferSetInitializedCount => {
            one()?;

            CheckedMemoryOperationKind::RawBufferSetInitializedCount
        }
        ImplementationHook::RawBufferRelease => {
            CheckedMemoryOperationKind::RawBufferRelease { element: one()? }
        }
        ImplementationHook::RawBufferReplace => {
            CheckedMemoryOperationKind::RawBufferReplace { element: one()? }
        }
        ImplementationHook::RawBufferRelocate => {
            CheckedMemoryOperationKind::RawBufferRelocate { element: one()? }
        }
        ImplementationHook::ByteBufferFill => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteBufferFill
        }
        ImplementationHook::ByteSliceCopy => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteSliceCopy
        }
        ImplementationHook::ByteBufferRead => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteBufferRead
        }
        ImplementationHook::SliceLength => {
            one()?;

            CheckedMemoryOperationKind::SliceLength
        }
        _ => {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        }
    };

    Ok(Some(kind))
}

pub(crate) fn memory_read_kind<C>(
    request: CheckerUnitView<'_, C>,
    pointee: TypeId,
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<MemoryReadKind, CheckerOutcome<CheckedMemoryOperations>>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(kind) = read_kinds.get(&pointee).copied() {
        return Ok(kind);
    }

    let result = match type_is_copyable(request, pointee) {
        CheckerOutcome::Complete(result) => result,
        CheckerOutcome::Cancelled => return Err(CheckerOutcome::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            return Err(CheckerOutcome::InfrastructureFailure(error));
        }
    };

    diagnostics.add_range(result.diagnostics().iter().cloned());

    let kind = if *result.value() {
        MemoryReadKind::Copy
    } else {
        MemoryReadKind::Move
    };

    read_kinds.insert(pointee, kind);

    Ok(kind)
}

fn ensure_no_type_arguments(
    types: &[TypeId],
) -> Result<(), CheckerOutcome<CheckedMemoryOperations>> {
    if !types.is_empty() {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundUnitId, CheckedMemoryOperationKind,
        MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind, MemoryOffsetUnit, MemoryReadKind,
    };
    use bray_compiler_known::ImplementationHook;
    use bray_diagnostics::DiagnosticBag;
    use bray_symbols::{GenericArgument, TypeData};

    use super::classify_operation;
    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, error_type, expression_unit, push_expression,
        semantic_values,
    };

    #[test]
    fn hooks_classify_without_source_name_matching() {
        with_request(|request| {
            let ty = error_type();
            let mut read_kinds = BTreeMap::new();
            let mut diagnostics = DiagnosticBag::new();

            let cases = [
                (
                    ImplementationHook::UninitNew,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitNew { element: ty },
                ),
                (
                    ImplementationHook::UninitPointer,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitPointer {
                        kind: MemoryAddressKind::Shared,
                        element: ty,
                    },
                ),
                (
                    ImplementationHook::UninitPointerMut,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitPointer {
                        kind: MemoryAddressKind::Mutable,
                        element: ty,
                    },
                ),
                (
                    ImplementationHook::UninitWrite,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitWrite { element: ty },
                ),
                (
                    ImplementationHook::UninitAssumeInitialized,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitAssumeInitialized { element: ty },
                ),
                (
                    ImplementationHook::UninitMove,
                    vec![ty],
                    CheckedMemoryOperationKind::UninitMove { element: ty },
                ),
                (
                    ImplementationHook::BorrowFrom,
                    vec![ty, ty],
                    CheckedMemoryOperationKind::BorrowFrom {
                        kind: MemoryAddressKind::Shared,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::BorrowMutFrom,
                    vec![ty, ty],
                    CheckedMemoryOperationKind::BorrowFrom {
                        kind: MemoryAddressKind::Mutable,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::AddressOf,
                    vec![ty],
                    CheckedMemoryOperationKind::Address {
                        kind: MemoryAddressKind::Shared,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::AddressOfMut,
                    vec![ty],
                    CheckedMemoryOperationKind::Address {
                        kind: MemoryAddressKind::Mutable,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::RawPointerNull,
                    vec![ty],
                    CheckedMemoryOperationKind::Null { pointee: ty },
                ),
                (
                    ImplementationHook::RawPointerIsNull,
                    vec![ty],
                    CheckedMemoryOperationKind::IsNull { pointee: ty },
                ),
                (
                    ImplementationHook::RawPointerOffset,
                    vec![ty],
                    CheckedMemoryOperationKind::Offset {
                        unit: MemoryOffsetUnit::Element,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::RawPointerByteOffset,
                    vec![ty],
                    CheckedMemoryOperationKind::Offset {
                        unit: MemoryOffsetUnit::Byte,
                        pointee: ty,
                    },
                ),
                (
                    ImplementationHook::RawPointerReinterpret,
                    vec![ty, ty],
                    CheckedMemoryOperationKind::Reinterpret {
                        source: ty,
                        target: ty,
                    },
                ),
                (
                    ImplementationHook::RawPointerRead,
                    vec![ty],
                    CheckedMemoryOperationKind::Read {
                        pointee: ty,
                        kind: MemoryReadKind::Copy,
                    },
                ),
                (
                    ImplementationHook::RawPointerWrite,
                    vec![ty],
                    CheckedMemoryOperationKind::Write { pointee: ty },
                ),
                (
                    ImplementationHook::MemoryCopy,
                    vec![ty],
                    CheckedMemoryOperationKind::Copy {
                        pointee: ty,
                        kind: MemoryCopyKind::NonOverlapping,
                    },
                ),
                (
                    ImplementationHook::MemoryCopyOverlapping,
                    vec![ty],
                    CheckedMemoryOperationKind::Copy {
                        pointee: ty,
                        kind: MemoryCopyKind::Overlapping,
                    },
                ),
                (
                    ImplementationHook::MemorySizeOf,
                    vec![ty],
                    CheckedMemoryOperationKind::LayoutQuery {
                        ty,
                        kind: MemoryLayoutQueryKind::Size,
                    },
                ),
                (
                    ImplementationHook::MemoryAlignOf,
                    vec![ty],
                    CheckedMemoryOperationKind::LayoutQuery {
                        ty,
                        kind: MemoryLayoutQueryKind::Alignment,
                    },
                ),
                (
                    ImplementationHook::MemoryStrideOf,
                    vec![ty],
                    CheckedMemoryOperationKind::LayoutQuery {
                        ty,
                        kind: MemoryLayoutQueryKind::Stride,
                    },
                ),
                (
                    ImplementationHook::MemoryLayoutOf,
                    vec![ty],
                    CheckedMemoryOperationKind::LayoutQuery {
                        ty,
                        kind: MemoryLayoutQueryKind::Layout,
                    },
                ),
                (
                    ImplementationHook::RawAllocate,
                    vec![],
                    CheckedMemoryOperationKind::RawAllocate,
                ),
                (
                    ImplementationHook::RawDeallocate,
                    vec![],
                    CheckedMemoryOperationKind::RawDeallocate,
                ),
                (
                    ImplementationHook::Allocate,
                    vec![],
                    CheckedMemoryOperationKind::Allocate,
                ),
                (
                    ImplementationHook::Deallocate,
                    vec![],
                    CheckedMemoryOperationKind::Deallocate,
                ),
            ];

            for (hook, types, expected) in cases {
                let arguments = types
                    .into_iter()
                    .map(GenericArgument::Type)
                    .collect::<Vec<_>>();

                let expression = first_expression(request);

                let result = classify_operation(
                    request,
                    hook,
                    &arguments,
                    expression,
                    &mut read_kinds,
                    &mut diagnostics,
                );

                assert_eq!(result, Ok(Some(expected)));
            }

            assert!(diagnostics.is_empty());
        });
    }

    #[test]
    fn raw_reads_follow_the_pointee_copy_contract() {
        with_request(|request| {
            let copyable = error_type();

            let non_copyable = semantic_values()
                .intern_type(TypeData::Slice(copyable))
                .unwrap_or_else(|error| panic!("slice type must intern: {error:?}"));

            let mut read_kinds = BTreeMap::new();
            let mut diagnostics = DiagnosticBag::new();
            let expression = first_expression(request);

            let copy = classify_operation(
                request,
                ImplementationHook::RawPointerRead,
                &[GenericArgument::Type(copyable)],
                expression,
                &mut read_kinds,
                &mut diagnostics,
            );

            let moved = classify_operation(
                request,
                ImplementationHook::RawPointerRead,
                &[GenericArgument::Type(non_copyable)],
                expression,
                &mut read_kinds,
                &mut diagnostics,
            );

            assert_eq!(
                copy,
                Ok(Some(CheckedMemoryOperationKind::Read {
                    pointee: copyable,
                    kind: MemoryReadKind::Copy,
                }))
            );

            assert_eq!(
                moved,
                Ok(Some(CheckedMemoryOperationKind::Read {
                    pointee: non_copyable,
                    kind: MemoryReadKind::Move,
                }))
            );

            assert!(diagnostics.is_empty());
        });
    }

    fn with_request<T>(action: impl FnOnce(CheckerUnitView<'_, TestCheckerContext>) -> T) -> T {
        let (unit, _) = expression_unit(BoundUnitId::new(41), |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        let entry = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("memory checker request must build: {error:?}"));

        action(request)
    }

    fn first_expression(
        request: CheckerUnitView<'_, TestCheckerContext>,
    ) -> bray_bound_tree::BoundExpressionId {
        request
            .unit()
            .tree()
            .expressions()
            .next()
            .map(|(expression, _)| expression)
            .unwrap_or_else(|| panic!("memory checker test requires one expression"))
    }
}
