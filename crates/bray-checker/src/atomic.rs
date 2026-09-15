use std::collections::BTreeSet;

use bray_bound_tree::{
    AtomicFetchKind, BoundExpressionId, CheckedMemoryOperationKind, CheckedMemoryOperations,
    MemoryOrder,
};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticMemoryOperation, SeverityKind,
};
use bray_symbols::{
    DeclaredLayoutMode, DeclaredStorageShape, GenericArgument, GenericSubstitutionId, TypeData,
    TypeId,
};
use bray_target::TargetAtomicRepresentation;

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

pub(crate) const fn atomic_hook(hook: ImplementationHook) -> bool {
    matches!(
        hook,
        ImplementationHook::AtomicInitialize
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
            | ImplementationHook::AtomicNotifyAll
    )
}

pub(crate) const fn diagnostic_hook(hook: ImplementationHook) -> Option<DiagnosticMemoryOperation> {
    use DiagnosticMemoryOperation as Operation;
    use ImplementationHook as Hook;

    Some(match hook {
        Hook::AtomicInitialize => Operation::AtomicInitialization,
        Hook::AtomicLoad => Operation::AtomicLoad,
        Hook::AtomicStore => Operation::AtomicStore,
        Hook::AtomicExchange => Operation::AtomicExchange,
        Hook::AtomicCompareExchange => Operation::AtomicCompareExchange,
        Hook::AtomicCompareExchangeWeak => Operation::AtomicCompareExchangeWeak,
        Hook::AtomicFetchAdd => Operation::AtomicFetchAdd,
        Hook::AtomicFetchSub => Operation::AtomicFetchSubtract,
        Hook::AtomicFetchAnd => Operation::AtomicFetchAnd,
        Hook::AtomicFetchOr => Operation::AtomicFetchOr,
        Hook::AtomicFetchXor => Operation::AtomicFetchXor,
        Hook::AtomicFence => Operation::AtomicFence,
        Hook::AtomicCompilerFence => Operation::AtomicCompilerFence,
        Hook::AtomicWait => Operation::AtomicWait,
        Hook::AtomicNotifyOne => Operation::AtomicNotifyOne,
        Hook::AtomicNotifyAll => Operation::AtomicNotifyAll,
        _ => return None,
    })
}

pub(crate) const fn diagnostic_checked_operation(
    kind: CheckedMemoryOperationKind,
) -> Option<DiagnosticMemoryOperation> {
    use DiagnosticMemoryOperation as Operation;

    Some(match kind {
        CheckedMemoryOperationKind::AtomicInitialize { .. } => Operation::AtomicInitialization,
        CheckedMemoryOperationKind::AtomicLoad { .. } => Operation::AtomicLoad,
        CheckedMemoryOperationKind::AtomicStore { .. } => Operation::AtomicStore,
        CheckedMemoryOperationKind::AtomicExchange { .. } => Operation::AtomicExchange,
        CheckedMemoryOperationKind::AtomicCompareExchange { weak: false, .. } => {
            Operation::AtomicCompareExchange
        }
        CheckedMemoryOperationKind::AtomicCompareExchange { weak: true, .. } => {
            Operation::AtomicCompareExchangeWeak
        }
        CheckedMemoryOperationKind::AtomicFetch {
            kind: AtomicFetchKind::Add,
            ..
        } => Operation::AtomicFetchAdd,
        CheckedMemoryOperationKind::AtomicFetch {
            kind: AtomicFetchKind::Subtract,
            ..
        } => Operation::AtomicFetchSubtract,
        CheckedMemoryOperationKind::AtomicFetch {
            kind: AtomicFetchKind::And,
            ..
        } => Operation::AtomicFetchAnd,
        CheckedMemoryOperationKind::AtomicFetch {
            kind: AtomicFetchKind::Or,
            ..
        } => Operation::AtomicFetchOr,
        CheckedMemoryOperationKind::AtomicFetch {
            kind: AtomicFetchKind::Xor,
            ..
        } => Operation::AtomicFetchXor,
        CheckedMemoryOperationKind::AtomicWait { .. } => Operation::AtomicWait,
        CheckedMemoryOperationKind::AtomicNotify { all: false, .. } => Operation::AtomicNotifyOne,
        CheckedMemoryOperationKind::AtomicNotify { all: true, .. } => Operation::AtomicNotifyAll,
        _ => return None,
    })
}

