use bray_bound_tree::{
    BoundCallableTarget, BoundExpressionId, SelectedOperation, SemanticSelection, StorageAccessId,
    StorageAccessPurpose,
};
use bray_symbols::{BorrowKind, ReceiverMode, TypeData};

use super::super::plan::{PlanError, Planner};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn plan_call(
        &mut self,
        id: BoundExpressionId,
        callee: BoundExpressionId,
        arguments: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Result<StorageAccessId, PlanError<C::UpstreamError>> {
        let semantic_selection = self.selections.expression(id);

        let selection = semantic_selection.and_then(|selection| {
            let SemanticSelection::Call(call) = selection else {
                return None;
            };

            Some(call)
        });

        let direct = selection
            .is_some_and(|call| matches!(call.target(), BoundCallableTarget::Declaration(_)))
            || matches!(
                semantic_selection,
                Some(SemanticSelection::Operation(
                    SelectedOperation::Construction(_)
                ))
            );

        if !direct {
            self.plan_expression(callee, Some(StorageAccessPurpose::Read))?;
        }

        if let Some(receiver) = selection.and_then(bray_bound_tree::SelectedCall::receiver) {
            self.plan_receiver(
                receiver.expression().into(),
                receiver.expression(),
                receiver.mode(),
            )?;
        }

        let mut reborrows = selection
            .into_iter()
            .flat_map(|call| call.arguments())
            .filter_map(|argument| match argument {
                bray_bound_tree::SelectedArgument::Explicit { reborrow, .. } => Some(*reborrow),
                bray_bound_tree::SelectedArgument::Default { .. } => None,
            });

        let observational = matches!(semantic_selection, Some(SemanticSelection::Predicate(_)))
            || selection
                .is_some_and(|call| matches!(call.target(), BoundCallableTarget::Predicate(_)));

        for argument in arguments {
            if observational {
                self.plan_expression(argument, Some(StorageAccessPurpose::Read))?;
            } else if let Some(kind) = reborrows.next().flatten() {
                self.plan_call_reborrow(argument, kind)?;
            } else {
                self.plan_expression(argument, Some(StorageAccessPurpose::ValueTransfer))?;
            }
        }

        self.temporary_access(id)
    }

    pub(super) const fn call_receiver_purpose(mode: ReceiverMode) -> StorageAccessPurpose {
        match mode {
            ReceiverMode::Shared => StorageAccessPurpose::Borrow(BorrowKind::Shared),
            ReceiverMode::Mutable => StorageAccessPurpose::Borrow(BorrowKind::Mutable),
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => {
                StorageAccessPurpose::ValueTransfer
            }
        }
    }

    pub(super) fn plan_receiver(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        expression: BoundExpressionId,
        mode: bray_symbols::ReceiverMode,
    ) -> Result<bray_bound_tree::StorageAccessId, PlanError<C::UpstreamError>> {
        let source = self.plan_expression(expression, Some(StorageAccessPurpose::Read))?;
        let purpose = Self::call_receiver_purpose(mode);

        let existing = self.has_fresh_borrow(expression, source);

        let source = if let StorageAccessPurpose::Borrow(kind) = purpose
            && !existing
        {
            // The invocation retains the loan separately from the receiver's value slot.
            self.borrow_access(
                occurrence
                    .expression()
                    .expect("receiver occurrence has an expression"),
                source,
                kind,
                self.expression_type(expression).ty(),
            )?
        } else {
            source
        };

        self.builder_mut()
            .plan_occurrence_access(
                occurrence,
                occurrence
                    .expression()
                    .expect("receiver occurrence has an expression"),
                purpose,
                source,
            )
            .unwrap_or_else(|error| panic!("receiver access must match its invocation: {error:?}"));

        Ok(source)
    }

    pub(super) fn plan_call_reborrow(
        &mut self,
        expression: BoundExpressionId,
        kind: BorrowKind,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let access = self.plan_expression(expression, Some(StorageAccessPurpose::Read))?;
        let access = self.borrowed_value_access(expression, access)?;

        // A newly produced borrow already establishes this occurrence's loan.
        if self.has_fresh_borrow(expression, access) {
            return Ok(());
        }

        let data = self
            .request
            .semantic_values()
            .type_data(self.expression_type(expression).ty());

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            panic!("selected call reborrow must have a borrow argument type");
        };

        let source = self.access_with_reached_type(expression, access, *target)?;

        let borrowed = self.borrow_access(
            expression,
            source,
            kind,
            self.expression_type(expression).ty(),
        )?;

        self.record_purpose(
            expression,
            Some(StorageAccessPurpose::Borrow(kind)),
            borrowed,
        )?;

        self.expression_accesses.insert(expression, borrowed);

        Ok(())
    }
}
