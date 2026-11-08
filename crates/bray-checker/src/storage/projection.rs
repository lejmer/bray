use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::StorageProjection;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{DependencyProjection, TypeData, TypeId};

/// Selects a pattern component relative to its already evaluated expression.
pub(crate) fn expression_storage_projections(
    storage: &bray_bound_tree::StoragePlan,
    expression: bray_bound_tree::BoundExpressionId,
    access: bray_bound_tree::StorageAccessId,
) -> Option<&[StorageProjection]> {
    let source = storage.node_plans(expression.into())
        .filter(|source| storage.access_contains(source.access(), access))
        .max_by_key(|source| storage.resolved_projections(source.access()).map_or(0, <[_]>::len));

    let source_path = source.and_then(|source| storage.resolved_projections(source.access()))
        .unwrap_or_default();

    storage.resolved_projections(access)?.strip_prefix(source_path)
}

/// The present-value projections introduced by a checked nullable adaptation.
pub(crate) fn nullable_wrapping(
    values: &bray_symbols::SemanticValueStore,
    source: TypeId,
    mut target: TypeId,
) -> Option<Vec<StorageProjection>> {
    let mut projections = Vec::new();

    while target != source {
        let data = values.type_data(target);

        let TypeData::Nullable(inner) = data.as_ref() else {
            return None;
        };

        projections.push(StorageProjection::NullableValue);
        target = *inner;
    }

    Some(projections)
}

/// Selects caller storage through the wrappers introduced by a checked input conversion.
/// Selecting the whole new wrapper retains the underlying value's dependencies.
pub(crate) fn unadapted_projections<'p>(
    values: &bray_symbols::SemanticValueStore,
    conversion: Option<&bray_bound_tree::SelectedConversion>,
    mut projections: &'p [DependencyProjection],
) -> Option<&'p [DependencyProjection]> {
    let Some(conversion) = conversion.filter(|conversion| {
        matches!(conversion.target(), bray_bound_tree::ConversionTarget::NullablePresent)
    }) else { return Some(projections); };

    let mut target = conversion.target_type();

    while target != conversion.source_type() {
        let TypeData::Nullable(inner) = *values.type_data(target) else {
            panic!("checked nullable adaptation retains its introduced layers: {conversion:?}");
        };

        if projections.is_empty() { return Some(projections); }

        projections = projections.strip_prefix(&[DependencyProjection::NullableValue])?;
        target = inner;
    }

    Some(projections)
}

/// Resolves physical storage only when it denotes a logical value in this plan.
/// A raw pointer's incoming pointee is distinct from the pointer value itself.
pub(crate) fn storage_observation_path(
    storage: &bray_bound_tree::StoragePlan,
    values: &bray_symbols::SemanticValueStore,
    target: &bray_bound_tree::StorageTarget,
) -> Option<bray_bound_tree::StoragePath> {
    if let bray_bound_tree::StorageOrigin::Entry { reference, .. } = &target.origin
        && !storage
            .storage_type(reference.root)
            .is_some_and(|ty| matches!(values.type_data(ty).as_ref(), TypeData::Borrow { .. }))
    {
        return None;
    }

    target.observation_path()
}

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
        (
            TypeData::Named {
                definition,
                substitution,
            },
            DependencyProjection::OwnedTarget,
        ) => request
            .context()
            .raw_buffer_element(*definition, *substitution)?
            .map(|element| {
                // This is the allocation's storage shape, not proof of an initialized slice.
                request
                    .semantic_values()
                    .intern_type(TypeData::Slice(element))
                    .expect("recognized buffer element has a slice storage type")
            }),
        (TypeData::Generator(element), DependencyProjection::OwnedTarget) => Some(
            request
                .semantic_values()
                .intern_type(TypeData::Slice(*element))
                .expect("generator owns contiguous element storage"),
        ),
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
            let member = match (*definition, projection) {
                (
                    bray_symbols::NamedTypeSymbolId::Struct(structure),
                    DependencyProjection::ProductField(field),
                ) if request
                    .context()
                    .struct_field(field)?
                    .is_some_and(|field| field.containing_symbol() == structure.into()) =>
                {
                    Some(field.into())
                }
                (
                    bray_symbols::NamedTypeSymbolId::Union(union),
                    DependencyProjection::UnionPayloadField(field),
                ) => {
                    let variant = request
                        .context()
                        .union_payload_field(field)?
                        .and_then(|field| match field.containing_symbol() {
                            bray_symbols::AnySymbolId::UnionVariant(variant) => Some(variant),
                            _ => None,
                        });

                    match variant {
                        Some(variant)
                            if request.context().union_variant(variant)?.is_some_and(
                                |variant| variant.containing_symbol() == union.into(),
                            ) =>
                        {
                            Some(field.into())
                        }
                        _ => None,
                    }
                }
                _ => None,
            };

            let queried = member
                .map(|member| request.context().storage_member_type(member))
                .transpose()?;

            let template = queried.as_ref().map(|queried| {
                diagnostics.add_range(queried.diagnostics().iter().cloned());

                queried.value()
            });

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
    projections: std::sync::Arc<[bray_bound_tree::StorageProjection]>,
    reached: Option<TypeId>,
) -> Result<
    Option<(TypeId, bray_bound_tree::StorageReferencePath)>,
    CheckerQueryError<C::UpstreamError>,