pub(crate) fn classify_atomic_operation<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    arguments: &[GenericArgument],
    expression: BoundExpressionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<
    Option<CheckedMemoryOperationKind>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    use CheckedMemoryOperationKind as Kind;
    use ImplementationHook as Hook;

    let operation =
        crate::memory_diagnostics::diagnostic_memory_operation(hook).ok_or_else(|| {
            CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )
        })?;

    let parsed = match parse_atomic_arguments(request, arguments) {
        Some(parsed) => parsed,
        None => {
            add_invalid_atomic_order_diagnostic(request, expression, operation, diagnostics)?;

            return Ok(None);
        }
    };

    let Some(representation) = operation_representation(request, &parsed, diagnostics)? else {
        add_unavailable_atomic_operation_diagnostic(request, expression, operation, diagnostics)?;

        return Ok(None);
    };

    let available = match representation {
        AtomicRepresentationResolution::Known(representation) => {
            value_operation_allowed(hook, representation)
                && operation_available(request, hook, representation.target)
        }
        AtomicRepresentationResolution::OpenGeneric => true,
    };

    if !available {
        add_unavailable_atomic_operation_diagnostic(request, expression, operation, diagnostics)?;

        return Ok(None);
    }

    let kind = match (hook, parsed.as_slice()) {
        (Hook::AtomicInitialize, [AtomicGenericArgument::Type(value)]) => {
            Kind::AtomicInitialize { value: *value }
        }
        (
            Hook::AtomicLoad,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(order),
            ],
        ) if order.valid_for_load() => Kind::AtomicLoad {
            value: *value,
            order: *order,
        },
        (
            Hook::AtomicStore,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(order),
            ],
        ) if order.valid_for_store() => Kind::AtomicStore {
            value: *value,
            order: *order,
        },
        (
            Hook::AtomicExchange,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(order),
            ],
        ) => Kind::AtomicExchange {
            value: *value,
            order: *order,
        },
        (
            Hook::AtomicCompareExchange | Hook::AtomicCompareExchangeWeak,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(success),
                AtomicGenericArgument::Order(failure),
            ],
        ) if success.permits_failure(*failure) => Kind::AtomicCompareExchange {
            value: *value,
            weak: hook == Hook::AtomicCompareExchangeWeak,
            success: *success,
            failure: *failure,
        },
        (
            Hook::AtomicFetchAdd
            | Hook::AtomicFetchSub
            | Hook::AtomicFetchAnd
            | Hook::AtomicFetchOr
            | Hook::AtomicFetchXor,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(order),
            ],
        ) => Kind::AtomicFetch {
            value: *value,
            kind: fetch_kind(hook),
            order: *order,
        },
        (Hook::AtomicFence | Hook::AtomicCompilerFence, [AtomicGenericArgument::Order(order)])
            if order.valid_for_fence() =>
        {
            Kind::Fence {
                compiler_only: hook == Hook::AtomicCompilerFence,
                order: *order,
            }
        }
        (
            Hook::AtomicWait,
            [
                AtomicGenericArgument::Type(value),
                AtomicGenericArgument::Order(order),
            ],
        ) if order.valid_for_load() => Kind::AtomicWait {
            value: *value,
            order: *order,
        },
        (Hook::AtomicNotifyOne | Hook::AtomicNotifyAll, [AtomicGenericArgument::Type(value)]) => {
            Kind::AtomicNotify {
                value: *value,
                all: hook == Hook::AtomicNotifyAll,
            }
        }
        (hook, _) if atomic_hook(hook) => {
            add_invalid_atomic_order_diagnostic(request, expression, operation, diagnostics)?;

            return Ok(None);
        }
        (hook, _) => {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidAtomicOperationInput {
                    hook,
                    argument_count: parsed.len(),
                },
            ));
        }
    };

    Ok(Some(kind))
}

/// Returns the target atomic representation for one compiler-known value representation.
///
/// Target-sized integers use `pointer_width_bits` to select their integer storage width.
pub const fn atomic_target_representation(
    role: RepresentationRole,
    pointer_width_bits: u16,
) -> Option<TargetAtomicRepresentation> {
    match role {
        RepresentationRole::ScalarBool
        | RepresentationRole::ScalarI8
        | RepresentationRole::ScalarU8 => Some(TargetAtomicRepresentation::U8),
        RepresentationRole::ScalarI16 | RepresentationRole::ScalarU16 => {
            Some(TargetAtomicRepresentation::U16)
        }
        RepresentationRole::ScalarI32 | RepresentationRole::ScalarU32 => {
            Some(TargetAtomicRepresentation::U32)
        }
        RepresentationRole::ScalarI64 | RepresentationRole::ScalarU64 => {
            Some(TargetAtomicRepresentation::U64)
        }
        RepresentationRole::ScalarI128 | RepresentationRole::ScalarU128 => {
            Some(TargetAtomicRepresentation::U128)
        }
        RepresentationRole::ScalarIsize | RepresentationRole::ScalarUsize => {
            TargetAtomicRepresentation::for_storage_size((pointer_width_bits / 8) as u64)
        }
        RepresentationRole::RawPointer => Some(TargetAtomicRepresentation::Pointer),
        _ => None,
    }
}

