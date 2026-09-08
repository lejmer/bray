use bray_bound_tree::{
    BoundExpressionId, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode,
    BoundPatternTarget, BoundReferenceTarget, PatternOperation, PatternProjection, StorageAccess,
    StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget,
    StorageIdentity, StorageProjection,
};
use bray_symbols::TypeData;

use super::plan::{PlanError, Planner, invalid_node};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C> Planner<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn plan_pattern(
        &mut self,
        id: BoundPatternId,
        subject_expression: BoundExpressionId,
        parent_access: StorageAccessId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        self.check_cancellation()?;

        if !self.planned_patterns.insert(id) {
            return Ok(());
        }

        // The immutable pattern is cloned so recursive planning can mutably advance
        // task-local state.
        let pattern = self
            .request
            .view()
            .pattern(id)
            .ok_or_else(|| invalid_node(id))?
            .clone();

        if pattern.kind() == BoundPatternKind::Alternative {
            self.install_alternative_bindings(id)?;
        }

        let checked = self
            .patterns
            .pattern(id)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let access = match checked.projection() {
            Some(projection) => {
                let access = self.project_pattern_access(
                    id,
                    parent_access,
                    projection,
                    checked.input_type(),
                    checked.is_recovered(),
                )?;

                self.record_pattern_purpose(
                    id,
                    subject_expression,
                    StorageAccessPurpose::Projection,
                    access,
                )?;

                access
            }
            None => parent_access,
        };

        self.builder_mut()?
            .bind(
                StorageBindingTarget::PatternSubject(id),
                StorageBinding::Access(access),
            )
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        if pattern.kind() == BoundPatternKind::Discard {
            let transfers_borrow = checked.operation() == PatternOperation::Consume
                && self.type_is_borrow(checked.input_type())?;

            if checked.operation() == PatternOperation::Consume {
                self.bind_owned_pattern_storage(
                    StorageBindingTarget::PatternDiscard(id),
                    id,
                    checked.input_type(),
                    checked.is_recovered(),
                )?;
            }

            self.record_pattern_purpose(
                id,
                subject_expression,
                pattern_operation_purpose(pattern.mode(), checked.operation(), transfers_borrow),
                access,
            )?;
        }

        if checked.target().is_none() {
            for binding in pattern.bindings() {
                self.bind_pattern_local(*binding, id, subject_expression, access)?;
            }
        }

        for entry in pattern.entries() {
            let Some(binding) = entry.binding() else {
                continue;
            };

            let checked = self
                .patterns
                .binding_type(binding)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

            let access = match checked.projection() {
                Some(projection) => {
                    let access = self.project_pattern_access(
                        id,
                        access,
                        projection,
                        checked.ty(),
                        checked.is_recovered(),
                    )?;

                    self.record_pattern_purpose(
                        id,
                        subject_expression,
                        StorageAccessPurpose::Projection,
                        access,
                    )?;

                    access
                }
                None => access,
            };

            self.bind_pattern_local(binding, id, subject_expression, access)?;
        }

        if let Some(target) = checked
            .target()
            .filter(|target| pattern.mode() == BoundPatternMode::Assignment || target.is_constant())
        {
            self.plan_pattern_target(id, subject_expression, pattern.mode(), target)?;
        }

        for child in pattern.children() {
            self.plan_pattern(*child, subject_expression, access)?;
        }

        if pattern.kind() == BoundPatternKind::Alternative {
            self.finalize_alternative_bindings(id)?;
        }

        Ok(())
    }

    fn bind_pattern_local(
        &mut self,
        binding: bray_symbols::LocalBindingSymbolId,
        pattern: BoundPatternId,
        subject_expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let checked = self
            .patterns
            .binding_type(binding)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let target = StorageBindingTarget::Local(binding);

        let transfers_borrow = checked.operation() == PatternOperation::Consume
            && self.type_is_borrow(checked.ty())?;

        if let Some(alternatives) = self.alternative_pattern_bindings.get_mut(&binding) {
            for accesses in alternatives {
                accesses.push(access);
            }
        } else {
            let binding = match checked.operation() {
                PatternOperation::Consume
                | PatternOperation::Copy
                | PatternOperation::Recovered => {
                    self.bind_owned_pattern_storage(
                        target,
                        pattern,
                        checked.ty(),
                        checked.is_recovered(),
                    )?;

                    None
                }
                PatternOperation::Observe => Some(StorageBinding::Access(access)),
                PatternOperation::SharedBorrow => {
                    Some(StorageBinding::Access(self.borrow_access(
                        subject_expression,
                        access,
                        bray_symbols::BorrowKind::Shared,
                    )?))
                }
                PatternOperation::MutableBorrow => {
                    Some(StorageBinding::Access(self.borrow_access(
                        subject_expression,
                        access,
                        bray_symbols::BorrowKind::Mutable,
                    )?))
                }
            };

            if let Some(binding) = binding {
                self.builder_mut()?
                    .bind(target, binding)
                    .map_err(CheckerInfrastructureError::StoragePlan)?;
            }
        }

        let mode = self
            .request
            .view()
            .pattern(pattern)
            .map(BoundPattern::mode)
            .ok_or_else(|| invalid_node(pattern))?;

        let purpose = pattern_operation_purpose(mode, checked.operation(), transfers_borrow);

        self.record_pattern_purpose(pattern, subject_expression, purpose, access)
    }

    fn bind_owned_pattern_storage(
        &mut self,
        target: StorageBindingTarget,
        pattern: BoundPatternId,
        ty: bray_symbols::TypeId,
        is_recovered: bool,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let identity = self.bind_identity(
            target,
            StorageIdentity::LocalOwned(pattern.into()),
            Some(ty),
        )?;

        let Some(identity) = identity else {
            return Ok(());
        };

        let pattern = self
            .request
            .view()
            .pattern(pattern)
            .ok_or_else(|| invalid_node(pattern))?;

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(identity),
            [],
            ty,
            pattern.origin().source_anchor(),
            pattern.is_recovered() || is_recovered,
        );

        self.builder_mut()?
            .push_access(access)
            .map_err(CheckerInfrastructureError::StoragePlan)?;

        Ok(())
    }

    fn type_is_borrow(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<bool, PlanError<C::UpstreamError>> {
        let data = self
            .request
            .semantic_values()
            .type_data(ty)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        Ok(matches!(data.as_ref(), TypeData::Borrow { .. }))
    }

    fn install_alternative_bindings(
        &mut self,
        pattern: BoundPatternId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let bindings = self.descendant_bindings(pattern)?;

        for binding in bindings {
            let checked = self
                .patterns
                .binding_type(binding)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

            if matches!(
                checked.operation(),
                PatternOperation::Consume | PatternOperation::Copy
            ) {
                self.bind_owned_pattern_storage(
                    StorageBindingTarget::Local(binding),
                    pattern,
                    checked.ty(),
                    checked.is_recovered(),
                )?;

                continue;
            }

            if !matches!(
                checked.operation(),
                PatternOperation::Observe
                    | PatternOperation::SharedBorrow
                    | PatternOperation::MutableBorrow
            ) {
                continue;
            }

            self.alternative_pattern_bindings
                .entry(binding)
                .or_default()
                .push(Vec::new());
        }

        Ok(())
    }

    fn finalize_alternative_bindings(
        &mut self,
        pattern_id: BoundPatternId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let pattern = self
            .request
            .view()
            .pattern(pattern_id)
            .ok_or_else(|| invalid_node(pattern_id))?;

        for binding in self.descendant_bindings(pattern_id)? {
            let Some(alternatives) = self.alternative_pattern_bindings.get_mut(&binding) else {
                continue;
            };

            let Some(accesses) = alternatives.pop() else {
                return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
            };

            if accesses.is_empty() {
                return Err(CheckerInfrastructureError::InvalidStoragePlan.into());
            }

            if !alternatives.is_empty() {
                continue;
            }

            self.alternative_pattern_bindings.remove(&binding);

            let ty = self
                .patterns
                .binding_type(binding)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?
                .ty();

            let alternative = self
                .builder_mut()?
                .push_alternative(pattern_id, accesses)
                .map_err(CheckerInfrastructureError::StoragePlan)?;

            let identity = self.bind_identity(
                StorageBindingTarget::Local(binding),
                StorageIdentity::Alternative {
                    pattern: pattern_id,
                    alternative,
                },
                Some(ty),
            )?;

            if let Some(identity) = identity {
                let access = StorageAccess::new(
                    StorageAccessRoot::Storage(identity),
                    [],
                    ty,
                    pattern.origin().source_anchor(),
                    pattern.is_recovered(),
                );

                self.builder_mut()?
                    .push_access(access)
                    .map_err(CheckerInfrastructureError::StoragePlan)?;
            }
        }

        Ok(())
    }

    fn descendant_bindings(
        &self,
        pattern: BoundPatternId,
    ) -> Result<Vec<bray_symbols::LocalBindingSymbolId>, PlanError<C::UpstreamError>> {
        let mut bindings = Vec::new();
        let mut pending = vec![pattern];

        while let Some(pattern) = pending.pop() {
            self.check_cancellation()?;

            let checked = self
                .patterns
                .pattern(pattern)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

            let pattern = self
                .request
                .view()
                .pattern(pattern)
                .ok_or_else(|| invalid_node(pattern))?;

            bindings.extend(checked.bindings(pattern));
            pending.extend_from_slice(pattern.children());
        }

        bindings.sort_unstable();
        bindings.dedup();

        Ok(bindings)
    }

    fn plan_pattern_target(
        &mut self,
        pattern: BoundPatternId,
        subject_expression: BoundExpressionId,
        mode: BoundPatternMode,
        target: BoundPatternTarget,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let target = match target {
            BoundPatternTarget::Local(target) => BoundReferenceTarget::Local(target),
            BoundPatternTarget::Surface(target) => BoundReferenceTarget::Surface(target),
        };

        let access = self.reference_access(subject_expression, target)?;

        let purpose = match mode {
            BoundPatternMode::Assignment => StorageAccessPurpose::Assignment,
            BoundPatternMode::Declaration
            | BoundPatternMode::MatchObserve
            | BoundPatternMode::MatchConsume => StorageAccessPurpose::Read,
        };

        self.record_pattern_purpose(pattern, subject_expression, purpose, access)
    }

    fn record_pattern_purpose(
        &mut self,
        pattern: BoundPatternId,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        self.builder_mut()?
            .plan_access(pattern.into(), expression, purpose, access)
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }

    fn project_pattern_access(
        &mut self,
        pattern: BoundPatternId,
        base: StorageAccessId,
        projection: PatternProjection,
        reached_type: bray_symbols::TypeId,
        mut is_recovered: bool,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let projection = StorageProjection::from(projection);

        let base = self
            .builder()?
            .access(base)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = base.root();
        let owner = base.reached_type();
        let mut projections = base.projections().to_vec();

        projections.push(projection);

        if projection == StorageProjection::OwnedTarget {
            is_recovered |= !self.plan_owned_borrows(owner)?;
        }

        let pattern = self
            .request
            .view()
            .pattern(pattern)
            .ok_or_else(|| invalid_node(pattern))?;

        let access = StorageAccess::new(
            root,
            projections,
            reached_type,
            pattern.origin().source_anchor(),
            pattern.is_recovered() || is_recovered,
        );

        self.builder_mut()?
            .push_access(access)
            .map_err(|error| CheckerInfrastructureError::StoragePlan(error).into())
    }

    fn plan_owned_borrows(
        &mut self,
        owner: bray_symbols::TypeId,
    ) -> Result<bool, PlanError<C::UpstreamError>> {
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

        let TypeData::OwnedIndirection { storage, target } = data.as_ref() else {
            return Ok(false);
        };

        let mut complete = true;

        for (kind, member) in [
            (bray_symbols::BorrowKind::Shared, "StorageBorrow"),
            (bray_symbols::BorrowKind::Mutable, "StorageBorrowMut"),
        ] {
            let member = bray_compiler_known::CompilerKnownDeclarationKey::try_new(member)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

            let selected = super::selected_storage_protocol_call(
                self.request.context(),
                *storage,
                *target,
                &member,
            )?;

            let (call, diagnostics) = selected.into_parts();

            self.diagnostics.add_range(diagnostics);

            match call {
                Some(call) => self
                    .builder_mut()?
                    .set_owned_borrow(owner, kind, call)
                    .map_err(CheckerInfrastructureError::StoragePlan)?,
                None => complete = false,
            }
        }

        Ok(complete)
    }
}

const fn pattern_operation_purpose(
    mode: BoundPatternMode,
    operation: PatternOperation,
    transfers_borrow: bool,
) -> StorageAccessPurpose {
    match operation {
        PatternOperation::Consume if transfers_borrow => StorageAccessPurpose::Read,
        PatternOperation::Consume if matches!(mode, BoundPatternMode::MatchConsume) => {
            StorageAccessPurpose::Move
        }
        PatternOperation::Consume => StorageAccessPurpose::ValueTransfer,
        PatternOperation::Copy => StorageAccessPurpose::Copy,
        PatternOperation::Observe => StorageAccessPurpose::Read,
        PatternOperation::SharedBorrow => {
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Shared)
        }
        PatternOperation::MutableBorrow => {
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable)
        }
        PatternOperation::Recovered => StorageAccessPurpose::Projection,
    }
}
