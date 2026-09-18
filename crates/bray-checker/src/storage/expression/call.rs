use bray_bound_tree::{BoundExpressionId, StorageAccessPurpose};
use bray_symbols::{BorrowKind, TypeData};

use super::super::plan::{PlanError, Planner};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn plan_call_reborrow(
        &mut self,
        expression: BoundExpressionId,
        kind: BorrowKind,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let access = self.plan_expression(expression, Some(StorageAccessPurpose::Read))?;
        let access = self.borrowed_value_access(expression, access)?;

        let record = self.builder()?
            .access(access)
            .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

        // A newly produced borrow already establishes this occurrence's loan.
        if let Some(capability) = record.root().borrow_capability()
            && self.builder()?
                .borrow_capability(capability)
                .is_some_and(|borrow| borrow.expression() == Some(expression))
        {
            return Ok(());
        }

        let data = self.request.semantic_values()
            .type_data(self.expression_type(expression)?.ty());

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            panic!("selected call reborrow must have a borrow argument type");
        };

        let source = self.access_with_reached_type(expression, access, *target)?;
        let borrowed = self.borrow_access(expression, source, kind)?;
        self.record_purpose(expression, Some(StorageAccessPurpose::Borrow(kind)), borrowed)?;
        self.expression_accesses.insert(expression, borrowed);

        Ok(())
    }
}
