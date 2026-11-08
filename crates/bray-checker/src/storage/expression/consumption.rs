use bray_bound_tree::{
    BoundExecutionSite, BoundExpressionId, StorageAccessId, StorageAccessPurpose,
    StorageAccessRoot, StorageIdentity, StoragePath, StorageValueConsumptionPhase,
};

use super::super::plan::Planner;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(in crate::storage) fn value_consumer_phase(&self, expression: BoundExpressionId) -> StorageValueConsumptionPhase {
        if self.selections.expression(expression).is_some_and(|selection| matches!(selection,
            bray_bound_tree::SemanticSelection::Call(call) if call.stages_inputs()))
            || self.selections.invoked_callable(expression.into()).is_some()
        {
            StorageValueConsumptionPhase::Admission
        } else {
            StorageValueConsumptionPhase::Completion
        }
    }

    pub(in crate::storage) fn consume_expression_value(
        &mut self,
        occurrence: BoundExecutionSite,
        phase: StorageValueConsumptionPhase,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
    ) {
        let value = self.builder().value_slot(expression).filter(|root| {
            !(matches!(purpose, StorageAccessPurpose::Borrow(_) | StorageAccessPurpose::Read)
            && !matches!(self.builder().identity(*root), Some(StorageIdentity::Evaluated(_)))
            && !matches!(self.request.semantic_values().type_data(self.expression_type(expression).ty()).as_ref(),
                bray_symbols::TypeData::Borrow { .. }))
        });

        // Place projections keep their produced loan descriptor until the actual
        // consumer has captured the reached data or formed a new borrow.
        let projected = self.projected_borrow_value(expression).filter(|root| Some(*root) != value);

        for root in value.into_iter().chain(projected) {
            self.builder_mut().consume_value(occurrence, phase, StoragePath::root(root))
                .expect("evaluated operand consumption names its registered output slot");
        }
    }

    fn projected_borrow_value(&self, expression: BoundExpressionId) -> Option<bray_bound_tree::StorageIdentityId> {
        let access = self.builder().access(*self.expression_accesses.get(&expression)?)?;
        let StorageAccessRoot::Storage(root) = access.root() else { return None; };
        let producer = self.builder().identity(root)?.definition_occurrence()?.expression()?;

        (producer != expression
            && self.builder().value_slot(producer) == Some(root)
            && matches!(self.request.semantic_values().type_data(self.expression_type(producer).ty()).as_ref(),
                bray_symbols::TypeData::Borrow { .. }))
            .then_some(root)
    }

    pub(in crate::storage) fn pattern_value_path(
        &self,
        expression: BoundExpressionId,
        access: StorageAccessId,
    ) -> Option<StoragePath> {
        let access = self.builder().access(access).expect("pattern retains its checked source access");

        if let StorageAccessRoot::Storage(root) = access.root() {
            match self.builder().identity(root) {
                Some(StorageIdentity::ScopedCapability { .. }) => {
                    // Matching may copy capability data. The lifecycle owner
                    // remains live until its selected scoped exit admits it.
                    return None;
                }
                Some(StorageIdentity::InvocationResult(_)) => {
                    return Some(StoragePath { root, projections: access.projections().into() });
                }
                _ => {}
            }
        }

        self.builder().value_slot(expression).map(StoragePath::root)
    }
}
