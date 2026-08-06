use bray_bound_tree::{
    BoundExpressionId, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode,
    BoundPatternTarget, BoundReferenceTarget, PatternOperation, PatternProjection, StorageAccess,
    StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget,
    StorageIdentity, StorageProjection,
};

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
    ) -> Result<(), PlanError> {
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
            self.install_alternative_bindings(&pattern)?;
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

                self.record_purpose(
                    subject_expression,
                    Some(StorageAccessPurpose::Projection),
                    access,
                )?;

                access
            }
            None => parent_access,
        };

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

                    self.record_purpose(
                        subject_expression,
                        Some(StorageAccessPurpose::Projection),
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
            self.plan_pattern_target(subject_expression, pattern.mode(), target)?;
        }

        for child in pattern.children() {
            self.plan_pattern(*child, subject_expression, access)?;
        }

        if pattern.kind() == BoundPatternKind::Alternative {
            self.finalize_alternative_bindings(id, &pattern)?;
        }

        Ok(())
    }

    fn bind_pattern_local(
        &mut self,
        binding: bray_symbols::LocalBindingSymbolId,
        pattern: BoundPatternId,
        subject_expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Result<(), PlanError> {
        let checked = self
            .patterns
            .binding_type(binding)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

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
                    let identity = self.bind_identity(
                        target,
                        StorageIdentity::LocalOwned(pattern.into()),
                        Some(checked.ty()),
                    )?;

                    if let Some(identity) = identity {
                        let pattern = self
                            .request
                            .view()
                            .pattern(pattern)
                            .ok_or_else(|| invalid_node(pattern))?;

                        let access = StorageAccess::new(
                            StorageAccessRoot::Storage(identity),
                            [],
                            checked.ty(),
                            pattern.origin().source_anchor(),
                            pattern.is_recovered() || checked.is_recovered(),
                        );

                        self.builder_mut()?
                            .push_access(access)
                            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
                    }

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
                    .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
            }
        }

        let purpose = match checked.operation() {
            PatternOperation::Consume => {
                match self.request.view().pattern(pattern).map(BoundPattern::mode) {
                    Some(BoundPatternMode::MatchConsume) => StorageAccessPurpose::Move,
                    Some(
                        BoundPatternMode::Declaration
                        | BoundPatternMode::Assignment
                        | BoundPatternMode::MatchObserve,
                    )
                    | None => StorageAccessPurpose::ValueTransfer,
                }
            }
            PatternOperation::Copy => StorageAccessPurpose::Copy,
            PatternOperation::Observe => StorageAccessPurpose::Read,
            PatternOperation::SharedBorrow => {
                StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Shared)
            }
            PatternOperation::MutableBorrow => {
                StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable)
            }
            PatternOperation::Recovered => StorageAccessPurpose::Projection,
        };

        self.record_purpose(subject_expression, Some(purpose), access)
    }

    fn install_alternative_bindings(&mut self, pattern: &BoundPattern) -> Result<(), PlanError> {
        let bindings = self.descendant_bindings(pattern)?;

        for binding in bindings {
            let checked = self
                .patterns
                .binding_type(binding)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

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
        pattern: &BoundPattern,
    ) -> Result<(), PlanError> {
        for binding in self.descendant_bindings(pattern)? {
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
                .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;

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
                    .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
            }
        }

        Ok(())
    }

    fn descendant_bindings(
        &self,
        pattern: &BoundPattern,
    ) -> Result<Vec<bray_symbols::LocalBindingSymbolId>, PlanError> {
        let mut bindings = pattern.bindings().to_vec();

        bindings.extend(pattern.entries().iter().filter_map(|entry| entry.binding()));

        let mut pending = pattern.children().to_vec();

        while let Some(pattern) = pending.pop() {
            self.check_cancellation()?;

            let pattern = self
                .request
                .view()
                .pattern(pattern)
                .ok_or_else(|| invalid_node(pattern))?;

            bindings.extend_from_slice(pattern.bindings());
            bindings.extend(pattern.entries().iter().filter_map(|entry| entry.binding()));
            pending.extend_from_slice(pattern.children());
        }

        bindings.sort_unstable();
        bindings.dedup();

        Ok(bindings)
    }

    fn plan_pattern_target(
        &mut self,
        subject_expression: BoundExpressionId,
        mode: BoundPatternMode,
        target: BoundPatternTarget,
    ) -> Result<(), PlanError> {
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

        self.record_purpose(subject_expression, Some(purpose), access)
    }

    fn project_pattern_access(
        &mut self,
        pattern: BoundPatternId,
        base: StorageAccessId,
        projection: PatternProjection,
        reached_type: bray_symbols::TypeId,
        is_recovered: bool,
    ) -> Result<StorageAccessId, PlanError> {
        let projection = match projection {
            PatternProjection::ProductField(field) => StorageProjection::ProductField(field),
            PatternProjection::TupleElement(ordinal) => StorageProjection::TupleElement(ordinal),
            PatternProjection::ActiveUnionPayloadField { variant, field } => {
                StorageProjection::ActiveUnionPayloadField { variant, field }
            }
            PatternProjection::ElementFromStart(ordinal) => {
                StorageProjection::ElementFromStart(ordinal)
            }
            PatternProjection::ElementFromEnd(ordinal) => {
                StorageProjection::ElementFromEnd(ordinal)
            }
            PatternProjection::NullableValue => StorageProjection::NullableValue,
            PatternProjection::OwnedTarget => StorageProjection::OwnedTarget,
        };

        let base = self
            .builder()?
            .access(base)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        let root = base.root();
        let mut projections = base.projections().to_vec();

        projections.push(projection);

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
            .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan.into())
    }
}
