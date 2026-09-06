use crate::lowering::LoweringError;
use crate::lowering::block::LoweredExpression;
use crate::lowering::lowerer::Lowerer;
use crate::lowering::projection::static_projection_kind;
use bray_bound_tree::{
    BoundExpressionId, StorageAccessId, StorageAccessPurpose, StorageIdentity, StorageIdentityId,
    StorageOperationStatus, StorageProjection,
};
use bray_ir::{MirBlockId, MirPlace, MirProjection};
use bray_symbols::{BorrowKind, TypeId};
pub(in crate::lowering) enum LoweredPlace {
    Continuing { block: MirBlockId, place: MirPlace },
    Terminated(LoweredExpression),
}

pub(in crate::lowering::expression) enum RootInitialization {
    Continuing { block: MirBlockId, place: MirPlace },
    Terminated(LoweredExpression),
}

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_expression_place(
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
        self.storage_decision_matching(expression, |plan| accepts(plan.purpose()))
    }

    pub(in crate::lowering) fn storage_decision_reaching(
        &self,
        expression: BoundExpressionId,
        reached_type: TypeId,
        accepts: impl Fn(StorageAccessPurpose) -> bool,
    ) -> Result<bray_bound_tree::StorageOperationDecision, LoweringError> {
        self.storage_decision_matching(expression, |plan| {
            accepts(plan.purpose())
                && self
                    .input
                    .storage_plan()
                    .access(plan.access())
                    .is_some_and(|access| access.reached_type() == reached_type)
        })
    }

    fn storage_decision_matching(
        &self,
        expression: BoundExpressionId,
        accepts: impl Fn(bray_bound_tree::StorageAccessPlan) -> bool,
    ) -> Result<bray_bound_tree::StorageOperationDecision, LoweringError> {
        let plan = self
            .input
            .storage_plan()
            .expression_plans(expression)
            .find(|plan| accepts(*plan))
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

        let (mut current, root) =
            match self.initialize_access_root(id, identity, expression, current)? {
                RootInitialization::Continuing { block, place } => (block, place),
                RootInitialization::Terminated(completion) => {
                    return Ok(LoweredPlace::Terminated(completion));
                }
            };

        let mut lowered = Vec::with_capacity(root.projections().len() + projections.len() + 2);
        lowered.extend(root.projections().iter().cloned());
        let mut storage = root.storage();

        let mut source_type = self.append_entry_dereference(
            identity,
            reached_type,
            projections.is_empty(),
            &mut lowered,
        )?;

        for (index, projection) in projections.iter().copied().enumerate() {
            source_type = self.append_projection_dereferences(source_type, &mut lowered)?;

            let result_type = self.projection_result_type(identity, &projections[..=index])?;

            if projection == StorageProjection::OwnedTarget {
                let mutable = self
                    .input
                    .storage_flow()
                    .operations()
                    .iter()
                    .any(|operation| {
                        operation.access() == id
                            && matches!(
                                operation.purpose(),
                                StorageAccessPurpose::Move
                                    | StorageAccessPurpose::Write
                                    | StorageAccessPurpose::Assignment
                                    | StorageAccessPurpose::Initialize
                                    | StorageAccessPurpose::Borrow(BorrowKind::Mutable)
                            )
                    });

                let kind = if mutable && self.guard_bindings.is_empty() {
                    BorrowKind::Mutable
                } else {
                    BorrowKind::Shared
                };

                let call = self
                    .owned_target_call(source_type, kind)
                    .ok_or(LoweringError::UnsupportedStorageAccess(id))?;

                let owner = MirPlace::new(storage, lowered, source_type);
                let source = self.expression_source(expression)?;

                let target =
                    self.project_owned_target(current, &source, &owner, call, result_type)?;

                storage = target.storage();
                lowered = target.projections().to_vec();
                source_type = result_type;
                continue;
            }

            let (continuation, kind) = self.lower_projection(projection, current)?;

            current = continuation;
            lowered.push(MirProjection::new(kind, source_type, result_type));
            source_type = result_type;
        }

        source_type = self.append_reached_dereference(source_type, reached_type, &mut lowered)?;

        Ok(LoweredPlace::Continuing {
            block: current,
            place: MirPlace::new(storage, lowered, source_type),
        })
    }

    pub(in crate::lowering) fn lower_access_place_with(
        &mut self,
        expression: BoundExpressionId,
        id: StorageAccessId,
        current: MirBlockId,
        continuation: impl FnOnce(
            &mut Self,
            MirBlockId,
            MirPlace,
        ) -> Result<LoweredExpression, LoweringError>,
    ) -> Result<LoweredExpression, LoweringError> {
        match self.lower_access_place(expression, id, current)? {
            LoweredPlace::Continuing { block, place } => continuation(self, block, place),
            LoweredPlace::Terminated(completion) => Ok(completion),
        }
    }

    pub(in crate::lowering) fn lower_materialized_access_place_with(
        &mut self,
        expression: BoundExpressionId,
        id: StorageAccessId,
        current: MirBlockId,
        continuation: impl FnOnce(
            &mut Self,
            MirBlockId,
            MirPlace,
        ) -> Result<LoweredExpression, LoweringError>,
    ) -> Result<LoweredExpression, LoweringError> {
        let temporary = self
            .input
            .storage_plan()
            .root_identity(id)
            .filter(|identity| !self.storages.contains_key(identity))
            .and_then(|identity| {
                self.input
                    .storage_plan()
                    .identity(identity)
                    .and_then(|model| match model {
                        StorageIdentity::Temporary(owner) if owner == expression => Some(owner),
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

        self.lower_access_place_with(expression, id, current, continuation)
    }

    fn initialize_access_root(
        &mut self,
        access: StorageAccessId,
        identity: StorageIdentityId,
        expression: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<RootInitialization, LoweringError> {
        if let Some(place) = self.guard_binding(identity) {
            return Ok(RootInitialization::Continuing {
                block: current,
                place: Self::retained_place(place),
            });
        }

        let model = self
            .input
            .storage_plan()
            .identity(identity)
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

        let static_reference = if let StorageIdentity::Static(declaration) = model {
            let selection = crate::lowering::expression::static_access::static_reference(
                &self.input,
                access,
                expression,
                declaration,
            )?;

            self.static_accesses.insert(access, selection.clone());

            Some(selection)
        } else {
            None
        };

        let existing = static_reference.as_ref().map_or_else(
            || self.storages.get(&identity).copied(),
            |reference| self.static_storages.get(reference).copied(),
        );

        if let Some(storage) = existing {
            return Ok(RootInitialization::Continuing {
                block: current,
                place: MirPlace::new(storage, [], self.storage_identity_type(identity)?),
            });
        }

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

        self.initialize_access_storage(identity, current, initial_value, static_reference.as_ref())
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
        let static_reference = self.static_accesses.get(&id).cloned();

        let root = self.place_for_identity_with_static(
            identity,
            root_type,
            bray_bound_tree::BoundNodeOrigin::source(source),
            static_reference.as_ref(),
        )?;

        let mut lowered = Vec::with_capacity(
            root.projections().len() + projections.len() + usize::from(project_borrowed_root) + 1,
        );

        lowered.extend(root.projections().iter().cloned());

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
            source_type = self.append_projection_dereferences(source_type, &mut lowered)?;

            let Some(kind) = static_projection_kind(projection) else {
                return Err(LoweringError::UnsupportedStorageAccess(id));
            };

            let result_type = self.projection_result_type(identity, &projections[..=index])?;

            lowered.push(MirProjection::new(kind, source_type, result_type));
            source_type = result_type;
        }

        if project_borrowed_root {
            source_type =
                self.append_reached_dereference(source_type, reached_type, &mut lowered)?;
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
}
