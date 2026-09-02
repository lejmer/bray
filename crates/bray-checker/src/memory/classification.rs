use std::collections::BTreeMap;

use bray_bound_tree::{
    CheckedMemoryOperationKind, CheckedMemoryOperations, MemoryAddressKind, MemoryCopyKind,
    MemoryLayoutQueryKind, MemoryOffsetUnit, MemoryReadKind,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, SeverityKind};
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
) -> Result<
    Option<CheckedMemoryOperationKind>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
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

    classify_core_operation(request, hook, &types, expression, read_kinds, diagnostics)
}

fn memory_type_arguments<Upstream>(
    arguments: &[GenericArgument],
) -> Result<Vec<TypeId>, CheckerOutcome<CheckedMemoryOperations, Upstream>> {
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
    expression: bray_bound_tree::BoundExpressionId,
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<
    Option<CheckedMemoryOperationKind>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    if matches!(
        hook,
        ImplementationHook::CallableFromPointer | ImplementationHook::PointerFromCallable
    ) && !validate_callable_address_type(request, hook, types, expression, diagnostics)?
    {
        return Ok(None);
    }

    if let Some(kind) = layout_query_kind(hook)
        && !validate_layout_query_type(request, hook, kind, types, expression, diagnostics)?
    {
        return Ok(None);
    }

    if memory_operation_requires_complete_pointee(hook)
        && !validate_memory_pointee_type(request, hook, types, expression, diagnostics)?
    {
        return Ok(None);
    }

    if let Some(kind) =
        classify_direct_memory_operation(request, hook, types, read_kinds, diagnostics)?
    {
        return Ok(Some(kind));
    }

    match hook {
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
        | ImplementationHook::ByteBufferCopy
        | ImplementationHook::ByteBufferRead => {
            classify_allocation_and_buffer_operation(hook, types)
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
        | ImplementationHook::AtomicNotifyAll => Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        )),
        ImplementationHook::FutureStart
        | ImplementationHook::TaskJoin
        | ImplementationHook::TaskCancel
        | ImplementationHook::BlockingExecution
        | ImplementationHook::ComputeExecution
        | ImplementationHook::MainThreadExecution
        | ImplementationHook::CurrentRunCancellationObservation
        | ImplementationHook::CurrentRunCancellationPropagation
        | ImplementationHook::NativeThreadExecution
        | ImplementationHook::NativeThreadStart
        | ImplementationHook::CurrentNativeThreadIdentity
        | ImplementationHook::MainNativeThreadIdentity
        | ImplementationHook::NativeThreadPanicReportRecovery
        | ImplementationHook::NativeThreadPanicReporting
        | ImplementationHook::TaskEventCreation
        | ImplementationHook::TaskEventSignal
        | ImplementationHook::TaskEventDestruction
        | ImplementationHook::TaskEventWait
        | ImplementationHook::TaskYield
        | ImplementationHook::SequenceLength
        | ImplementationHook::SequenceIsEmpty
        | ImplementationHook::NullableIsPresent
        | ImplementationHook::NullableIsAbsent
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
        | ImplementationHook::RangeSharedIterate
        | ImplementationHook::RangeMoveIterate
        | ImplementationHook::RangeNext
        | ImplementationHook::TestingFail => Ok(None),
        _ => Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        )),
    }
}

