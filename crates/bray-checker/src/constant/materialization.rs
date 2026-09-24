use std::collections::BTreeSet;

use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{ConstantValueId, ConstantValueKind, TypeData, TypeId};

use crate::representation::type_representation_for_context;
use crate::{CheckerQueryResult, CheckerRequestContext};

pub(super) fn nonmaterializable_value<C: CheckerRequestContext + ?Sized>(
    context: &C,
    value: ConstantValueId,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<Option<TypeId>, C::UpstreamError> {
    let values = context.semantic_values();
    let data = values.constant_value_data(value);

    if matches!(data.kind(), ConstantValueKind::Error) {
        return Ok(None);
    }

    nonmaterializable_type(context, data.ty(), diagnostics)
}

pub(super) fn nonmaterializable_type<C: CheckerRequestContext + ?Sized>(
    context: &C,
    ty: TypeId,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<Option<TypeId>, C::UpstreamError> {
    let values = context.semantic_values();
    let type_data = values.type_data(ty);

    match type_data.as_ref() {
        TypeData::Named { definition, .. } => {
            let role = type_representation_for_context(context, ty);

            if matches!(
                role,
                Some(
                    RepresentationRole::RawPointer
                        | RepresentationRole::DevicePointer
                        | RepresentationRole::Atomic
                        | RepresentationRole::Uninit
                        | RepresentationRole::RunResult
                        | RepresentationRole::PanicReport
                        | RepresentationRole::Future
                        | RepresentationRole::Task
                )
            ) {
                return Ok(Some(ty));
            }

            if role.is_some() {
                return Ok(None);
            }

            let lifecycle = context.declared_type_has_lifecycle(*definition)?;
            diagnostics.add_range(lifecycle.diagnostics().iter().cloned());

            Ok((!lifecycle.diagnostics().has_errors() && *lifecycle.value()).then_some(ty))
        }
        TypeData::Borrow { .. }
        | TypeData::OwnedIndirection { .. }
        | TypeData::Generator(_)
        | TypeData::Slice(_)
        | TypeData::FlexibleArray(_)
        | TypeData::TraitView(_) => Ok(Some(ty)),
        TypeData::Error
        | TypeData::Tuple(_)
        | TypeData::Array { .. }
        | TypeData::Nullable(_)
        | TypeData::Callable(_)
        | TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. } => Ok(None),
    }
}

pub(super) fn nonmaterializable_value_tree<C: CheckerRequestContext + ?Sized>(
    context: &C,
    root: ConstantValueId,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<Option<TypeId>, C::UpstreamError> {
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();

    while let Some(value) = pending.pop() {
        if !visited.insert(value) {
            continue;
        }

        if let Some(ty) = nonmaterializable_value(context, value, diagnostics)? {
            return Ok(Some(ty));
        }

        let data = context.semantic_values().constant_value_data(value);

        match data.kind() {
            ConstantValueKind::NullablePresent(value) => pending.push(*value),
            ConstantValueKind::Tuple(values) | ConstantValueKind::Array(values) => {
                pending.extend(values.iter().copied());
            }
            ConstantValueKind::Product(fields) => {
                pending.extend(fields.iter().map(|field| *field.value()));
            }
            ConstantValueKind::Union { fields, .. } => {
                pending.extend(fields.iter().map(|field| *field.value()));
            }
            ConstantValueKind::Error
            | ConstantValueKind::Boolean(_)
            | ConstantValueKind::Character(_)
            | ConstantValueKind::Integer(_)
            | ConstantValueKind::Real(_)
            | ConstantValueKind::Complex { .. }
            | ConstantValueKind::String(_)
            | ConstantValueKind::StaticAddress(_)
            | ConstantValueKind::Unit
            | ConstantValueKind::NullableAbsent => {}
        }
    }

    Ok(None)
}
