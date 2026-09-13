use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpressionId, SelectedArgument, SelectedCall, StorageAccessId, StorageProjection,
};
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

        let data = self
            .request
            .semantic_values()
            .type_data(ty)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

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
            .requirements()
            .iter()
            .filter_map(|requirement| match requirement {
                DependencyRequirement::Direct { subject, .. } => Some(subject),
                DependencyRequirement::Guarded(_) => None,
            })
            .collect();

        let mut sources = sources.into_iter();

        let Some(source) = sources.next() else {
            return Ok(None);
        };

        if sources.next().is_some() {
            return Ok(None);
        }

        let argument = match source.subject_root() {
            DependencySubjectRoot::Receiver => call
                .receiver()
                .filter(|receiver| {
                    matches!(
                        receiver.mode(),
                        bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
                    )
                })
                .map(|receiver| receiver.expression()),
            DependencySubjectRoot::Parameter(parameter) => {
                call.arguments().iter().find_map(|argument| match argument {
                    SelectedArgument::Explicit {
                        ordinal,
                        expression,
                        ..
                    } if *ordinal == parameter.raw() => Some(*expression),
                    _ => None,
                })
            }
            _ => None,
        };

        let Some(argument) = argument else {
            return Ok(None);
        };

        let argument_type = self
            .request
            .semantic_values()
            .type_data(self.expression_type(argument)?.ty())
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        if matches!(source.subject_root(), DependencySubjectRoot::Parameter(_))
            && !matches!(argument_type.as_ref(), TypeData::Borrow { .. })
        {
            return Ok(None);
        }

        let argument = self.plan_expression(argument, None)?;
        let mut access = self.borrowed_value_access(expression, argument)?;

        for projection in source.projections() {
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

        let creates_capability = self
            .builder()?
            .access(access)
            .is_some_and(|access| access.root().borrow_capability().is_none());

        if creates_capability {
            let source = self.access_with_reached_type(expression, access, target)?;
            access = self.borrow_access(expression, source, kind)?;
        }

        let capability = self
            .builder()?
            .access(access)
            .and_then(|access| access.root().borrow_capability())
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let access = self.retain_borrow_value(
            expression,
            capability,
            bray_bound_tree::StorageIdentity::Temporary(expression),
            ty,
            self.expression_type(expression)?,
        )?;

        if creates_capability {
            self.record_purpose(
                expression,
                Some(bray_bound_tree::StorageAccessPurpose::Borrow(kind)),
                access,
            )?;
        }

        Ok(Some(access))
    }
}
