use bray_bound_tree::{
    BorrowCapabilityOrigin, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget,
    PlannedBorrowCapability, SelectedOperation, SemanticSelection, StorageAccess, StorageAccessId,
    StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget, StorageIdentity,
    StorageProjection,
};
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, MemberLookupResult, NamedTypeSymbolId, SymbolOrdinal, TypeData,
    TypeId,
};

use super::super::plan::{PlanError, Planner, missing_node};
use crate::CheckerRequestContext;

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
                self.builder().binding(StorageBindingTarget::Local(id))
            }
            BoundReferenceTarget::Local(AnyLocalSymbolId::AnonymousCallableParameter(id)) => self
                .builder()
                .binding(StorageBindingTarget::AnonymousParameter(id)),
            BoundReferenceTarget::Local(AnyLocalSymbolId::PostconditionResult(id)) => self
                .builder()
                .binding(StorageBindingTarget::PostconditionResult(id)),
            BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(id)) => {
                self.builder().binding(StorageBindingTarget::Parameter(id))
            }
            BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(id)) => {
                self.builder().binding(StorageBindingTarget::Receiver(id))
            }
            BoundReferenceTarget::Surface(AnySymbolId::Static(id)) => {
                let target = StorageBindingTarget::Static(id);

                if self.builder().binding(target).is_none() {
                    let ty = self.expression_type(expression).ty();
                    let _ = self.bind_identity(target, StorageIdentity::Static(id), Some(ty))?;
                }

                self.builder().binding(target)
            }
            BoundReferenceTarget::Surface(AnySymbolId::PredicateParameter(id)) => self
                .builder()
                .binding(StorageBindingTarget::PredicateParameter(id)),
            BoundReferenceTarget::Surface(AnySymbolId::StructField(field)) => {
                return self
                    .receiver_field_access(expression, StorageProjection::ProductField(field));
            }
            BoundReferenceTarget::Surface(AnySymbolId::UnionPayloadField(field)) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    panic!(
                        "Storage-planning inputs or constructed records violate the requested unit contract. in reference_access"
                    );
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
            .builder()
            .binding(StorageBindingTarget::Receiver(receiver))
        {
            Some(StorageBinding::Identity(storage)) => self.direct_access(expression, storage)?,
            Some(StorageBinding::Access(access)) => self.copy_access(expression, access)?,
            None => return self.recovery_access(expression),
        };

        self.project_access(
            expression,
            receiver,
            projection,
            self.expression_type(expression),
        )
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
            Some(_) => panic!(
                "Storage-planning inputs or constructed records violate the requested unit contract. in selected_member_projection"
            ),
            None => return self.member_from_checked_receiver(expression),
        };

        Ok(match member {
            AnySymbolId::StructField(field) => {
                MemberStorage::Projection(StorageProjection::ProductField(field))
            }
            AnySymbolId::UnionPayloadField(field) => {
                let Some(field) = self.request.symbols().union_payload_field(field) else {
                    panic!(
                        "Storage-planning inputs or constructed records violate the requested unit contract. in selected_member_projection"
                    );
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
            .unwrap_or_else(|| missing_node(expression));

        let (receiver, selector) = match bound {
            bray_bound_tree::BoundExpression::MemberAccess(member) => {
                (member.receiver(), member.selector())
            }
            bray_bound_tree::BoundExpression::TraitQualifiedMember(member) => {
                (member.receiver(), member.selector())
            }
            _ => panic!(
                "Storage-planning inputs or constructed records violate the requested unit contract. in member_from_checked_receiver"
            ),
        };

        if bound.is_recovered() {
            return Ok(MemberStorage::Recovered);
        }

        let Some(BoundMemberSelector::Name(name)) = selector else {
            return Ok(MemberStorage::Recovered);
        };

        let receiver = self
            .types
            .expression(receiver).unwrap_or_else(|| panic!("member_from_checked_receiver requires checked expression type or node, expression: {expression:?}, receiver: {receiver:?}"));

        if receiver.is_recovered() {
            return Ok(MemberStorage::Recovered);
        }

        let data = self.request.semantic_values().type_data(receiver.ty());

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
                    panic!(
                        "Storage-planning inputs or constructed records violate the requested unit contract. in member_from_checked_receiver"
                    );
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
            MemberStorage::Projection(projection) => self.project_access(
                expression,
                receiver,
                projection,
                self.expression_type(expression),
            ),
            MemberStorage::Value => self.temporary_access(expression),
            MemberStorage::Recovered => self.conservative_subject_access(expression, receiver),
        }
    }

    pub(super) fn project_access(
        &mut self,
        expression: BoundExpressionId,
        base: StorageAccessId,
        projection: StorageProjection,
        reached_type: bray_bound_tree::ExpressionTypeResult,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let base = self.borrowed_value_access(expression, base)?;

        let base = self
            .builder()
            .access(base).unwrap_or_else(|| panic!("project_access requires planned storage access, expression: {expression:?}, base: {base:?}"));

        let root = base.root();
        let mut projections = base.projections().to_vec();

        projections.push(projection);

        self.push_expression_access(expression, root, projections, reached_type)
    }

    pub(super) fn borrowed_value_access(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let record = self
            .builder()
            .access(access).unwrap_or_else(|| panic!("borrowed_value_access requires planned storage access, expression: {expression:?}, access: {access:?}"));

        let StorageAccessRoot::Storage(identity) = record.root() else {
            return Ok(access);
        };

        let reached_type = record.reached_type();

        let Some(source) = self.borrowed_values.get(&identity).copied() else {
            return Ok(access);
        };

        let source = self.borrowed_value_access(expression, source)?;

        let source = self
            .builder()
            .access(source).unwrap_or_else(|| panic!("borrowed_value_access requires planned storage access, expression: {expression:?}, access: {access:?}, source: {source:?}"));

        let Some(capability) = source.root().borrow_capability() else {
            return Ok(access);
        };

        // The binding stores the converted borrow value independently of its initializer's representation.
        self.push_expression_access(
            expression,
            StorageAccessRoot::BorrowedStorage {
                capability,
                storage: identity,
            },
            [],
            bray_bound_tree::ExpressionTypeResult::new(
                reached_type,
                self.expression_type(expression).status(),
            ),
        )
    }

    fn direct_access(
        &mut self,
        expression: BoundExpressionId,
        storage: bray_bound_tree::StorageIdentityId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let root = match self.builder().identity(storage) {
            Some(StorageIdentity::Error(_)) => StorageAccessRoot::Recovery(storage),
            Some(_) => StorageAccessRoot::Storage(storage),
            None => panic!(
                "Storage-planning inputs or constructed records violate the requested unit contract. in direct_access"
            ),
        };

        self.push_expression_access(expression, root, [], self.expression_type(expression))
    }

    pub(super) fn copy_access(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let access = self
            .builder()
            .access(access).unwrap_or_else(|| panic!("copy_access requires planned storage access, expression: {expression:?}, access: {access:?}"));

        let root = access.root();
        let projections = access.projections().to_vec();

        self.push_expression_access(
            expression,
            root,
            projections,
            self.expression_type(expression),
        )
    }

    pub(super) fn access_with_reached_type(
        &mut self,
        expression: BoundExpressionId,
        access: StorageAccessId,
        reached_type: TypeId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let access = self
            .builder()
            .access(access).unwrap_or_else(|| panic!("access_with_reached_type requires planned storage access, expression: {expression:?}, access: {access:?}, reached_type: {reached_type:?}"));

        let root = access.root();
        let projections = access.projections().to_vec();
        let expression_type = self.expression_type(expression);

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
        result: TypeId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let borrowed = self
            .builder()
            .access(access).unwrap_or_else(|| panic!("borrow_access requires planned storage access, expression: {expression:?}, access: {access:?}, result: {result:?}"));

        let parent = borrowed.root().borrow_capability();

        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| missing_node(expression));

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Expression(expression),
            kind,
            access,
            parent,
            node.origin().source_anchor(),
            borrowed.is_recovered() || node.is_recovered(),
        );

        let capability = self
            .builder_mut()
            .push_borrow_capability(capability).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in borrow_access: {error:?}"));

        self.push_expression_access(
            expression,
            StorageAccessRoot::Borrow(capability),
            [],
            bray_bound_tree::ExpressionTypeResult::new(
                result,
                self.expression_type(expression).status(),
            ),
        )
    }

    pub(super) fn conservative_subject_access(
        &mut self,
        expression: BoundExpressionId,
        subject: StorageAccessId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let subject = self
            .builder()
            .access(subject).unwrap_or_else(|| panic!("conservative_subject_access requires planned storage access, expression: {expression:?}, subject: {subject:?}"));

        let root = subject.root();
        let projections = subject.projections().to_vec();

        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| missing_node(expression));

        let result = self.expression_type(expression);

        let access = StorageAccess::new(
            root,
            projections,
            result.ty(),
            node.origin().source_anchor(),
            true,
        );

        Ok(self.builder_mut()
            .push_access(access).unwrap_or_else(|error| panic!("conservative_subject_access must satisfy its checked construction contract: {error:?}")))
    }

    pub(super) fn temporary_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let ty = self.expression_type(expression).ty();

        let storage = self
            .builder_mut()
            .push_identity(StorageIdentity::Temporary(expression)).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in temporary_access: {error:?}"));

        self.builder_mut()
            .set_identity_type(storage, ty).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in temporary_access: {error:?}"));

        self.direct_access(expression, storage)
    }

    pub(super) fn custom_index_access(
        &mut self,
        expression: BoundExpressionId,
        receiver_access: StorageAccessId,
        kind: bray_symbols::BorrowKind,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let result = self.expression_type(expression);

        let receiver = self
            .builder()
            .access(receiver_access).unwrap_or_else(|| panic!("custom_index_access requires planned storage access, expression: {expression:?}, receiver_access: {receiver_access:?}"));

        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| missing_node(expression));

        let capability = PlannedBorrowCapability::new(
            BorrowCapabilityOrigin::Expression(expression),
            kind,
            receiver_access,
            receiver.root().borrow_capability(),
            node.origin().source_anchor(),
            receiver.is_recovered() || node.is_recovered(),
        );

        let capability = self
            .builder_mut()
            .push_borrow_capability(capability).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in custom_index_access: {error:?}"));

        let borrow_type = self
            .request
            .semantic_values()
            .intern_type(bray_symbols::TypeData::Borrow {
                kind,
                target: result.ty(),
            }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in custom_index_access: {error:?}"));

        self.retain_borrow_value(
            expression,
            capability,
            StorageIdentity::CustomIndexBorrow(expression),
            borrow_type,
            result,
        )
    }

    pub(super) fn retain_borrow_value(
        &mut self,
        expression: BoundExpressionId,
        capability: bray_bound_tree::BorrowCapabilityId,
        identity: StorageIdentity,
        stored_type: TypeId,
        reached_type: bray_bound_tree::ExpressionTypeResult,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let storage = self
            .builder_mut()
            .push_identity(identity).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in retain_borrow_value: {error:?}"));

        self.builder_mut()
            .set_identity_type(storage, stored_type).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in retain_borrow_value: {error:?}"));

        self.push_expression_access(
            expression,
            StorageAccessRoot::BorrowedStorage {
                capability,
                storage,
            },
            [],
            reached_type,
        )
    }

    pub(super) fn protocol_access(
        &mut self,
        expression: BoundExpressionId,
        identity: StorageIdentity,
        ty: bray_symbols::TypeId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| missing_node(expression));

        let source = node.origin().source_anchor();
        let is_recovered = node.is_recovered();

        let storage = self
            .builder_mut()
            .push_identity(identity).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in protocol_access: {error:?}"));

        self.builder_mut()
            .set_identity_type(storage, ty).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in protocol_access: {error:?}"));

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(storage),
            [],
            ty,
            source,
            is_recovered,
        );

        Ok(self
            .builder_mut()
            .push_access(access)
            .unwrap_or_else(|error| {
                panic!("protocol_access must satisfy its checked construction contract: {error:?}")
            }))
    }

    pub(super) fn recovery_access(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| missing_node(expression));

        let source = node.origin().source_anchor();
        let result = self.expression_type(expression);

        let storage = self
            .builder_mut()
            .push_identity(StorageIdentity::Error(source)).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in recovery_access: {error:?}"));

        Ok(self
            .builder_mut()
            .push_access(StorageAccess::new(
                StorageAccessRoot::Recovery(storage),
                [],
                result.ty(),
                source,
                true,
            ))
            .unwrap_or_else(|error| {
                panic!("recovery_access must satisfy its checked construction contract: {error:?}")
            }))
    }

    pub(super) fn result_access(
        &mut self,
        expression: BoundExpressionId,
        value: Option<BoundExpressionId>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let result = match value {
            Some(value) => self.expression_type(value),
            None => self.expression_type(expression),
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
            .unwrap_or_else(|| missing_node(expression));

        let builder = self.builder();

        let recovered_borrow = root
            .borrow_capability()
            .and_then(|capability| builder.borrow_capability(capability))
            .is_some_and(|capability| capability.is_recovered());

        let access = StorageAccess::new(
            root,
            projections,
            result.ty(),
            node.origin().source_anchor(),
            node.is_recovered() || result.is_recovered() || recovered_borrow,
        );

        Ok(self.builder_mut()
            .push_access(access).unwrap_or_else(|error| panic!("push_expression_access must satisfy its checked construction contract: {error:?}")))
    }
}

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(in crate::storage) fn record_purpose(
        &mut self,
        expression: BoundExpressionId,
        purpose: Option<StorageAccessPurpose>,
        access: StorageAccessId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let Some(purpose) = purpose else {
            return Ok(());
        };

        let purpose = self.materialization_purpose(expression, purpose, access)?;

        self.builder_mut()
            .plan_access(expression.into(), expression, purpose, access)
            .unwrap_or_else(|error| {
                panic!("record_purpose must satisfy its checked construction contract: {error:?}")
            });

        Ok(())
    }

    pub(in crate::storage) fn materialization_purpose(
        &self,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
    ) -> Result<StorageAccessPurpose, PlanError<C::UpstreamError>> {
        if purpose != StorageAccessPurpose::ValueTransfer {
            return Ok(purpose);
        }

        let builder = self.builder();

        let record = builder
            .access(access).unwrap_or_else(|| panic!("materialization_purpose requires planned storage access, expression: {expression:?}, access: {access:?}"));

        let fresh = record
            .root()
            .borrow_capability()
            .and_then(|id| builder.borrow_capability(id))
            .is_some_and(|borrow| borrow.expression() == Some(expression))
            || matches!(
                self.request.view().expression(expression),
                Some(bray_bound_tree::BoundExpression::Call(_))
            );

        if !fresh {
            return Ok(purpose);
        }

        let ty = self
            .request
            .semantic_values()
            .type_data(record.reached_type());

        Ok(if matches!(ty.as_ref(), TypeData::Borrow { .. }) {
            StorageAccessPurpose::Read
        } else {
            purpose
        })
    }
}
