use bray_bound_tree::{
    BorrowCapabilityOrigin, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget,
    PlannedBorrowCapability, SelectedOperation, SemanticSelection, StorageAccess, StorageAccessId,
    StorageAccessRoot, StorageBinding, StorageBindingTarget, StorageIdentity, StorageProjection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, SymbolOrdinal};

use super::super::plan::{PlanError, Planner, invalid_node};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::storage) fn reference_access(
        &mut self,
        expression: BoundExpressionId,
        target: BoundReferenceTarget,
    ) -> Result<StorageAccessId, PlanError> {
        let binding = match target {
            BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(id)) => {
                self.builder()?.binding(StorageBindingTarget::Local(id))
            }
            BoundReferenceTarget::Local(AnyLocalSymbolId::AnonymousCallableParameter(id)) => self
                .builder()?
                .binding(StorageBindingTarget::AnonymousParameter(id)),
            BoundReferenceTarget::Local(AnyLocalSymbolId::PostconditionResult(id)) => self
                .builder()?
                .binding(StorageBindingTarget::PostconditionResult(id)),
            BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(id)) => {
                self.builder()?.binding(StorageBindingTarget::Parameter(id))
            }
            BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(id)) => {
                self.builder()?.binding(StorageBindingTarget::Receiver(id))
            }
            BoundReferenceTarget::Surface(AnySymbolId::PredicateParameter(id)) => self
                .builder()?
                .binding(StorageBindingTarget::PredicateParameter(id)),
            BoundReferenceTarget::Surface(AnySymbolId::StructField(field)) => {
                return self
                    .receiver_field_access(expression, StorageProjection::ProductField(field));
            }
            BoundReferenceTarget::Surface(AnySymbolId::UnionPayloadField(field)) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                return self.receiver_field_access(
                    expression,
                    StorageProjection::ActiveUnionPayloadField {
                        variant: field.variant(),
                        field: field.id(),
                    },
                );
            }
            BoundReferenceTarget::Local(
                AnyLocalSymbolId::Constant(_) | AnyLocalSymbolId::AnonymousCallable(_),
            )
            | BoundReferenceTarget::Surface(_) => None,
        };

        match binding {
            Some(StorageBinding::Identity(storage)) => self.direct_access(expression, storage),
            Some(StorageBinding::Access(access)) => self.copy_access(expression, access),
            None => self.temporary_access(expression),
        }
    }

    fn receiver_field_access(
        &mut self,
        expression: BoundExpressionId,
        projection: StorageProjection,
    ) -> Result<StorageAccessId, PlanError> {
        let Some(receiver) = self.receiver_storage else {
            return self.recovery_access(expression);
        };

        let receiver = self.direct_access(expression, receiver)?;

        self.project_access(expression, receiver, Some(projection))
    }

    pub(super) fn member_projection(
        &self,
        expression: BoundExpressionId,
        selector: Option<&BoundMemberSelector>,
    ) -> Result<Option<StorageProjection>, PlanError> {
        match selector {
            Some(BoundMemberSelector::TupleElement(index)) => Ok(Some(
                StorageProjection::TupleElement(SymbolOrdinal::new(*index)),
            )),
            Some(BoundMemberSelector::Name(_)) | None => {
                self.selected_member_projection(expression)
            }
        }
    }

    pub(super) fn selected_member_projection(
        &self,
        expression: BoundExpressionId,
    ) -> Result<Option<StorageProjection>, PlanError> {
        // TODO(BRA-268): Restrict this fallback to recovery once every member provider publishes
        // an exact selection.
        let Some(SemanticSelection::Operation(SelectedOperation::Member(target))) =
            self.selections.expression(expression)
        else {
            return Ok(None);
        };

        Ok(match target.member() {
            AnySymbolId::StructField(field) => Some(StorageProjection::ProductField(field)),
            AnySymbolId::UnionPayloadField(field) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                Some(StorageProjection::ActiveUnionPayloadField {
                    variant: field.variant(),
                    field: field.id(),
                })
            }
            _ => None,
        })
    }

    pub(super) fn project_access(
        &mut self,
        expression: BoundExpressionId,
        base: StorageAccessId,
        projection: Option<StorageProjection>,
    ) -> Result<StorageAccessId, PlanError> {
        let Some(projection) = projection else {
            return self.temporary_access(expression);
        };

        let base = self
            .builder()?
            .access(base)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = base.root();
        let mut projections = base.projections().to_vec();

        projections.push(projection);

        self.push_expression_access(
            expression,
            root,
            projections,
            self.expression_type(expression)?,
        )
    }

    fn direct_access(
        &mut self,
        expression: BoundExpressionId,
        storage: bray_bound_tree::StorageIdentityId,
    ) -> Result<StorageAccessId, PlanError> {
        let root = match self.builder()?.identity(storage) {
            Some(StorageIdentity::Error(_)) => StorageAccessRoot::Recovery(storage),
            Some(_) => StorageAccessRoot::Storage(storage),
            None => return Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
        };

        self.push_expression_access(expression, root, [], self.expression_type(expression)?)
    }

    pub(super) fn copy_access(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError> {
        let access = self
            .builder()?
            .access(access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = access.root();
        let projections = access.projections().to_vec();

        self.push_expression_access(
            expression,
            root,
            projections,
            self.expression_type(expression)?,
        )
    }

    pub(in crate::storage) fn borrow_access(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Result<StorageAccessId, PlanError> {
        let borrowed = self
            .builder()?
            .access(access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let parent = match borrowed.root() {
            StorageAccessRoot::Borrow(parent) => Some(parent),
            StorageAccessRoot::Storage(_)
            | StorageAccessRoot::OwnedIndirection { .. }
            | StorageAccessRoot::Recovery(_) => None,
        };

        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Expression(expression),
            kind,
            access,
            parent,
            node.origin().source_anchor(),
            borrowed.is_recovered() || node.is_recovered(),
        );

        let capability = self
            .builder_mut()?
            .push_borrow_capability(capability)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        self.push_expression_access(
            expression,
            StorageAccessRoot::Borrow(capability),
            [],
            self.expression_type(expression)?,
        )
    }

    pub(super) fn conservative_subject_access(
        &mut self,
        expression: BoundExpressionId,
        subject: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError> {
        let subject = self
            .builder()?
            .access(subject)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = subject.root();
        let projections = subject.projections().to_vec();

        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let result = self.expression_type(expression)?;

        let access = StorageAccess::new(
            root,
            projections,
            result.ty(),
            node.origin().source_anchor(),
            true,
        );

        self.builder_mut()?
            .push_access(access)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn temporary_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError> {
        let storage = self
            .builder_mut()?
            .push_identity(StorageIdentity::Temporary(expression))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        self.direct_access(expression, storage)
    }

    pub(super) fn recovery_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError> {
        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let source = node.origin().source_anchor();
        let result = self.expression_type(expression)?;

        let storage = self
            .builder_mut()?
            .push_identity(StorageIdentity::Error(source))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

        self.builder_mut()?
            .push_access(StorageAccess::new(
                StorageAccessRoot::Recovery(storage),
                [],
                result.ty(),
                source,
                true,
            ))
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }

    pub(super) fn result_access(
        &mut self,
        expression: BoundExpressionId,
        value: Option<BoundExpressionId>,
    ) -> Result<StorageAccessId, PlanError> {
        let result = match value {
            Some(value) => self.expression_type(value)?,
            None => self.expression_type(expression)?,
        };

        self.push_expression_access(
            expression,
            StorageAccessRoot::Storage(self.result_storage),
            [],
            result,
        )
    }

    fn push_expression_access(
        &mut self,
        expression: BoundExpressionId,
        root: StorageAccessRoot,
        projections: impl IntoIterator<Item = StorageProjection>,
        result: bray_bound_tree::ExpressionTypeResult,
    ) -> Result<StorageAccessId, PlanError> {
        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let access = StorageAccess::new(
            root,
            projections,
            result.ty(),
            node.origin().source_anchor(),
            node.is_recovered() || result.is_recovered(),
        );

        self.builder_mut()?
            .push_access(access)
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }
}
