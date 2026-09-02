use bray_bound_tree::{
    BorrowCapabilityOrigin, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget,
    PlannedBorrowCapability, SelectedOperation, SemanticSelection, StorageAccess, StorageAccessId,
    StorageAccessRoot, StorageBinding, StorageBindingTarget, StorageIdentity, StorageProjection,
};
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, MemberLookupResult, NamedTypeSymbolId, SymbolOrdinal, TypeData,
    TypeId,
};

use super::super::plan::{PlanError, Planner, invalid_node};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

pub(super) enum MemberStorage {
    Projection(StorageProjection),
    Value,
    Recovered,
}

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(in crate::storage) fn reference_access(
        &mut self,
        expression: BoundExpressionId,
        target: BoundReferenceTarget,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
            BoundReferenceTarget::Surface(AnySymbolId::Static(id)) => {
                let target = StorageBindingTarget::Static(id);

                if self.builder()?.binding(target).is_none() {
                    let ty = self.expression_type(expression)?.ty();
                    let _ = self.bind_identity(target, StorageIdentity::Static(id), Some(ty))?;
                }

                self.builder()?.binding(target)
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
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let Some(receiver) = self.receiver_parameter() else {
            return self.recovery_access(expression);
        };

        let receiver = match self
            .builder()?
            .binding(StorageBindingTarget::Receiver(receiver))
        {
            Some(StorageBinding::Identity(storage)) => self.direct_access(expression, storage)?,
            Some(StorageBinding::Access(access)) => self.copy_access(expression, access)?,
            None => return self.recovery_access(expression),
        };

        self.project_access(expression, receiver, Some(projection))
    }

    pub(super) fn member_projection(
        &self,
        expression: BoundExpressionId,
        selector: Option<&BoundMemberSelector>,
    ) -> Result<MemberStorage, PlanError<C::UpstreamError>> {
        match selector {
            Some(BoundMemberSelector::TupleElement(index)) => Ok(MemberStorage::Projection(
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
    ) -> Result<MemberStorage, PlanError<C::UpstreamError>> {
        let member = match self.selections.expression(expression) {
            Some(SemanticSelection::Operation(SelectedOperation::Member(target))) => {
                target.member()
            }
            Some(_) => return Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
            None => return self.member_from_checked_receiver(expression),
        };

        Ok(match member {
            AnySymbolId::StructField(field) => {
                MemberStorage::Projection(StorageProjection::ProductField(field))
            }
            AnySymbolId::UnionPayloadField(field) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                MemberStorage::Projection(StorageProjection::ActiveUnionPayloadField {
                    variant: field.variant(),
                    field: field.id(),
                })
            }
            _ => MemberStorage::Value,
        })
    }

    fn member_from_checked_receiver(
        &self,
        expression: BoundExpressionId,
    ) -> Result<MemberStorage, PlanError<C::UpstreamError>> {
        let bound = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let (receiver, selector) = match bound {
            bray_bound_tree::BoundExpression::MemberAccess(member) => {
                (member.receiver(), member.selector())
            }
            bray_bound_tree::BoundExpression::TraitQualifiedMember(member) => {
                (member.receiver(), member.selector())
            }
            _ => return Err(CheckerInfrastructureError::InvalidStoragePlan.into()),
        };

        if bound.is_recovered() {
            return Ok(MemberStorage::Recovered);
        }

        let Some(BoundMemberSelector::Name(name)) = selector else {
            return Ok(MemberStorage::Recovered);
        };

        let receiver = self
            .types
            .expression(receiver)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        if receiver.is_recovered() {
            return Ok(MemberStorage::Recovered);
        }

        let data = self
            .request
            .semantic_values()
            .type_data(receiver.ty())
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(MemberStorage::Value);
        };

        let result = match definition {
            NamedTypeSymbolId::Struct(structure) => self
                .request
                .symbols()
                .lookup_member((*structure).into(), name.as_str()),
            NamedTypeSymbolId::Union(union) => self
                .request
                .symbols()
                .lookup_member((*union).into(), name.as_str()),
        };

        match result {
            MemberLookupResult::Found(AnySymbolId::StructField(field)) => Ok(
                MemberStorage::Projection(StorageProjection::ProductField(field)),
            ),
            MemberLookupResult::Found(AnySymbolId::UnionPayloadField(field)) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
                };

                Ok(MemberStorage::Projection(
                    StorageProjection::ActiveUnionPayloadField {
                        variant: field.variant(),
                        field: field.id(),
                    },
                ))
            }
            MemberLookupResult::Found(_) => Ok(MemberStorage::Value),
            MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => Ok(MemberStorage::Recovered),
        }
    }

    pub(super) fn member_access(
        &mut self,
        expression: BoundExpressionId,
        receiver: StorageAccessId,
        storage: MemberStorage,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        match storage {
            MemberStorage::Projection(projection) => {
                self.project_access(expression, receiver, Some(projection))
            }
            MemberStorage::Value => self.temporary_access(expression),
            MemberStorage::Recovered => self.conservative_subject_access(expression, receiver),
        }
    }

    pub(super) fn project_access(
        &mut self,
        expression: BoundExpressionId,
        base: StorageAccessId,
        projection: Option<StorageProjection>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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

    pub(super) fn access_with_reached_type(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
        reached_type: TypeId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let access = self
            .builder()?
            .access(access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = access.root();
        let projections = access.projections().to_vec();
        let expression_type = self.expression_type(expression)?;

        self.push_expression_access(
            expression,
            root,
            projections,
            bray_bound_tree::ExpressionTypeResult::new(reached_type, expression_type.status()),
        )
    }

    pub(in crate::storage) fn borrow_access(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let borrowed = self
            .builder()?
            .access(access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let parent = borrowed.root().borrow_capability();

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
            .map_err(CheckerInfrastructureError::StoragePlan)?;

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
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }

    pub(super) fn temporary_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let ty = self.expression_type(expression)?.ty();

        let storage = self
            .builder_mut()?
            .push_identity(StorageIdentity::Temporary(expression))
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        self.builder_mut()?
            .set_identity_type(storage, ty)
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        self.direct_access(expression, storage)
    }

    pub(super) fn custom_index_access(
        &mut self,
        expression: BoundExpressionId,
        receiver_access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let result = self.expression_type(expression)?;

        let receiver = self
            .builder()?
            .access(receiver_access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Expression(expression),
            kind,
            receiver_access,
            receiver.root().borrow_capability(),
            node.origin().source_anchor(),
            receiver.is_recovered() || node.is_recovered(),
        );

        let capability = self
            .builder_mut()?
            .push_borrow_capability(capability)
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        let borrow_type = self
            .request
            .semantic_values()
            .intern_type(bray_symbols::TypeData::Borrow {
                kind,
                target: result.ty(),
            })
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let storage = self
            .builder_mut()?
            .push_identity(StorageIdentity::CustomIndexBorrow(expression))
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        self.builder_mut()?
            .set_identity_type(storage, borrow_type)
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        self.push_expression_access(
            expression,
            StorageAccessRoot::BorrowedStorage {
                capability,
                storage,
            },
            [],
            result,
        )
    }

    pub(super) fn iteration_access(
        &mut self,
        expression: BoundExpressionId,
        identity: StorageIdentity,
        ty: bray_symbols::TypeId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let node = self
            .request
            .view()
            .expression(expression)
            .ok_or_else(|| invalid_node(expression))?;

        let source = node.origin().source_anchor();
        let is_recovered = node.is_recovered();

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(
                self.builder_mut()?
                    .push_identity(identity)
                    .map_err(CheckerInfrastructureError::StoragePlan)?,
            ),
            [],
            ty,
            source,
            is_recovered,
        );

        self.builder_mut()?
            .push_access(access)
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }

    pub(super) fn recovery_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        self.builder_mut()?
            .push_access(StorageAccess::new(
                StorageAccessRoot::Recovery(storage),
                [],
                result.ty(),
                source,
                true,
            ))
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }

    pub(super) fn result_access(
        &mut self,
        expression: BoundExpressionId,
        value: Option<BoundExpressionId>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
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
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }
}
