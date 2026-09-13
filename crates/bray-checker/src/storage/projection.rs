use super::plan::{PlanError, Planner};
use crate::{CheckerInfrastructureError, CheckerRequestContext};
use bray_bound_tree::StorageProjection;
use bray_symbols::{DeclaredStorageShape, TypeData, TypeId};

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn projected_storage_type(
        &mut self,
        owner: TypeId,
        projection: StorageProjection,
    ) -> Result<TypeId, PlanError<C::UpstreamError>> {
        let owner = self
            .request
            .semantic_values()
            .unborrowed_type(owner)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let data = self
            .request
            .semantic_values()
            .type_data(owner)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let ty = match (data.as_ref(), projection) {
            (TypeData::OwnedIndirection { target, .. }, StorageProjection::OwnedTarget) => {
                Some(*target)
            }
            (TypeData::Nullable(target), StorageProjection::NullableValue) => Some(*target),
            (TypeData::Tuple(elements), StorageProjection::TupleElement(index)) => {
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
                let representation = self.request.declared_type_representation(*definition)?;

                let (representation, diagnostics) = representation.into_parts();

                self.diagnostics.add_range(diagnostics);

                let template = match (representation.storage(), projection) {
                    (
                        DeclaredStorageShape::Structure(members),
                        StorageProjection::ProductField(field),
                    ) => members
                        .iter()
                        .find(|member| member.field() == Some(field))
                        .map(|member| member.ty()),
                    (
                        DeclaredStorageShape::Union(variants),
                        StorageProjection::ActiveUnionPayloadField { variant, field },
                    ) => variants
                        .iter()
                        .find(|candidate| candidate.variant() == variant)
                        .and_then(|variant| {
                            variant
                                .members()
                                .iter()
                                .find(|member| member.field() == Some(field))
                        })
                        .map(|member| member.ty()),
                    _ => None,
                }
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                let checked = crate::constant::checked_substituted_type(
                    self.request,
                    template,
                    *substitution,
                )?;

                let (ty, diagnostics) = checked.into_parts();

                self.diagnostics.add_range(diagnostics);

                ty
            }
            _ => None,
        };

        ty.ok_or_else(|| CheckerInfrastructureError::InvalidStoragePlan.into())
    }
}