fn classify_direct_memory_operation<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<
    Option<CheckedMemoryOperationKind>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let kind = match hook {
        ImplementationHook::UninitNew => CheckedMemoryOperationKind::UninitNew {
            element: one_type_argument(types)?,
        },
        ImplementationHook::UninitPointer => CheckedMemoryOperationKind::UninitPointer {
            kind: MemoryAddressKind::Shared,
            element: one_type_argument(types)?,
        },
        ImplementationHook::UninitPointerMut => CheckedMemoryOperationKind::UninitPointer {
            kind: MemoryAddressKind::Mutable,
            element: one_type_argument(types)?,
        },
        ImplementationHook::UninitWrite => CheckedMemoryOperationKind::UninitWrite {
            element: one_type_argument(types)?,
        },
        ImplementationHook::UninitAssumeInitialized => {
            CheckedMemoryOperationKind::UninitAssumeInitialized {
                element: one_type_argument(types)?,
            }
        }
        ImplementationHook::UninitMove => CheckedMemoryOperationKind::UninitMove {
            element: one_type_argument(types)?,
        },
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
        ImplementationHook::AddressOf | ImplementationHook::AddressOfMut => {
            CheckedMemoryOperationKind::Address {
                kind: if hook == ImplementationHook::AddressOf {
                    MemoryAddressKind::Shared
                } else {
                    MemoryAddressKind::Mutable
                },
                pointee: one_type_argument(types)?,
            }
        }
        ImplementationHook::RawPointerNull => CheckedMemoryOperationKind::Null {
            pointee: one_type_argument(types)?,
        },
        ImplementationHook::RawPointerIsNull => CheckedMemoryOperationKind::IsNull {
            pointee: one_type_argument(types)?,
        },
        ImplementationHook::RawPointerOffset | ImplementationHook::RawPointerByteOffset => {
            CheckedMemoryOperationKind::Offset {
                unit: if hook == ImplementationHook::RawPointerOffset {
                    MemoryOffsetUnit::Element
                } else {
                    MemoryOffsetUnit::Byte
                },
                pointee: one_type_argument(types)?,
            }
        }
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
        ImplementationHook::CallableFromPointer => {
            CheckedMemoryOperationKind::CallableFromPointer {
                callable: one_type_argument(types)?,
            }
        }
        ImplementationHook::PointerFromCallable => {
            CheckedMemoryOperationKind::PointerFromCallable {
                callable: one_type_argument(types)?,
            }
        }
        ImplementationHook::CallbackState | ImplementationHook::TransferredValueBorrow => {
            CheckedMemoryOperationKind::CallbackState {
                state: one_type_argument(types)?,
            }
        }
        ImplementationHook::RawPointerRead => {
            let pointee = one_type_argument(types)?;
            let kind = memory_read_kind(request, pointee, read_kinds, diagnostics)?;

            CheckedMemoryOperationKind::Read { pointee, kind }
        }
        ImplementationHook::RawPointerWrite => CheckedMemoryOperationKind::Write {
            pointee: one_type_argument(types)?,
        },
        ImplementationHook::MemoryCopy | ImplementationHook::MemoryCopyOverlapping => {
            CheckedMemoryOperationKind::Copy {
                pointee: one_type_argument(types)?,
                kind: if hook == ImplementationHook::MemoryCopy {
                    MemoryCopyKind::NonOverlapping
                } else {
                    MemoryCopyKind::Overlapping
                },
            }
        }
        ImplementationHook::MemorySizeOf
        | ImplementationHook::MemoryAlignOf
        | ImplementationHook::MemoryStrideOf
        | ImplementationHook::MemoryLayoutOf
        | ImplementationHook::MemoryTrailingLayoutOf => CheckedMemoryOperationKind::LayoutQuery {
            ty: one_type_argument(types)?,
            kind: layout_query_kind(hook).ok_or_else(|| {
                CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                )
            })?,
        },
        _ => return Ok(None),
    };

    Ok(Some(kind))
}

fn one_type_argument<Upstream>(
    types: &[TypeId],
) -> Result<TypeId, CheckerOutcome<CheckedMemoryOperations, Upstream>> {
    let [ty] = types else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    Ok(*ty)
}

const fn layout_query_kind(hook: ImplementationHook) -> Option<MemoryLayoutQueryKind> {
    match hook {
        ImplementationHook::MemorySizeOf => Some(MemoryLayoutQueryKind::Size),
        ImplementationHook::MemoryAlignOf => Some(MemoryLayoutQueryKind::Alignment),
        ImplementationHook::MemoryStrideOf => Some(MemoryLayoutQueryKind::Stride),
        ImplementationHook::MemoryLayoutOf => Some(MemoryLayoutQueryKind::Layout),
        ImplementationHook::MemoryTrailingLayoutOf => Some(MemoryLayoutQueryKind::Trailing),
        _ => None,
    }
}

