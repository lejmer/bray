use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundCallableTarget, CheckedMemoryOperation, CheckedMemoryOperationKind,
    CheckedMemoryOperations, CheckedSemanticSelections, MemoryAddressKind, MemoryCopyKind,
    MemoryLayoutQueryKind, MemoryOffsetUnit, MemoryReadKind, SemanticSelection,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, DiagnosticResult, SeverityKind};
use bray_symbols::{CallableInstanceData, GenericArgument, TypeId};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    type_is_copyable,
};
use crate::diagnostic::{diagnostic_id, expression_span};

pub(crate) fn check_memory_operations<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
) -> CheckerOutcome<CheckedMemoryOperations>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if selections.unit() != request.unit().unit()
        || selections.kind() != request.unit().key().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let mut operations = Vec::new();
    let mut read_kinds = BTreeMap::new();
    let mut diagnostics = DiagnosticBag::new();

    for entry in selections.entries() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let SemanticSelection::Call(call) = entry.selection() else {
            continue;
        };

        let BoundCallableTarget::Declaration(instance) = call.target() else {
            continue;
        };

        let symbol = instance.definition().symbol();
        let available = request.available_compiler_known_symbols();

        let Some(hook) = available.provider().role_registry().symbol_implementation(symbol)
        else {
            continue;
        };

        if available.symbol_implementation(symbol).is_none() {
            let span = match expression_span(request, entry.expression()) {
                Ok(span) => span,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            diagnostics.add(
                Diagnostic::new(
                    diagnostic_id(diagnostics.len()),
                    DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
            );

            continue;
        }

        let type_arguments = match type_arguments(request, instance) {
            Ok(arguments) => arguments,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        let kind = match classify_operation(
            request,
            hook,
            &type_arguments,
            &mut read_kinds,
            &mut diagnostics,
        ) {
            Ok(Some(kind)) => kind,
            Ok(None) => continue,
            Err(outcome) => return outcome,
        };

        operations.push(CheckedMemoryOperation::new(entry.expression(), kind));
    }

    let facts = match CheckedMemoryOperations::try_new(
        request.unit().unit(),
        request.unit().key().kind(),
        operations,
        false,
    ) {
        Ok(facts) => facts,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            );
        }
    };

    CheckerOutcome::Complete(DiagnosticResult::new(facts, diagnostics))
}

fn type_arguments<C>(
    request: CheckerUnitView<'_, C>,
    instance: CallableInstanceData,
) -> Result<Vec<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution = request
        .semantic_values()
        .generic_substitution_data(instance.substitution())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    substitution
        .bindings()
        .iter()
        .map(|binding| match binding.argument() {
            GenericArgument::Type(ty) => Ok(ty),
            GenericArgument::Constant(_) => {
                Err(CheckerInfrastructureError::InvalidSemanticSelectionInput)
            }
        })
        .collect()
}

fn classify_operation<C>(
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
        ImplementationHook::AddressOf => CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Shared,
            pointee: one()?,
        },
        ImplementationHook::AddressOfMut => CheckedMemoryOperationKind::Address {
            kind: MemoryAddressKind::Mutable,
            pointee: one()?,
        },
        ImplementationHook::RawPointerNull => CheckedMemoryOperationKind::Null {
            pointee: one()?,
        },
        ImplementationHook::RawPointerIsNull => CheckedMemoryOperationKind::IsNull {
            pointee: one()?,
        },
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
        ImplementationHook::RawPointerRead => {
            let pointee = one()?;

            let read_kind = match read_kinds.get(&pointee).copied() {
                Some(kind) => kind,
                None => {
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

                    kind
                }
            };

            CheckedMemoryOperationKind::Read {
                pointee,
                kind: read_kind,
            }
        }
        ImplementationHook::RawPointerWrite => CheckedMemoryOperationKind::Write {
            pointee: one()?,
        },
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
        ImplementationHook::Allocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::Allocate
        }
        ImplementationHook::Deallocate => {
            ensure_no_type_arguments(types)?;

            CheckedMemoryOperationKind::Deallocate
        }
        ImplementationHook::FutureStart
        | ImplementationHook::TaskJoin
        | ImplementationHook::TaskCancel
        | ImplementationHook::BlockingExecution
        | ImplementationHook::ComputeExecution
        | ImplementationHook::MainThreadExecution => return Ok(None),
    };

    Ok(Some(kind))
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
        MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind, MemoryOffsetUnit,
        MemoryReadKind,
    };
    use bray_compiler_known::ImplementationHook;
    use bray_diagnostics::DiagnosticBag;
    use bray_symbols::TypeData;

    use super::classify_operation;
    use crate::test_support::{
        TestCheckerContext, callable_entry, error_type, expression_unit, push_expression,
        semantic_values,
    };
    use crate::CheckerUnitView;

    #[test]
    fn hooks_classify_without_source_name_matching() {
        with_request(|request| {
            let ty = error_type();
            let mut read_kinds = BTreeMap::new();
            let mut diagnostics = DiagnosticBag::new();

            let cases = [
                (
                    ImplementationHook::AddressOf,
                    vec![ty],
                    CheckedMemoryOperationKind::Address {
                        kind: MemoryAddressKind::Shared,
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
                    ImplementationHook::MemoryCopyOverlapping,
                    vec![ty],
                    CheckedMemoryOperationKind::Copy {
                        pointee: ty,
                        kind: MemoryCopyKind::Overlapping,
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
                    ImplementationHook::Allocate,
                    vec![],
                    CheckedMemoryOperationKind::Allocate,
                ),
            ];

            for (hook, types, expected) in cases {
                let result = classify_operation(
                    request,
                    hook,
                    &types,
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

            let copy = classify_operation(
                request,
                ImplementationHook::RawPointerRead,
                &[copyable],
                &mut read_kinds,
                &mut diagnostics,
            );

            let moved = classify_operation(
                request,
                ImplementationHook::RawPointerRead,
                &[non_copyable],
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
}
