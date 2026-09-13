use super::plan::{PlanError, Planner};
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::StorageProjection;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{DeclaredStorageShape, DependencyProjection, TypeData, TypeId};

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn projected_storage_type(
        &mut self,
        owner: TypeId,
        projection: StorageProjection,
    ) -> Result<TypeId, PlanError<C::UpstreamError>> {
        let projection = match projection {
            StorageProjection::OwnedTarget => DependencyProjection::OwnedTarget,
            StorageProjection::NullableValue => DependencyProjection::NullableValue,
            StorageProjection::TupleElement(index) => DependencyProjection::TupleElement(index),
            StorageProjection::ProductField(field) => DependencyProjection::ProductField(field),
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                DependencyProjection::UnionPayloadField(field)
            }
            _ => return Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
        };

        let checked = projected_value_type(self.request, owner, projection)?;

        let (ty, diagnostics) = checked.into_parts();

        self.diagnostics.add_range(diagnostics);

        ty.ok_or_else(|| CheckerInfrastructureError::InvalidStoragePlan.into())
    }
}

/// Resolves one checked value projection while preserving declaration diagnostics and substitution.
pub(crate) fn projected_value_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    owner: TypeId,
    projection: DependencyProjection,
) -> Result<DiagnosticResult<Option<TypeId>>, CheckerQueryError<C::UpstreamError>> {
    let owner = request
        .semantic_values()
        .unborrowed_type(owner)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let data = request
        .semantic_values()
        .type_data(owner)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let mut diagnostics = DiagnosticBag::new();

    let ty = match (data.as_ref(), projection) {
        (TypeData::OwnedIndirection { target, .. }, DependencyProjection::OwnedTarget) => {
            Some(*target)
        }
        (TypeData::Nullable(target), DependencyProjection::NullableValue) => Some(*target),
        (
            TypeData::Array { element, .. } | TypeData::Slice(element),
            DependencyProjection::Element(_),
        ) => Some(*element),
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
