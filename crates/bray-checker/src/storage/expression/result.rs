use std::collections::BTreeSet;

use bray_bound_tree::{BoundExpressionId, SelectedCall, StorageAccessId, StorageProjection};
use bray_symbols::{DependencyProjection, DependencyRequirement, DependencySubjectRoot, TypeData};

use super::super::plan::{PlanError, Planner};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn returned_borrow_access(
        &mut self,
        expression: BoundExpressionId,
        call: &SelectedCall,
    ) -> Result<Option<StorageAccessId>, PlanError<C::UpstreamError>> {
        let ty = self.expression_type(expression)?.ty();

        let data = self.request.semantic_values().type_data(ty);

        let TypeData::Borrow { kind, target } = data.as_ref() else {
            return Ok(None);
        };

        let (kind, target) = (*kind, *target);

        let template = crate::dependency::call_result_template(self.request, call, |callable| {
            self.request
                .context()
                .callable_result_dependencies(callable)
        })?;

        let sources: BTreeSet<_> = template
            .template
            .requirements()
            .iter()
            .filter_map(|requirement| match requirement {
                DependencyRequirement::Direct { subject, .. } => Some(subject),
                DependencyRequirement::Guarded(_)
                | DependencyRequirement::ResultCall { .. }
                | DependencyRequirement::FixedPoint { .. }
                | DependencyRequirement::Variable { .. } => None,
            })
            .collect();

        let mut sources = sources.into_iter();

        let Some(source) = sources.next() else {
            return Ok(None);
        };

        if sources.next().is_some() {
            return Ok(None);
        }

        if source.subject_root() == DependencySubjectRoot::Receiver
            && call.receiver().is_none_or(|receiver| {
                !matches!(
                    receiver.mode(),
                    bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
                )
            })
        {
            return Ok(None);
        }

        let argument = crate::dependency::result_argument(call, source.subject_root());

        let Some(argument) = argument else {
            return Ok(None);
        };

        let argument_type = self
            .request
            .semantic_values()
            .type_data(self.expression_type(argument)?.ty());

        if matches!(source.subject_root(), DependencySubjectRoot::Parameter(_))
            && !matches!(argument_type.as_ref(), TypeData::Borrow { .. })
        {
            return Ok(None);
        }

        self.retain_returned_borrow(
            expression,
            bray_bound_tree::StorageIdentity::Temporary(expression),
            ty,
            kind,
            target,
            argument,
            source.projections(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "returned-borrow retention requires its actual owner, input path and closed borrow types"
    )]
    pub(super) fn retain_returned_borrow(
        &mut self,
        expression: BoundExpressionId,
        identity: bray_bound_tree::StorageIdentity,
        ty: bray_symbols::TypeId,
        kind: bray_symbols::BorrowKind,
        target: bray_symbols::TypeId,
        argument: BoundExpressionId,
        projections: &[DependencyProjection],
    ) -> Result<Option<StorageAccessId>, PlanError<C::UpstreamError>> {
        let argument = self.plan_expression(argument, None)?;
        let mut access = self.borrowed_value_access(expression, argument)?;

        for projection in projections {
            let projection = match projection {
                DependencyProjection::ProductField(field) => {
                    StorageProjection::ProductField(*field)
                }
                DependencyProjection::TupleElement(ordinal) => {
                    StorageProjection::TupleElement(*ordinal)
                }
                DependencyProjection::OwnedTarget => {
                    let owner = self
                        .builder()?
                        .access(access)
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?
                        .reached_type();

                    if !self.plan_owned_borrows(owner)? {
                        return Ok(None);
                    }

                    StorageProjection::OwnedTarget
                }
                DependencyProjection::NullableValue => StorageProjection::NullableValue,
                DependencyProjection::UnionPayloadField(field) => {
                    let record = self
                        .request
                        .union_payload_field(*field)?
                        .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

                    StorageProjection::ActiveUnionPayloadField {
                        variant: record.variant(),
                        field: *field,
                    }
                }
                DependencyProjection::Element(_) => return Ok(None),
            };

            let owner = self
                .builder()?
                .access(access)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?
                .reached_type();

            let ty = self.projected_storage_type(owner, projection)?;

            access = self.project_access(
                expression,
                access,
                projection,
                bray_bound_tree::ExpressionTypeResult::new(
                    ty,
                    self.expression_type(expression)?.status(),
                ),
            )?;
        }

        let builder = self.builder()?;

        let creates_capability = builder.access(access).is_some_and(|access| {
            !access.projections().is_empty()
                || access.root().borrow_capability().is_none_or(|capability| {
                    builder
                        .borrow_capability(capability)
                        .expect("returned borrow references a planned capability")
                        .kind()
                        != kind
                })
        });

        if creates_capability {
            let source = self.access_with_reached_type(expression, access, target)?;

            access = self.borrow_access(expression, source, kind, ty)?;
        }

        let capability = self
            .builder()?
            .access(access)
            .and_then(|access| access.root().borrow_capability())
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let access = self.retain_borrow_value(
            expression,
            capability,
            identity,
            ty,
            bray_bound_tree::ExpressionTypeResult::new(
                ty,
                self.expression_type(expression)?.status(),
            ),
        )?;

        if creates_capability {
            let node = match identity {
                bray_bound_tree::StorageIdentity::ScopedCapability { pattern, .. } => {
                    pattern.into()
                }
                _ => expression.into(),
            };

            self.builder_mut()?
                .plan_access(
                    node,
                    expression,
                    bray_bound_tree::StorageAccessPurpose::Borrow(kind),
                    access,
                )
                .map_err(CheckerInfrastructureError::StoragePlan)?;
        }

        Ok(Some(access))
    }

    pub(super) fn scoped_borrow_access(
        &mut self,
        scoped: &bray_bound_tree::SelectedScopedUse,
        pattern: bray_bound_tree::BoundPatternId,
    ) -> Result<Option<StorageAccessId>, PlanError<C::UpstreamError>> {
        let ty = scoped.capability_type();
        let data = self.request.semantic_values().type_data(ty);

        let TypeData::Borrow { kind, target } = data.as_ref() else {
            return Ok(None);
        };

        let callable = scoped.enter().0;

        let template = self
            .request
            .context()
            .callable_result_dependencies(callable.definition().callable_symbol())?;

        let template =
            crate::dependency::resolved_result_template(self.request, Some(callable), template)?;

        let sources = template
            .requirements()
            .iter()
            .filter_map(|requirement| match requirement {
                DependencyRequirement::Direct { subject, .. } => Some(subject),
                _ => None,
            })
            .collect::<BTreeSet<_>>();

        let mut sources = sources.into_iter();

        let Some(source) = sources.next() else {
            return Ok(None);
        };

        if sources.next().is_some() || source.subject_root() != DependencySubjectRoot::Receiver {
            return Ok(None);
        }

        self.retain_returned_borrow(
            scoped.expression(),
            bray_bound_tree::StorageIdentity::ScopedCapability {
                expression: scoped.expression(),
                pattern,
            },
            ty,
            *kind,
            *target,
            scoped.initializer(),
            source.projections(),
        )
    }
}