const fn memory_operation_requires_complete_pointee(hook: ImplementationHook) -> bool {
    matches!(
        hook,
        ImplementationHook::UninitNew
            | ImplementationHook::UninitPointer
            | ImplementationHook::UninitPointerMut
            | ImplementationHook::UninitWrite
            | ImplementationHook::UninitAssumeInitialized
            | ImplementationHook::UninitMove
            | ImplementationHook::BorrowFrom
            | ImplementationHook::BorrowMutFrom
            | ImplementationHook::RawPointerOffset
            | ImplementationHook::RawPointerRead
            | ImplementationHook::RawPointerWrite
            | ImplementationHook::MemoryCopy
            | ImplementationHook::MemoryCopyOverlapping
    )
}

fn validate_memory_pointee_type<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    expression: bray_bound_tree::BoundExpressionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(pointee) = types.first().copied() else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    let complete = crate::representation::type_supports_complete_fixed_layout(request, pointee)
        .map_err(query_outcome)?;

    let data = request
        .semantic_values()
        .type_data(pointee)
        .map_err(|error| {
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::SemanticValueStore(
                error,
            ))
        })?;

    if matches!(data.as_ref(), bray_symbols::TypeData::Error)
        || complete && !matches!(data.as_ref(), bray_symbols::TypeData::Callable(_))
    {
        return Ok(true);
    }

    let operation =
        crate::memory_diagnostics::diagnostic_memory_operation(hook).ok_or_else(|| {
            CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )
        })?;

    let span = crate::diagnostic::expression_span(request, expression)
        .map_err(CheckerOutcome::InfrastructureFailure)?;

    diagnostics.add(
        Diagnostic::new(
            crate::diagnostic::diagnostic_id(diagnostics.len()),
            DiagnosticKind::CheckingMemoryPointeeTypeUnsupported,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(DiagnosticArg::memory_operation(operation)),
    );

    Ok(false)
}

fn validate_layout_query_type<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    kind: MemoryLayoutQueryKind,
    types: &[TypeId],
    expression: bray_bound_tree::BoundExpressionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let [ty] = types else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    let fixed = crate::representation::type_supports_complete_fixed_layout(request, *ty)
        .map_err(query_outcome)?;

    let flexible = crate::representation::type_supports_flexible_c_layout(request, *ty)
        .map_err(query_outcome)?;

    let valid = match kind {
        MemoryLayoutQueryKind::Alignment => fixed || flexible,
        MemoryLayoutQueryKind::Trailing => flexible,
        MemoryLayoutQueryKind::Size
        | MemoryLayoutQueryKind::Stride
        | MemoryLayoutQueryKind::Layout => fixed,
    };

    if valid {
        return Ok(true);
    }

    let span = crate::diagnostic::expression_span(request, expression)
        .map_err(CheckerOutcome::InfrastructureFailure)?;

    let mut diagnostic = Diagnostic::new(
        crate::diagnostic::diagnostic_id(diagnostics.len()),
        if kind == MemoryLayoutQueryKind::Trailing {
            DiagnosticKind::CheckingTrailingLayoutQueryTypeUnsupported
        } else {
            DiagnosticKind::CheckingFixedLayoutQueryTypeUnsupported
        },
        SeverityKind::Error,
    )
    .with_primary_span(span);

    if kind != MemoryLayoutQueryKind::Trailing {
        let operation =
            crate::memory_diagnostics::diagnostic_memory_operation(hook).ok_or_else(|| {
                CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                )
            })?;

        diagnostic = diagnostic.with_arg(DiagnosticArg::memory_operation(operation));
    }

    diagnostics.add(diagnostic);

    Ok(false)
}

fn query_outcome<Upstream>(
    error: crate::CheckerQueryError<Upstream>,
) -> CheckerOutcome<CheckedMemoryOperations, Upstream> {
    match error {
        crate::CheckerQueryError::Cancelled => CheckerOutcome::Cancelled,
        crate::CheckerQueryError::Infrastructure(error) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        crate::CheckerQueryError::Upstream(error) => CheckerOutcome::UpstreamFailure(error),
    }
}