const fn value_operation_allowed(
    hook: ImplementationHook,
    representation: AtomicValueRepresentation,
) -> bool {
    if !matches!(
        hook,
        ImplementationHook::AtomicFetchAdd
            | ImplementationHook::AtomicFetchSub
            | ImplementationHook::AtomicFetchAnd
            | ImplementationHook::AtomicFetchOr
            | ImplementationHook::AtomicFetchXor
    ) {
        return true;
    }

    representation.integer
}

#[derive(Clone, Copy)]
struct AtomicValueRepresentation {
    target: TargetAtomicRepresentation,
    integer: bool,
}

#[derive(Clone, Copy)]
enum AtomicRepresentationResolution {
    Known(AtomicValueRepresentation),
    OpenGeneric,
}

#[derive(Clone, Copy)]
enum AtomicGenericArgument {
    Type(TypeId),
    Order(MemoryOrder),
}

fn parse_atomic_arguments<C>(
    request: CheckerUnitView<'_, C>,
    arguments: &[GenericArgument],
) -> Option<Vec<AtomicGenericArgument>>
where
    C: CheckerRequestContext + ?Sized,
{
    arguments
        .iter()
        .map(|argument| match argument {
            GenericArgument::Type(ty) => Some(AtomicGenericArgument::Type(*ty)),
            GenericArgument::Constant(term) => {
                let value = request.semantic_values().constant_term_integer(*term);

                value
                    .and_then(|value| value.to_u64())
                    .and_then(MemoryOrder::from_u64)
                    .map(AtomicGenericArgument::Order)
            }
        })
        .collect()
}

fn operation_representation<C>(
    request: CheckerUnitView<'_, C>,
    arguments: &[AtomicGenericArgument],
    diagnostics: &mut DiagnosticBag,
) -> Result<
    Option<AtomicRepresentationResolution>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(value) = atomic_value_type(arguments) else {
        return Ok(Some(AtomicRepresentationResolution::Known(
            AtomicValueRepresentation {
                target: TargetAtomicRepresentation::U8,
                integer: false,
            },
        )));
    };

    atomic_value_representation(request, value, diagnostics, &mut BTreeSet::new())
}

fn atomic_value_representation<C>(
    request: CheckerUnitView<'_, C>,
    value: TypeId,
    diagnostics: &mut DiagnosticBag,
    pending: &mut BTreeSet<TypeId>,
) -> Result<
    Option<AtomicRepresentationResolution>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    if !pending.insert(value) {
        return Ok(None);
    }

    let role = crate::representation::type_representation(request, value);

    if let Some(role) = role {
        pending.remove(&value);

        return Ok(atomic_target_representation(
            role,
            request
                .selected_target()
                .machine()
                .pointer_width_bits()
                .get(),
        )
        .map(|target| {
            AtomicRepresentationResolution::Known(AtomicValueRepresentation {
                target,
                integer: role.integer_representation().is_some(),
            })
        }));
    }

    let data = request.semantic_values().type_data(value);

    if matches!(data.as_ref(), TypeData::TypeParameter(_)) {
        pending.remove(&value);

        return Ok(Some(AtomicRepresentationResolution::OpenGeneric));
    }

    let TypeData::Named {
        definition,
        substitution,
    } = data.as_ref()
    else {
        pending.remove(&value);

        return Ok(None);
    };

    let representation = request
        .declared_type_representation(*definition)
        .map_err(atomic_query_outcome)?;

    // The support remains available to other consumers, so retain its diagnostics here.
    diagnostics.add_range(representation.diagnostics().clone());
    let representation = representation.value();

    if representation.is_recovered() || !representation.has_finite_size() {
        pending.remove(&value);

        return Ok(None);
    }

    let result = if representation.layout() == DeclaredLayoutMode::Transparent {
        transparent_atomic_representation(
            request,
            representation.storage(),
            *substitution,
            diagnostics,
            pending,
        )?
    } else if representation.is_plain_storage() {
        request
            .plain_storage_atomic_representation(value)
            .map_err(atomic_query_outcome)?
            .map(|target| {
                AtomicRepresentationResolution::Known(AtomicValueRepresentation {
                    target,
                    integer: false,
                })
            })
    } else {
        None
    };

    pending.remove(&value);

    Ok(result)
}

