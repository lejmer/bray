use bray_bound_tree::{
    AnyBoundNodeId, CallableProofDependency, CallableProofTarget, SelectedOperation,
    SemanticSelection,
};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::flow::GuaranteeDomain;
use crate::CheckerInfrastructureError;
use crate::analysis::model::AnalysisOperationKind;

impl GuaranteeDomain<'_> {
    /// Construction admits local cleanup before publishing ownership, even if cleanup later skips work.
    pub(super) fn construction_admission_dependencies(
        &self,
        operation: AnalysisOperationKind,
    ) -> Result<Option<Vec<CallableProofDependency>>, CheckerInfrastructureError> {
        let expression = match operation {
            AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
            | AnalysisOperationKind::Call {
                expression,
                phase: crate::analysis::model::AnalysisCallPhase::Attempt,
            } => expression,
            _ => return Ok(Some(Vec::new())),
        };

        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.selections.expression(expression)
        else {
            return Ok(Some(Vec::new()));
        };

        let Some(ty) = construction.new_owner_type() else {
            return Ok(Some(Vec::new()));
        };

        let mut dependencies = Vec::new();

        for slot in [
            TypeAssociatedLifecycleSlot::Finalizer,
            TypeAssociatedLifecycleSlot::Destructor,
        ] {
            let Some(action) = self.input.lifecycle(ty, slot) else {
                continue;
            };

            let Some(required) = crate::cleanup_admission::cleanup_admission_requirements(
                self.values,
                self.available,
                action.conditions(),
                action.execution(),
                action.result(),
            )?
            else {
                return Ok(None);
            };

            dependencies.extend(required.into_iter().map(|obligation| {
                CallableProofDependency::new(
                    CallableProofTarget::Implicit {
                        site: expression.into(),
                        callable: action.callable(),
                    },
                    obligation,
                )
            }));
        }

        Ok(Some(dependencies))
    }
}