> {
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

    Ok(Some((
        ty,
        bray_bound_tree::StorageReferencePath::new(projections, borrows.into()),
    )))
}

pub(crate) fn resolve_storage_projections<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    projections: &[DependencyProjection],
) -> crate::CheckerQueryResult<Option<Vec<StorageProjection>>, C::UpstreamError> {
    convert_projections(request.semantic_values(), projections, |field| {
        Ok(request
            .union_payload_field(field)?
            .expect("checked field projection retains its declaration")
            .variant())
    })
}

pub(crate) fn owned_storage_projections(
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    projections: &[bray_symbols::ConstantProjectionKind],
) -> Vec<StorageProjection> {
    use bray_symbols::ConstantProjectionKind;

    projections
        .iter()
        .map(|projection| match *projection {
            ConstantProjectionKind::ProductField(field) => StorageProjection::ProductField(field),
            ConstantProjectionKind::UnionPayloadField(field) => {
                StorageProjection::ActiveUnionPayloadField {
                    variant: dependencies.projection_variant(field),
                    field,
                }
            }
            ConstantProjectionKind::NullableValue => StorageProjection::NullableValue,
            ConstantProjectionKind::TupleElement(ordinal) => {
                StorageProjection::TupleElement(ordinal)
            }
            ConstantProjectionKind::ArrayElementOrdinal(ordinal) => {
                StorageProjection::ElementFromStart(ordinal)
            }
            _ => unreachable!("checked transfer contains only owning projections"),
        })
        .collect()
}

pub(crate) fn storage_projections(
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    projections: &[DependencyProjection],
) -> Option<Vec<StorageProjection>> {
    convert_projections(values, projections, |field| {
        Ok::<_, std::convert::Infallible>(dependencies.projection_variant(field))
    })
    .unwrap_or_else(|error| match error {})
}

fn convert_projections<E>(
    values: &bray_symbols::SemanticValueStore,
    projections: &[DependencyProjection],
    mut variant: impl FnMut(
        bray_symbols::UnionPayloadFieldSymbolId,
    ) -> Result<bray_symbols::UnionVariantSymbolId, E>,
) -> Result<Option<Vec<StorageProjection>>, E> {
    let mut result = Vec::with_capacity(projections.len());

    for projection in projections {
        result.push(match *projection {
            DependencyProjection::ProductField(field) => StorageProjection::ProductField(field),
            DependencyProjection::TupleElement(index) => StorageProjection::TupleElement(index),
            DependencyProjection::NullableValue => StorageProjection::NullableValue,
            DependencyProjection::OwnedTarget => StorageProjection::OwnedTarget,
            DependencyProjection::UninitPayload => StorageProjection::UninitPayload,
            DependencyProjection::UnionPayloadField(field) => {
                StorageProjection::ActiveUnionPayloadField {
                    variant: variant(field)?,
                    field,
                }
            }
            DependencyProjection::Element(term) => {
                let term = values.constant_term_data(term);

                let bray_symbols::ConstantTermData::Value(value) = term.as_ref() else {
                    return Ok(None);
                };

                let value = values.constant_value_data(*value);

                let bray_symbols::ConstantValueKind::Integer(index) = value.kind() else {
                    return Ok(None);
                };

                let Some(index) = index.to_u64().and_then(|index| u32::try_from(index).ok()) else {
                    return Ok(None);
                };

                StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index))
            }
        });
    }

    Ok(Some(result))
}

/// Resolves a checked storage component using its established declaration and substitution.
pub fn projected_storage_type<C: CheckerRequestContext + ?Sized>(
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
            let slice = sequence_element_type(request, owner).map(|element| {
                request
                    .semantic_values()
                    .intern_type(TypeData::Slice(element))
                    .expect("checked sequence element belongs to the same semantic store")
            });

            return Ok(DiagnosticResult::new(slice, DiagnosticBag::new()));
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

pub(crate) fn dependency_projections(
    projections: &[bray_bound_tree::StorageProjection],
) -> Option<Vec<bray_symbols::DependencyProjection>> {
    projections
        .iter()
        .copied()
        .map(dependency_projection)
        .collect()
}

pub(crate) fn dependency_projection(
    projection: bray_bound_tree::StorageProjection,
) -> Option<bray_symbols::DependencyProjection> {
    use bray_bound_tree::StorageProjection;
    use bray_symbols::DependencyProjection;

    match projection {
        StorageProjection::ProductField(field) => Some(DependencyProjection::ProductField(field)),
        StorageProjection::TupleElement(index) => Some(DependencyProjection::TupleElement(index)),
        StorageProjection::NullableValue => Some(DependencyProjection::NullableValue),
        StorageProjection::ActiveUnionPayloadField { field, .. } => {
            Some(DependencyProjection::UnionPayloadField(field))
        }
        StorageProjection::OwnedTarget => Some(DependencyProjection::OwnedTarget),
        StorageProjection::UninitPayload => Some(DependencyProjection::UninitPayload),
        _ => None,
    }
}
