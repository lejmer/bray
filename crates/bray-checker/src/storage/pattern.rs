use bray_bound_tree::{
    BoundExpressionId, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternTarget,
    BoundReferenceTarget, PatternOperation, PatternProjection, StorageAccess, StorageAccessId,
    StorageAccessPurpose, StorageBinding, StorageBindingTarget, StorageIdentity, StorageProjection,
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

        for binding in pattern.bindings() {
            self.bind_pattern_local(*binding, id, subject_expression, access)?;
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

        if let Some(target) = pattern.target() {
            self.plan_pattern_target(subject_expression, target)?;
        }

        for child in pattern.children() {
            self.plan_pattern(*child, subject_expression, access)?;
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

        if !self.conservative_pattern_bindings.contains(&binding) {
            match checked.operation() {
                PatternOperation::Consume
                | PatternOperation::Copy
                | PatternOperation::Recovered => {
                    self.bind_identity(target, StorageIdentity::LocalOwned(pattern.into()))?;
                }
                PatternOperation::Observe
                | PatternOperation::SharedBorrow
                | PatternOperation::MutableBorrow => {
                    self.builder_mut()?
                        .bind(target, StorageBinding::Access(access))
                        .map_err(|_| CheckerInfrastructureError::InvalidStoragePlan)?;
                }
            }
        }

        let purpose = match checked.operation() {
            PatternOperation::Consume => StorageAccessPurpose::Move,
            PatternOperation::Copy | PatternOperation::Observe => StorageAccessPurpose::Read,
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
        let source = pattern.origin().source_anchor();

        // TODO(BRA-268): Replace recovery-backed logical storage with exact branch-dependent
        // alias alternatives once the storage relationship domain can retain them.
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

            self.conservative_pattern_bindings.insert(binding);

            let target = StorageBindingTarget::Local(binding);

            if self.builder()?.binding(target).is_none() {
                self.bind_identity(target, StorageIdentity::Error(source))?;
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
        target: BoundPatternTarget,
    ) -> Result<(), PlanError> {
        let target = match target {
            BoundPatternTarget::Local(target) => BoundReferenceTarget::Local(target),
            BoundPatternTarget::Surface(target) => BoundReferenceTarget::Surface(target),
        };

        let access = self.reference_access(subject_expression, target)?;

        self.record_purpose(
            subject_expression,
            Some(StorageAccessPurpose::Write),
            access,
        )
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