fn validate_callable_address_type<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    expression: bray_bound_tree::BoundExpressionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let [callable] = types else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    let data = request
        .semantic_values()
        .type_data(*callable)
        .map_err(|error| {
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::SemanticValueStore(
                error,
            ))
        })?;

    let valid_type = matches!(
        data.as_ref(),
        bray_symbols::TypeData::Callable(callable)
            if callable.abi() != bray_symbols::CallableAbi::Bray
    );

    let span = crate::diagnostic::expression_span(request, expression)
        .map_err(CheckerOutcome::InfrastructureFailure)?;

    if !valid_type {
        diagnostics.add(
            Diagnostic::new(
                crate::diagnostic::diagnostic_id(diagnostics.len()),
                DiagnosticKind::CheckingCallableAddressTypeUnsupported,
                SeverityKind::Error,
            )
            .with_primary_span(span),
        );

        return Ok(false);
    }

    if !request
        .selected_target()
        .properties()
        .operations()
        .callable_addresses()
    {
        let operation =
            crate::memory_diagnostics::diagnostic_memory_operation(hook).ok_or_else(|| {
                CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                )
            })?;

        crate::memory_diagnostics::add_target_memory_operation_unavailable(
            span,
            request.selected_target().identity().as_str(),
            operation,
            diagnostics,
        );

        return Ok(false);
    }

    Ok(true)
}

fn classify_allocation_and_buffer_operation<Upstream>(
    hook: ImplementationHook,
    types: &[TypeId],
) -> Result<Option<CheckedMemoryOperationKind>, CheckerOutcome<CheckedMemoryOperations, Upstream>> {
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
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferCapacity
        }
        ImplementationHook::RawBufferInitializedCount => {
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferInitializedCount
        }
        ImplementationHook::RawBufferPointer => {
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferPointer
        }
        ImplementationHook::RawBufferInitializedSlice => {
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferInitializedSlice
        }
        ImplementationHook::RawBufferInitializedSliceMut => {
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferInitializedSliceMut
        }
        ImplementationHook::RawBufferSparePointer => {
            CheckedMemoryOperationKind::RawBufferSparePointer {
                element: one_type_argument(types)?,
            }
        }
        ImplementationHook::RawBufferSetInitializedCount => {
            one_type_argument(types)?;

            CheckedMemoryOperationKind::RawBufferSetInitializedCount
        }
        ImplementationHook::RawBufferRelease => CheckedMemoryOperationKind::RawBufferRelease {
            element: one_type_argument(types)?,
        },
        ImplementationHook::RawBufferReplace => CheckedMemoryOperationKind::RawBufferReplace {
            element: one_type_argument(types)?,
        },
        ImplementationHook::RawBufferRelocate => CheckedMemoryOperationKind::RawBufferRelocate {
            element: one_type_argument(types)?,
        },
        ImplementationHook::ByteBufferFill => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteBufferFill
        }
        ImplementationHook::ByteBufferCopy => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteBufferCopy
        }
        ImplementationHook::ByteBufferRead => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::ByteBufferRead
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
) -> Result<MemoryReadKind, CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
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
        CheckerOutcome::UpstreamFailure(error) => {
            return Err(CheckerOutcome::UpstreamFailure(error));
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

fn ensure_no_type_arguments<Upstream>(
    types: &[TypeId],
) -> Result<(), CheckerOutcome<CheckedMemoryOperations, Upstream>> {
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
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_symbols::{GenericArgument, GenericTypeParameterSymbolId, SymbolId, TypeData};

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
    fn raw_reads_reject_dynamically_sized_pointees() {
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

            assert_eq!(moved, Ok(None));

            assert_eq!(
                diagnostics
                    .iter()
                    .filter(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::CheckingMemoryPointeeTypeUnsupported
                    })
                    .count(),
                1
            );
        });
    }

    #[test]
    fn generic_layout_requirements_defer_to_instantiation() {
        with_request(|request| {
            let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(8_001));

            let ty = semantic_values()
                .intern_type(TypeData::TypeParameter(parameter))
                .unwrap_or_else(|error| panic!("generic parameter type must intern: {error:?}"));

            assert_eq!(
                crate::representation::type_supports_complete_fixed_layout(request, ty),
                Ok(true)
            );

            assert_eq!(
                crate::representation::type_supports_flexible_c_layout(request, ty),
                Ok(true)
            );
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
