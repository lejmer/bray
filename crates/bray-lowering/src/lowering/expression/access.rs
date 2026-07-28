use bray_bound_tree::{
    BoundExpression, BoundExpressionId, IndexTarget, SelectedOperation, StorageAccessId,
    StorageAccessPurpose, StorageIdentity, StorageIdentityId, StorageOperationStatus,
    StorageProjection,
};
use bray_ir::{
    MirBlockId, MirCall, MirCallTarget, MirCallableReference, MirFieldReference, MirOperand,
    MirOperationKind, MirPlace, MirProjection,
};
use bray_symbols::{AnySymbolId, CallableAbi, TypeId};

use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;
use super::super::LoweringError;

pub(super) enum LoweredPlace {
    Continuing { block: MirBlockId, place: MirPlace },
    Terminated(LoweredExpression),
}

impl Lowerer<'_> {
    pub(super) fn lower_member_access(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        match self.input.semantic_selections().expression(id) {
            Some(bray_bound_tree::SemanticSelection::Operation(
                SelectedOperation::Construction(_),
            )) => self.lower_construction(id, current),
            Some(bray_bound_tree::SemanticSelection::Operation(SelectedOperation::Member(
                target,
            ))) => match target.member() {
                AnySymbolId::UnionVariant(_) => self.lower_construction(id, current),
                AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_) => {
                    self.lower_storage_operand(id, current)
                }
                _ => Err(LoweringError::UnsupportedExpression(id)),
            },
            None => self.lower_storage_operand(id, current),
            _ => Err(LoweringError::UnsupportedExpression(id)),
        }
    }

    pub(super) fn lower_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = match self.selected_operation(id)? {
            SelectedOperation::Index { target, .. } => *target,
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        match target {
            IndexTarget::ArrayElement
            | IndexTarget::SliceElement
            | IndexTarget::ArraySlice
            | IndexTarget::Slice => self.lower_storage_operand(id, current),
            IndexTarget::Custom { fulfillment, .. } => {
                self.lower_custom_index(id, current, fulfillment)
            }
        }
    }

    fn lower_custom_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        fulfillment: bray_symbols::CallableInstanceData,
    ) -> Result<LoweredExpression, LoweringError> {
        let operands = self
            .input
            .unit()
            .view()
            .expression(id)
            .and_then(|expression| match expression {
                BoundExpression::Structured(expression) => Some(expression.operands()),
                _ => None,
            })
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?
            .to_vec();

        let source = self.expression_source(id)?;
        let mut block = current;
        let mut arguments = Vec::with_capacity(operands.len());

        for operand in operands {
            let lowered = self.lower_expression(operand, block)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(operand));
            };

            block = continuation;
            arguments.push(value);
        }

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Call(MirCall::new(
                MirCallTarget::Direct(MirCallableReference::new(
                    fulfillment,
                    CallableAbi::Bray,
                )),
                arguments,
            )),
        )?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    pub(super) fn lower_storage_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let decision = self
            .storage_decision(expression, |purpose| {
                matches!(
                    purpose,
                    StorageAccessPurpose::Read
                        | StorageAccessPurpose::Copy
                        | StorageAccessPurpose::Move
                        | StorageAccessPurpose::ValueTransfer
                )
            })
            .or_else(|error| match error {
                LoweringError::MissingStorageAccess(_) => {
                    self.storage_decision(expression, |purpose| {
                        matches!(
                            purpose,
                            StorageAccessPurpose::Member
                                | StorageAccessPurpose::Index
                                | StorageAccessPurpose::Slice
                                | StorageAccessPurpose::Projection
                        )
                    })
                }
                _ => Err(error),
            })?;

        let source = self.expression_source(expression)?;

        let (block, place) =
            match self.lower_access_place(expression, decision.access(), current)? {
                LoweredPlace::Continuing { block, place } => (block, place),
                LoweredPlace::Terminated(completion) => return Ok(completion),
            };

        let operand = match decision.purpose() {
            StorageAccessPurpose::Move => MirOperand::Move(place),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => MirOperand::Copy(place),
            _ => return Err(LoweringError::UnsupportedStorageAccess(decision.access())),
        };

        Ok(LoweredExpression::continuing(
            block,
            Some(operand),
            source,
        ))
    }

    pub(super) fn lower_expression_place(
        &mut self,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        current: MirBlockId,
    ) -> Result<LoweredPlace, LoweringError> {
        let decision = self.storage_decision(expression, |candidate| candidate == purpose)?;

        self.lower_access_place(expression, decision.access(), current)
    }

    fn storage_decision(
        &self,
        expression: BoundExpressionId,
        accepts: impl Fn(StorageAccessPurpose) -> bool,
    ) -> Result<bray_bound_tree::StorageOperationDecision, LoweringError> {
        let plan = self
            .input
            .storage_plan()
            .expression_plans(expression)
            .find(|plan| accepts(plan.purpose()))
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        let decision = self
            .input
            .storage_flow()
            .operations()
            .iter()
            .copied()
            .find(|decision| {
                decision.expression() == expression && decision.access() == plan.access()
            })
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        if decision.status() != StorageOperationStatus::Valid {
            return Err(LoweringError::RecoveredBoundNode(expression.into()));
        }

        Ok(decision)
    }

    fn lower_access_place(
        &mut self,
        expression: BoundExpressionId,
        id: StorageAccessId,
        current: MirBlockId,
    ) -> Result<LoweredPlace, LoweringError> {
        let access = self
            .input
            .storage_plan()
            .access(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?;

        if access.is_recovered() {
            return Err(LoweringError::RecoveredBoundNode(expression.into()));
        }

        let reached_type = access.reached_type();

        let identity = self
            .input
            .storage_plan()
            .root_identity(id)
            .ok_or(LoweringError::MissingStorageIdentity(id))?;

        let projections = self
            .input
            .storage_plan()
            .resolved_projections(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?
            .to_vec();

        let (mut current, storage) =
            match self.initialize_access_root(identity, expression, current)? {
                RootInitialization::Continuing { block, storage } => (block, storage),
                RootInitialization::Terminated(completion) => {
                    return Ok(LoweredPlace::Terminated(completion));
                }
            };

        let mut lowered = Vec::new();

        for projection in projections {
            current = self.lower_projection(projection, current, &mut lowered)?;
        }

        Ok(LoweredPlace::Continuing {
            block: current,
            place: MirPlace::new(storage, lowered, reached_type),
        })
    }

    fn initialize_access_root(
        &mut self,
        identity: StorageIdentityId,
        expression: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<RootInitialization, LoweringError> {
        if let Some(storage) = self.storages.get(&identity).copied() {
            return Ok(RootInitialization::Continuing {
                block: current,
                storage,
            });
        }

        let model = self
            .input
            .storage_plan()
            .identity(identity)
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

        let owner = match model {
            StorageIdentity::Temporary(owner) | StorageIdentity::Allocation(owner)
                if owner != expression =>
            {
                Some(owner)
            }
            _ => None,
        };

        let mut current = current;
        let mut initial_value = None;

        if let Some(owner) = owner {
            let lowered = self.lower_expression(owner, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(RootInitialization::Terminated(lowered));
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(owner));
            };

            current = continuation;
            initial_value = Some((owner, value));
        }

        let root_type = self.storage_identity_type(identity)?;

        let origin = initial_value.as_ref().map_or_else(
            || bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source()),
            |(owner, _)| {
                self.input
                    .unit()
                    .view()
                    .expression(*owner)
                    .map(|expression| expression.origin())
                    .unwrap_or_else(|| {
                        bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source())
                    })
            },
        );

        let place = self.place_for_identity(identity, root_type, origin)?;

        if let Some((owner, value)) = initial_value {
            self.builder.push_operation(
                current,
                self.expression_source(owner)?,
                MirOperationKind::Store {
                    destination: place.clone(),
                    value,
                },
                None,
            )?;
        }

        Ok(RootInitialization::Continuing {
            block: current,
            storage: place.storage(),
        })
    }

    pub(in crate::lowering) fn place_for_access(
        &mut self,
        id: StorageAccessId,
    ) -> Result<MirPlace, LoweringError> {
        let access = self
            .input
            .storage_plan()
            .access(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?;

        if access.is_recovered()
            || self
                .input
                .storage_plan()
                .resolved_projections(id)
                .is_none_or(|projections| !projections.is_empty())
        {
            return Err(LoweringError::UnsupportedStorageAccess(id));
        }

        let reached_type = access.reached_type();
        let source = access.source();

        let identity = self
            .input
            .storage_plan()
            .root_identity(id)
            .ok_or(LoweringError::MissingStorageIdentity(id))?;

        self.place_for_identity(
            identity,
            reached_type,
            bray_bound_tree::BoundNodeOrigin::source(source),
        )
    }

    fn storage_identity_type(&self, identity: StorageIdentityId) -> Result<TypeId, LoweringError> {
        let plan = self.input.storage_plan();

        plan.access_entries()
            .find_map(|(access, model)| {
                (plan.root_identity(access) == Some(identity)
                    && plan
                        .resolved_projections(access)
                        .is_some_and(<[_]>::is_empty))
                .then_some(model.reached_type())
            })
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))
    }

    fn lower_projection(
        &mut self,
        projection: StorageProjection,
        mut current: MirBlockId,
        lowered: &mut Vec<MirProjection>,
    ) -> Result<MirBlockId, LoweringError> {
        match projection {
            StorageProjection::ProductField(field) => {
                lowered.push(MirProjection::Field(MirFieldReference::Struct(field)));
            }
            StorageProjection::TupleElement(ordinal) => {
                lowered.push(MirProjection::TupleField(ordinal.raw()));
            }
            StorageProjection::ElementFromStart(ordinal) => {
                lowered.push(MirProjection::ElementFromStart(ordinal.raw()));
            }
            StorageProjection::ElementFromEnd(ordinal) => {
                lowered.push(MirProjection::ElementFromEnd(ordinal.raw()));
            }
            StorageProjection::ActiveUnionPayloadField { variant, field } => {
                lowered.push(MirProjection::Variant(variant));
                lowered.push(MirProjection::Field(MirFieldReference::UnionPayload(field)));
            }
            StorageProjection::Element(selector) => {
                let (continuation, selector) = self.lower_selector(selector, current)?;

                current = continuation;
                lowered.push(MirProjection::Index(selector));
            }
            StorageProjection::SliceRange { start, end } => {
                let (continuation, start) = self.lower_optional_selector(start, current)?;

                let (continuation, end) = self.lower_optional_selector(end, continuation)?;

                current = continuation;
                lowered.push(MirProjection::Slice { start, end });
            }
            StorageProjection::NullableValue => lowered.push(MirProjection::NullableValue),
            StorageProjection::OwnedTarget => lowered.push(MirProjection::Dereference),
        }

        Ok(current)
    }

    fn lower_optional_selector(
        &mut self,
        selector: Option<BoundExpressionId>,
        current: MirBlockId,
    ) -> Result<(MirBlockId, Option<MirOperand>), LoweringError> {
        match selector {
            Some(selector) => {
                let (current, value) = self.lower_selector(selector, current)?;

                Ok((current, Some(value)))
            }
            None => Ok((current, None)),
        }
    }

    fn lower_selector(
        &mut self,
        selector: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let lowered = self.lower_expression(selector, current)?;

        let Some(current) = lowered.block else {
            return Err(LoweringError::UnsupportedExpression(selector));
        };

        let Some(value) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(selector));
        };

        Ok((current, value))
    }
}

enum RootInitialization {
    Continuing {
        block: MirBlockId,
        storage: bray_ir::MirStorageId,
    },
    Terminated(LoweredExpression),
}
