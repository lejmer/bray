use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::StorageProjection;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{DeclaredStorageShape, DependencyProjection, TypeData, TypeId};

/// Resolves one checked value projection while preserving declaration diagnostics and substitution.
pub(crate) fn projected_value_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    owner: TypeId,
    projection: DependencyProjection,
) -> Result<DiagnosticResult<Option<TypeId>>, CheckerQueryError<C::UpstreamError>> {
    let owner = request.semantic_values().unborrowed_type(owner);

    if projection == DependencyProjection::UninitPayload {
        return Ok(DiagnosticResult::new(
            request
                .available_compiler_known_symbols()
                .unary_representation_argument(
                    request.semantic_values(),
                    bray_compiler_known::RepresentationRole::Uninit,
                    owner,
                ),
            DiagnosticBag::new(),
        ));
    }

    let data = request.semantic_values().type_data(owner);

    let mut diagnostics = DiagnosticBag::new();

    let ty = match (data.as_ref(), projection) {
        (TypeData::OwnedIndirection { target, .. }, DependencyProjection::OwnedTarget) => {
            Some(*target)
        }
        (TypeData::Nullable(target), DependencyProjection::NullableValue) => Some(*target),
        (TypeData::Array { .. } | TypeData::Slice(_), DependencyProjection::Element(_)) => {
            sequence_element_type(request, owner)
        }
        (TypeData::Tuple(elements), DependencyProjection::TupleElement(index)) => {
            usize::try_from(index.raw())
                .ok()
                .and_then(|index| elements.get(index))
                .copied()
        }
        (
            TypeData::Named {
                definition,
                substitution,
            },
            projection,
        ) => {
            let representation = request.declared_type_representation(*definition)?;

            let (representation, declaration_diagnostics) = representation.into_parts();

            diagnostics.add_range(declaration_diagnostics);

            let template = match (representation.storage(), projection) {
                (
                    DeclaredStorageShape::Structure(members),
                    DependencyProjection::ProductField(field),
                ) => members
                    .iter()
                    .find(|member| member.field() == Some(field))
                    .map(|member| member.ty()),
                (
                    DeclaredStorageShape::Union(variants),
                    DependencyProjection::UnionPayloadField(field),
                ) => variants
                    .iter()
                    .flat_map(|variant| variant.members())
                    .find(|member| member.field() == Some(field))
                    .map(|member| member.ty()),
                _ => None,
            };

            if let Some(template) = template {
                let checked =
                    crate::constant::checked_substituted_type(request, template, *substitution)?;

                let (ty, type_diagnostics) = checked.into_parts();

                diagnostics.add_range(type_diagnostics);

                ty
            } else {
                None
            }
        }
        _ => None,
    };

    Ok(DiagnosticResult::new(ty, diagnostics))
}

/// Prepares the dereference steps of a checked projection path once for its consumers.
/// A selected access can additionally require dereferencing the final value to its reached type.
pub(crate) fn reference_path<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    mut ty: TypeId,
    projections: &[bray_bound_tree::StorageProjection],
    reached: Option<TypeId>,
) -> Result<Option<(TypeId, Vec<(usize, TypeId)>)>, CheckerQueryError<C::UpstreamError>> {
    let mut borrows = Vec::new();

    for index in 0..=projections.len() {
        while let TypeData::Borrow { target, .. } = request.semantic_values().type_data(ty).as_ref()
        {
            if index == projections.len() && reached.is_none_or(|reached| reached == ty) {
                break;
            }

            ty = *target;
            borrows.push((index, ty));
        }

        if let Some(projection) = projections.get(index) {
            let projected = projected_storage_type(request, ty, *projection)?;

            let Some(projected) = *projected.value() else {
                return Ok(None);
            };

            ty = projected;
        }
    }

    Ok(Some((ty, borrows)))
}

pub(crate) fn storage_projections<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    projections: &[DependencyProjection],
) -> Option<Vec<StorageProjection>> {
    projections
        .iter()
        .map(|projection| match projection {
            DependencyProjection::ProductField(field) => {
                Some(StorageProjection::ProductField(*field))
            }
            DependencyProjection::TupleElement(index) => {
                Some(StorageProjection::TupleElement(*index))
            }
            DependencyProjection::NullableValue => Some(StorageProjection::NullableValue),
            DependencyProjection::OwnedTarget => Some(StorageProjection::OwnedTarget),
            DependencyProjection::UninitPayload => Some(StorageProjection::UninitPayload),
            DependencyProjection::UnionPayloadField(field) => {
                let field = request
                    .symbols()
                    .union_payload_field(*field)
                    .expect("checked field projection retains its declaration");

                Some(StorageProjection::ActiveUnionPayloadField {
                    variant: field.variant(),
                    field: field.id(),
                })
            }
            DependencyProjection::Element(_) => None,
        })
        .collect::<Option<Vec<_>>>()
}

fn projected_storage_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    owner: TypeId,
    projection: StorageProjection,
) -> Result<DiagnosticResult<Option<TypeId>>, CheckerQueryError<C::UpstreamError>> {
    let projection = match projection {
        StorageProjection::ProductField(field) => DependencyProjection::ProductField(field),
        StorageProjection::TupleElement(index) => DependencyProjection::TupleElement(index),
        StorageProjection::NullableValue => DependencyProjection::NullableValue,
        StorageProjection::OwnedTarget => DependencyProjection::OwnedTarget,
        StorageProjection::UninitPayload => DependencyProjection::UninitPayload,
        StorageProjection::ActiveUnionPayloadField { field, .. } => {
            DependencyProjection::UnionPayloadField(field)
        }
        StorageProjection::Element(_)
        | StorageProjection::ElementFromStart(_)
        | StorageProjection::ElementFromEnd(_) => {
            return Ok(DiagnosticResult::new(
                sequence_element_type(request, owner),
                DiagnosticBag::new(),
            ));
        }
        StorageProjection::SliceRange { .. } => {
            return Ok(DiagnosticResult::new(None, DiagnosticBag::new()));
        }
    };

    projected_value_type(request, owner, projection)
}

fn sequence_element_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    owner: TypeId,
) -> Option<TypeId> {
    match request
        .semantic_values()
        .type_data(request.semantic_values().unborrowed_type(owner))
        .as_ref()
    {
        TypeData::Array { element, .. } | TypeData::Slice(element) => Some(*element),
        _ => None,
    }
}

/// Canonicalizes built-in literal selectors at storage planning, before alias analysis.
pub(super) fn element_projection<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    literals: &bray_bound_tree::CheckedLiteralValues,
    selector: bray_bound_tree::BoundExpressionId,
) -> StorageProjection {
    let index = literals.expression(selector).and_then(|value| {
        let value = request.semantic_values().constant_value_data(value);

        match value.kind() {
            bray_symbols::ConstantValueKind::Integer(index) => {
                index.to_u64().and_then(|index| u32::try_from(index).ok())
            }
            _ => None,
        }
    });

    match index {
        Some(index) => StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index)),
        None => StorageProjection::Element(selector),
    }
}