fn transparent_atomic_representation<C>(
    request: CheckerUnitView<'_, C>,
    storage: &DeclaredStorageShape,
    substitution: GenericSubstitutionId,
    diagnostics: &mut DiagnosticBag,
    pending: &mut BTreeSet<TypeId>,
) -> Result<
    Option<AtomicRepresentationResolution>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let DeclaredStorageShape::Structure(members) = storage else {
        return Ok(None);
    };

    let [member] = members.as_ref() else {
        return Ok(None);
    };

    let checked = crate::constant::checked_substituted_type(request, member.ty(), substitution)
        .map_err(atomic_query_outcome)?;

    let (member_type, checked_diagnostics) = checked.into_parts();

    diagnostics.add_range(checked_diagnostics);

    let Some(member_type) = member_type else {
        return Ok(None);
    };

    atomic_value_representation(request, member_type, diagnostics, pending)
}

fn atomic_query_outcome<Upstream>(
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

fn atomic_value_type(arguments: &[AtomicGenericArgument]) -> Option<TypeId> {
    arguments.iter().find_map(|argument| match argument {
        AtomicGenericArgument::Type(value) => Some(*value),
        AtomicGenericArgument::Order(_) => None,
    })
}

const fn fetch_kind(hook: ImplementationHook) -> AtomicFetchKind {
    match hook {
        ImplementationHook::AtomicFetchAdd => AtomicFetchKind::Add,
        ImplementationHook::AtomicFetchSub => AtomicFetchKind::Subtract,
        ImplementationHook::AtomicFetchAnd => AtomicFetchKind::And,
        ImplementationHook::AtomicFetchOr => AtomicFetchKind::Or,
        ImplementationHook::AtomicFetchXor => AtomicFetchKind::Xor,
        _ => panic!("atomic fetch hook required"),
    }
}

fn operation_available<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    representation: TargetAtomicRepresentation,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    use ImplementationHook as Hook;

    let support = request
        .selected_target()
        .properties()
        .atomics()
        .representation(representation);

    let operations = support.operations();

    match hook {
        Hook::AtomicInitialize | Hook::AtomicLoad | Hook::AtomicStore => operations.load_store(),
        Hook::AtomicExchange => operations.exchange(),
        Hook::AtomicCompareExchange | Hook::AtomicCompareExchangeWeak => {
            operations.compare_exchange()
        }
        Hook::AtomicFetchAdd | Hook::AtomicFetchSub => operations.fetch_arithmetic(),
        Hook::AtomicFetchAnd | Hook::AtomicFetchOr | Hook::AtomicFetchXor => {
            operations.fetch_bitwise()
        }
        Hook::AtomicWait | Hook::AtomicNotifyOne | Hook::AtomicNotifyAll => support.wait_notify(),
        Hook::AtomicFence | Hook::AtomicCompilerFence => {
            request.selected_target().properties().atomics().any()
        }
        _ => false,
    }
}

fn add_invalid_atomic_order_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    operation: DiagnosticMemoryOperation,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let span =
        expression_span(request, expression).map_err(CheckerOutcome::InfrastructureFailure)?;

    diagnostics.add(
        Diagnostic::new(
            diagnostic_id(diagnostics.len()),
            DiagnosticKind::CheckingInvalidAtomicMemoryOrder,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::MemoryOperationFailure,
            span,
        ))
        .with_arg(DiagnosticArg::memory_operation(operation)),
    );

    Ok(())
}

fn add_unavailable_atomic_operation_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    operation: DiagnosticMemoryOperation,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let span =
        expression_span(request, expression).map_err(CheckerOutcome::InfrastructureFailure)?;

    crate::memory_diagnostics::add_target_memory_operation_unavailable(
        span,
        request.selected_target().identity().as_str(),
        operation,
        diagnostics,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::RepresentationRole;
    use bray_target::TargetAtomicRepresentation;

    use super::atomic_target_representation;

    #[test]
    fn language_scalar_roles_map_to_exact_target_atomic_representations() {
        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarBool, 64),
            Some(TargetAtomicRepresentation::U8)
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarI32, 64),
            Some(TargetAtomicRepresentation::U32)
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarU128, 64),
            Some(TargetAtomicRepresentation::U128)
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::RawPointer, 64),
            Some(TargetAtomicRepresentation::Pointer)
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarR32, 64),
            None
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarUsize, 32),
            Some(TargetAtomicRepresentation::U32)
        );

        assert_eq!(
            atomic_target_representation(RepresentationRole::ScalarIsize, 64),
            Some(TargetAtomicRepresentation::U64)
        );
    }
}
