use bray_bound_tree::{
    BoundExecutionSite, BoundExpression, BoundExpressionId, SelectedArgument, SelectedInput,
    SemanticSelection, StorageAccessPurpose, StorageIdentity,
};

use super::super::plan::{PlanError, Planner};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> Planner<'_, C> {
    pub(super) fn plan_default_inputs(
        &mut self,
        expression: BoundExpressionId,
        selection: &SemanticSelection,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        if let SemanticSelection::Call(call) = selection
            && !matches!(
                call.target(),
                bray_bound_tree::BoundCallableTarget::Declaration(_)
                    | bray_bound_tree::BoundCallableTarget::TraitView { .. }
            )
            && let Some(BoundExpression::Call(source)) = self.request.view().expression(expression)
        {
            self.plan_expression(source.callee(), Some(StorageAccessPurpose::Read))?;
        }

        for input in selection.inputs() {
            let ordinal = selection
                .input_ordinal(input.root())
                .expect("selected input has its ordinal");

            let invocation = BoundExecutionSite::CallInput {
                expression,
                ordinal,
            };

            if let Some(source) = input.expression() {
                match input {
                    SelectedInput::Receiver(receiver) => {
                        let purpose = Self::call_receiver_purpose(receiver.mode());

                        let occurrence = if matches!(purpose, StorageAccessPurpose::Borrow(_)) {
                            source.into()
                        } else {
                            invocation
                        };

                        self.plan_invocation_input(occurrence, source, purpose)?;
                    }
                    SelectedInput::Argument(SelectedArgument::Explicit {
                        reborrow: Some(kind),
                        ..
                    }) => {
                        self.plan_call_reborrow(source, *kind)?;
                    }
                    _ => {
                        self.plan_invocation_input(
                            invocation,
                            source,
                            StorageAccessPurpose::ValueTransfer,
                        )?;
                    }
                }
            }

            self.plan_input_storage(
                expression,
                ordinal,
                input.ty(self.request.semantic_values()),
            )?;
        }

        Ok(())
    }

    fn plan_input_storage(
        &mut self,
        expression: BoundExpressionId,
        ordinal: bray_symbols::SymbolOrdinal,
        ty: bray_symbols::TypeId,
    ) -> Result<(), PlanError<C::UpstreamError>> {
        let binding = bray_bound_tree::StorageBindingTarget::InvocationInput {
            expression,
            ordinal,
        };

        let identity = self
            .bind_identity(
                binding,
                StorageIdentity::InvocationInput {
                    expression,
                    ordinal,
                },
                Some(ty),
            )?
            .expect("each selected input is planned once");

        let source = self
            .request
            .view()
            .expression(expression)
            .expect("selected input has its call source")
            .origin()
            .source_anchor();

        let access = self
            .builder_mut()
            .push_access(bray_bound_tree::StorageAccess::new(
                bray_bound_tree::StorageAccessRoot::Storage(identity),
                [],
                ty,
                source,
                false,
            ))
            .expect("planned input access belongs to its call unit");

        self.builder_mut()
            .plan_occurrence_access(
                bray_bound_tree::BoundExecutionSite::CallInput {
                    expression,
                    ordinal,
                },
                expression,
                StorageAccessPurpose::Initialize,
                access,
            )
            .expect("planned input initialization has its exact invocation occurrence");

        self.builder_mut()
            .plan_occurrence_access(
                expression.into(),
                expression,
                StorageAccessPurpose::ValueTransfer,
                access,
            )
            .expect("selected callee consumes its exact staged input");

        Ok(())
    }
}
