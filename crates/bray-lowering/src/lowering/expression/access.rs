use bray_bound_tree::{
    BoundExpressionId, SelectedOperation, SelectedReceiver, StorageAccessId, StorageAccessPurpose,
    StorageIdentity, StorageIdentityId, StorageOperationStatus, StorageProjection,
};
use bray_ir::{
    MirBlockId, MirFieldReference, MirOperand, MirOperationKind, MirPlace, MirProjection,
    MirProjectionKind, MirStoreKind,
};
use bray_symbols::{AnySymbolId, BorrowKind, ReceiverMode, TypeData, TypeId};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

pub(in crate::lowering) enum LoweredPlace {
    Continuing { block: MirBlockId, place: MirPlace },
    Terminated(LoweredExpression),
}

impl Lowerer<'_> {
    pub(super) fn lower_borrow(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = expression
            .borrow_kind()
            .ok_or(LoweringError::UnsupportedExpression(id))?;

        let source = self.source(expression.origin());
        let result_type = self.expression_type(id)?;

        let result_data = self
            .input
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let TypeData::Borrow {
            kind: result_kind,
            target,
        } = result_data.as_ref()
        else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        if *result_kind != kind {
            return Err(LoweringError::UnsupportedExpression(id));
        }

        self.lower_storage_borrow(id, id, current, kind, *target, result_type, source)
    }

    pub(super) fn lower_call_receiver(
        &mut self,
        receiver: &SelectedReceiver,
        current: MirBlockId,
    ) -> Result<(LoweredExpression, TypeId), LoweringError> {
        let target = receiver.target_type();

        let kind = match receiver.mode() {
            ReceiverMode::Shared => BorrowKind::Shared,
            ReceiverMode::Mutable => BorrowKind::Mutable,
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => {
                return self
                    .lower_expression(receiver.expression(), current)
                    .map(|lowered| (lowered, target));
            }
        };

        let result_type = self
            .input
            .semantic_values()
            .intern_type(TypeData::Borrow { kind, target })
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let source = self.expression_source(receiver.expression())?;

        self.lower_storage_borrow(
            receiver.expression(),
            receiver.expression(),
            current,
            kind,
            target,
            result_type,
            source,
        )
        .map(|lowered| (lowered, result_type))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "borrow lowering requires both source occurrences and the checked borrow types"
    )]
    fn lower_storage_borrow(
        &mut self,
        access_expression: BoundExpressionId,
        initialization_expression: BoundExpressionId,
        current: MirBlockId,
        kind: BorrowKind,
        target: TypeId,
        result_type: TypeId,
        source: bray_ir::MirSourceAnchor,
    ) -> Result<LoweredExpression, LoweringError> {
        let decision = self.storage_decision(access_expression, |purpose| {
            purpose == StorageAccessPurpose::Borrow(kind)
        })?;

        let temporary = self
            .input
            .storage_plan()
            .root_identity(decision.access())
            .filter(|identity| !self.storages.contains_key(identity))
            .and_then(|identity| {
                self.input
                    .storage_plan()
                    .identity(identity)
                    .and_then(|model| match model {
                        StorageIdentity::Temporary(owner) if owner == initialization_expression => {
                            Some(owner)
                        }
                        _ => None,
                    })
            });

        let current = if let Some(temporary) = temporary {
            let lowered = self.lower_expression(temporary, current)?;
            let lowered = self.materialize_for_later_evaluation(temporary, lowered)?;

            let Some(current) = lowered.block else {
                return Ok(lowered);
            };

            current
        } else {
            current
        };

        let (current, place) =
            match self.lower_access_place(initialization_expression, decision.access(), current)? {
                LoweredPlace::Continuing { block, place } => (block, place),
                LoweredPlace::Terminated(completion) => return Ok(completion),
            };

        let mut projections = place.projections().to_vec();

        let parameter_borrow = self
            .input
            .storage_plan()
            .root_identity(decision.access())
            .and_then(|identity| self.input.storage_plan().storage_type(identity))
            .and_then(|ty| {
                self.input
                    .semantic_values()
                    .type_data(ty)
                    .ok()
                    .and_then(|data| match data.as_ref() {
                        TypeData::Borrow { target, .. } => Some((ty, *target)),
                        _ => None,
                    })
            });

        let already_dereferenced = projections
            .first()
            .is_some_and(|projection| projection.kind() == &MirProjectionKind::Dereference);

        if let Some((parameter_type, reached_type)) = parameter_borrow
            && !already_dereferenced
        {
            projections.insert(
                0,
                MirProjection::new(MirProjectionKind::Dereference, parameter_type, reached_type),
            );
        }

        let place = MirPlace::new(place.storage(), projections, target);

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Borrow { kind, place },
            Some(result_type),
        )?;

        let value = commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(access_expression))?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    pub(super) fn lower_implicit_shared_borrow(
        &mut self,
        parent: BoundExpressionId,
        operand: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        self.lower_implicit_borrow(parent, operand, current, BorrowKind::Shared)
    }

    pub(super) fn lower_implicit_borrow(
        &mut self,
        parent: BoundExpressionId,
        operand: BoundExpressionId,
        current: MirBlockId,
        kind: BorrowKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = self.expression_type(operand)?;

        let result_type = self
            .input
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind,
                target,
            })
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let source = self.expression_source(operand)?;

        self.lower_storage_borrow(
            operand,
            parent,
            current,
            kind,
            target,
            result_type,
            source,
        )
    }

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

        let entry_borrow = self
            .input
            .storage_plan()
            .root_identity(decision.access())
            .and_then(|identity| self.input.storage_plan().identity(identity))
            .is_some_and(|identity| {
                matches!(
                    identity,
                    StorageIdentity::Parameter(_)
                        | StorageIdentity::Receiver(_)
                        | StorageIdentity::AnonymousParameter(_)
                        | StorageIdentity::PredicateParameter(_)
                )
            });

        if entry_borrow
            && place.projections().is_empty()
            && let TypeData::Borrow { kind, target } = self
                .input
                .semantic_values()
                .type_data(place.ty())
                .map_err(|_| LoweringError::SemanticValueUnavailable)?
                .as_ref()
        {
            let target = MirPlace::new(
                place.storage(),
                [MirProjection::new(
                    MirProjectionKind::Dereference,
                    place.ty(),
                    *target,
                )],
                *target,
            );

            let value = self.push_value_operation(
                expression,
                block,
                Self::retained_source(&source),
                MirOperationKind::Borrow {
                    kind: *kind,
                    place: target,
                },
            )?;

            return Ok(LoweredExpression::continuing(block, Some(value), source));
        }

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

        Ok(LoweredExpression::continuing(block, Some(operand), source))
    }

    pub(super) fn lower_expression_place(
        &mut self,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        current: MirBlockId,
    ) -> Result<LoweredPlace, LoweringError> {
        let decision = self.storage_decision(expression, |candidate| {
            candidate == purpose
                || purpose == StorageAccessPurpose::Assignment
                    && candidate == StorageAccessPurpose::Write
        })?;

        self.lower_access_place(expression, decision.access(), current)
    }

    pub(in crate::lowering) fn storage_decision(
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
                decision.expression() == expression
                    && decision.access() == plan.access()
                    && plan.purpose().matches_checked(decision.purpose())
            })
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        if decision.status() != StorageOperationStatus::Valid {
            return Err(LoweringError::RecoveredBoundNode(expression.into()));
        }

        Ok(decision)
    }

    pub(in crate::lowering) fn lower_access_place(
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

        let mut lowered = Vec::with_capacity(projections.len() + 1);

        let mut source_type = self.append_entry_dereference(
            identity,
            reached_type,
            projections.is_empty(),
            &mut lowered,
        )?;

        for (index, projection) in projections.iter().copied().enumerate() {
            let result_type = self.projection_result_type(identity, &projections[..=index])?;

            let (continuation, kind) = self.lower_projection(projection, current)?;

            current = continuation;
            lowered.push(MirProjection::new(kind, source_type, result_type));
            source_type = result_type;
        }

        Ok(LoweredPlace::Continuing {
            block: current,
            place: MirPlace::new(storage, lowered, source_type),
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

        let (owner, custom_index) = match model {
            StorageIdentity::CustomIndexBorrow(owner) => (Some(owner), true),
            StorageIdentity::Temporary(owner) | StorageIdentity::Allocation(owner)
                if owner != expression =>
            {
                (Some(owner), false)
            }
            _ => (None, false),
        };

        let mut current = current;
        let mut initial_value = None;

        if let Some(owner) = owner {
            let lowered = if custom_index {
                self.lower_custom_index_borrow(owner, current)?
            } else {
                self.lower_expression(owner, current)?
            };

            let Some(continuation) = lowered.block else {
                return Ok(RootInitialization::Terminated(lowered));
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(owner));
            };

            current = continuation;
            initial_value = Some((owner, value));
        }

        self.initialize_access_storage(identity, current, initial_value)
    }

    fn initialize_access_storage(
        &mut self,
        identity: StorageIdentityId,
        current: MirBlockId,
        initial_value: Option<(BoundExpressionId, MirOperand)>,
    ) -> Result<RootInitialization, LoweringError> {
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

        if let Some((owner, value)) = initial_value
            && !value.reads_from(&place)
        {
            self.builder.push_operation(
                current,
                self.expression_source(owner)?,
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
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
        project_borrowed_root: bool,
    ) -> Result<MirPlace, LoweringError> {
        let access = self
            .input
            .storage_plan()
            .access(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?;

        if access.is_recovered() {
            return Err(LoweringError::UnsupportedStorageAccess(id));
        }

        let reached_type = access.reached_type();
        let source = access.source();

        let identity = self
            .input
            .storage_plan()
            .root_identity(id)
            .ok_or(LoweringError::MissingStorageIdentity(id))?;

        let projections = self
            .input
            .storage_plan()
            .resolved_projections(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?;

        let root_type = self.storage_identity_type(identity)?;

        let root = self.place_for_identity(
            identity,
            root_type,
            bray_bound_tree::BoundNodeOrigin::source(source),
        )?;

        let mut lowered =
            Vec::with_capacity(projections.len() + usize::from(project_borrowed_root));

        let mut source_type = if project_borrowed_root {
            self.append_entry_dereference(
                identity,
                reached_type,
                projections.is_empty(),
                &mut lowered,
            )?
        } else {
            root_type
        };

        for (index, projection) in projections.iter().copied().enumerate() {
            let Some(kind) = static_projection_kind(projection) else {
                return Err(LoweringError::UnsupportedStorageAccess(id));
            };

            let result_type = self.projection_result_type(identity, &projections[..=index])?;

            lowered.push(MirProjection::new(kind, source_type, result_type));
            source_type = result_type;
        }

        Ok(MirPlace::new(root.storage(), lowered, source_type))
    }

    pub(in crate::lowering) fn storage_identity_type(
        &self,
        identity: StorageIdentityId,
    ) -> Result<TypeId, LoweringError> {
        self.input
            .storage_plan()
            .storage_type(identity)
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))
    }

    fn append_entry_dereference(
        &self,
        identity: StorageIdentityId,
        reached_type: TypeId,
        has_no_explicit_projections: bool,
        projections: &mut Vec<MirProjection>,
    ) -> Result<TypeId, LoweringError> {
        let source_type = self.storage_identity_type(identity)?;

        if self.input.storage_plan().identity_type(identity).is_none()
            || has_no_explicit_projections && source_type == reached_type
        {
            return Ok(source_type);
        }

        let data = self
            .input
            .semantic_values()
            .type_data(source_type)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            return Ok(source_type);
        };

        projections.push(MirProjection::new(
            MirProjectionKind::Dereference,
            source_type,
            *target,
        ));

        Ok(*target)
    }

    fn projection_result_type(
        &self,
        identity: StorageIdentityId,
        projections: &[StorageProjection],
    ) -> Result<TypeId, LoweringError> {
        let plan = self.input.storage_plan();

        plan.access_entries()
            .find_map(|(access, model)| {
                (plan.root_identity(access) == Some(identity)
                    && plan.resolved_projections(access) == Some(projections))
                .then_some(model.reached_type())
            })
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))
    }

    fn lower_projection(
        &mut self,
        projection: StorageProjection,
        mut current: MirBlockId,
    ) -> Result<(MirBlockId, MirProjectionKind), LoweringError> {
        let kind = match static_projection_kind(projection) {
            Some(kind) => kind,
            None => match projection {
                StorageProjection::Element(selector) => {
                    let (continuation, selector) = self.lower_selector(selector, current)?;

                    current = continuation;

                    MirProjectionKind::Index(selector)
                }
                StorageProjection::SliceRange { start, end } => {
                    let (continuation, start) = self.lower_optional_selector(start, current)?;

                    let (continuation, end) = self.lower_optional_selector(end, continuation)?;

                    current = continuation;

                    MirProjectionKind::Slice { start, end }
                }
                _ => unreachable!("every static projection was handled above"),
            },
        };

        Ok((current, kind))
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

fn static_projection_kind(projection: StorageProjection) -> Option<MirProjectionKind> {
    match projection {
        StorageProjection::ProductField(field) => {
            Some(MirProjectionKind::Field(MirFieldReference::Struct(field)))
        }
        StorageProjection::TupleElement(ordinal) => {
            Some(MirProjectionKind::TupleField(ordinal.raw()))
        }
        StorageProjection::ElementFromStart(ordinal) => {
            Some(MirProjectionKind::ElementFromStart(ordinal.raw()))
        }
        StorageProjection::ElementFromEnd(ordinal) => {
            Some(MirProjectionKind::ElementFromEnd(ordinal.raw()))
        }
        StorageProjection::ActiveUnionPayloadField { variant, field } => {
            Some(MirProjectionKind::ActiveUnionPayloadField { variant, field })
        }
        StorageProjection::NullableValue => Some(MirProjectionKind::NullableValue),
        StorageProjection::OwnedTarget => Some(MirProjectionKind::Dereference),
        StorageProjection::Element(_) | StorageProjection::SliceRange { .. } => None,
    }
}

enum RootInitialization {
    Continuing {
        block: MirBlockId,
        storage: bray_ir::MirStorageId,
    },
    Terminated(LoweredExpression),
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::StorageProjection;
    use bray_ir::MirProjectionKind;
    use bray_symbols::SymbolOrdinal;

    use super::static_projection_kind;

    #[test]
    fn cleanup_places_retain_static_projection_paths() {
        let projection =
            static_projection_kind(StorageProjection::TupleElement(SymbolOrdinal::new(2)));

        assert!(matches!(projection, Some(MirProjectionKind::TupleField(2))));
    }
}
