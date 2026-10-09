use bray_bound_tree::{
    BoundExpressionId, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode,
    BoundPatternTarget, BoundReferenceTarget, PatternOperation, PatternProjection, StorageAccess,
    StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget,
    StorageIdentity, StorageProjection,
};
use bray_symbols::TypeData;

use super::plan::{PlanError, Planner, missing_node};
use crate::CheckerRequestContext;

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
            .unwrap_or_else(|| missing_node(id))
            .clone();

        if pattern.kind() == BoundPatternKind::Alternative {
            self.install_alternative_bindings(id)?;
        }

        let checked = self
            .patterns
            .pattern(id).unwrap_or_else(|| panic!("plan_pattern requires checked pattern, id: {id:?}, subject_expression: {subject_expression:?}, parent_access: {parent_access:?}"));

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

        self.builder_mut()
            .bind(
                StorageBindingTarget::PatternSubject(id),
                StorageBinding::Access(access),
            ).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in plan_pattern: {error:?}"));

        if pattern.kind() == BoundPatternKind::Discard {
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
                pattern_operation_purpose(pattern.mode(), checked.operation()),
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
                .binding_type(binding).unwrap_or_else(|| panic!("plan_pattern requires checked pattern binding type, id: {id:?}, subject_expression: {subject_expression:?}, parent_access: {parent_access:?}, binding: {binding:?}"));

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
            .binding_type(binding).unwrap_or_else(|| panic!("bind_pattern_local requires checked pattern binding type, binding: {binding:?}, pattern: {pattern:?}, subject_expression: {subject_expression:?}, access: {access:?}"));

        let target = StorageBindingTarget::Local(binding);

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

                    if self.type_is_borrow(checked.ty())
                        && self
                            .request
                            .view()
                            .pattern(pattern)
                            .is_some_and(|pattern| !pattern.is_mutable())
                        && let Some(StorageBinding::Identity(identity)) =
                            self.builder().binding(target)
                    {
                        // An immutable binding keeps this exact source for every later projection.
                        self.borrowed_values.insert(identity, access);
                    }

                    None
                }
                PatternOperation::Observe => Some(StorageBinding::Access(access)),
                PatternOperation::SharedBorrow => {
                    Some(StorageBinding::Access(self.borrow_access(
                        subject_expression,
                        access,
                        bray_symbols::BorrowKind::Shared,
                        self.expression_type(subject_expression).ty(),
                    )?))
                }
                PatternOperation::MutableBorrow => {
                    Some(StorageBinding::Access(self.borrow_access(
                        subject_expression,
                        access,
                        bray_symbols::BorrowKind::Mutable,
                        self.expression_type(subject_expression).ty(),
                    )?))
                }
            };

            if let Some(binding) = binding {
                self.builder_mut()
                    .bind(target, binding).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_pattern_local: {error:?}"));
            }
        }

        let mode = self
            .request
            .view()
            .pattern(pattern)
            .map(BoundPattern::mode)
            .unwrap_or_else(|| missing_node(pattern));

        let purpose = pattern_operation_purpose(mode, checked.operation());

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
            .unwrap_or_else(|| missing_node(pattern));

        let access = StorageAccess::new(
            StorageAccessRoot::Storage(identity),
            [],
            ty,
            pattern.origin().source_anchor(),
            pattern.is_recovered() || is_recovered,
        );

        self.builder_mut()
            .push_access(access).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in bind_owned_pattern_storage: {error:?}"));

        Ok(())
    }

    fn type_is_borrow(&self, ty: bray_symbols::TypeId) -> bool {
        let data = self.request.semantic_values().type_data(ty);

        matches!(data.as_ref(), TypeData::Borrow { .. })
    }

    fn install_alternative_bindings(
        &mut self,
        pattern: BoundPatternId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let bindings = self.descendant_bindings(pattern)?;

        for binding in bindings {
            let checked = self
                .patterns
                .binding_type(binding).unwrap_or_else(|| panic!("install_alternative_bindings requires checked pattern binding type, pattern: {pattern:?}, binding: {binding:?}"));

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
            .unwrap_or_else(|| missing_node(pattern_id));

        for binding in self.descendant_bindings(pattern_id)? {
            let Some(alternatives) = self.alternative_pattern_bindings.get_mut(&binding) else {
                continue;
            };

            let Some(accesses) = alternatives.pop() else {
                panic!(
                    "Storage-planning inputs or constructed records violate the requested unit contract. in finalize_alternative_bindings"
                );
            };

            if accesses.is_empty() {
                panic!(
                    "Storage-planning inputs or constructed records violate the requested unit contract. in finalize_alternative_bindings"
                );
            }

            if !alternatives.is_empty() {
                continue;
            }

            self.alternative_pattern_bindings.remove(&binding);

            let ty = self
                .patterns
                .binding_type(binding).unwrap_or_else(|| panic!("finalize_alternative_bindings requires checked pattern binding type, pattern_id: {pattern_id:?}, binding: {binding:?}"))
                .ty();

            let alternative = self
                .builder_mut()
                .push_alternative(pattern_id, accesses).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in finalize_alternative_bindings: {error:?}"));

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

                self.builder_mut()
                    .push_access(access).unwrap_or_else(|error| panic!("The storage-plan builder rejected one exact relationship. in finalize_alternative_bindings: {error:?}"));
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

            let checked = self.patterns.pattern(pattern).unwrap_or_else(|| {
                panic!("descendant_bindings requires checked pattern, pattern: {pattern:?}")
            });

            let pattern = self
                .request
                .view()
                .pattern(pattern)
                .unwrap_or_else(|| missing_node(pattern));

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
            | BoundPatternMode::Scoped
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
        let purpose = self.materialization_purpose(expression, purpose, access)?;

        self.builder_mut()
            .plan_access(pattern.into(), expression, purpose, access).unwrap_or_else(|error| panic!("record_pattern_purpose must satisfy its checked construction contract: {error:?}"));

        Ok(())
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
            .builder()
            .access(base).unwrap_or_else(|| panic!("project_pattern_access requires planned storage access, pattern: {pattern:?}, base: {base:?}, reached_type: {reached_type:?}"));

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
            .unwrap_or_else(|| missing_node(pattern));

        let access = StorageAccess::new(
            root,
            projections,
            reached_type,
            pattern.origin().source_anchor(),
            pattern.is_recovered() || is_recovered,
        );

        Ok(self.builder_mut()
            .push_access(access).unwrap_or_else(|error| panic!("project_pattern_access must satisfy its checked construction contract: {error:?}")))
    }
}

const fn pattern_operation_purpose(
    mode: BoundPatternMode,
    operation: PatternOperation,
) -> StorageAccessPurpose {
    match operation {
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
