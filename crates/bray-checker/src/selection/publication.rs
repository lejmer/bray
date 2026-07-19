use bray_bound_tree::{
    CheckedExpressionTypes, CheckedSemanticSelections, SemanticSelection, SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, UnitCheckRequest};

use super::{CandidateSelection, SemanticSelectionInput, select_callable, select_operation};

pub(crate) fn check_semantic_selections<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: SemanticSelectionInput,
) -> CheckerOutcome<CheckedSemanticSelections>
where
    C: CheckerRequestContext + ?Sized,
{
    if types.unit() != request.view().unit() || types.kind() != request.view().kind() {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let (calls, operations) = input.into_parts();
    let mut entries = Vec::with_capacity(calls.len() + operations.len());
    let mut diagnostics = Vec::with_capacity(calls.len() + operations.len());

    for call in calls {
        let expression = call.expression();

        match select_callable(request, types, call) {
            CheckerOutcome::Complete(result) => {
                let (selection, owned_diagnostics) = result.into_parts();

                diagnostics.push(owned_diagnostics);

                if let CandidateSelection::Selected(call) = selection {
                    entries.push(SemanticSelectionEntry::new(
                        expression,
                        SemanticSelection::Call(call),
                    ));
                }
            }
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        }
    }

    for operation in operations {
        let expression = operation.expression();

        match select_operation(request, types, operation) {
            CheckerOutcome::Complete(result) => {
                let (selection, owned_diagnostics) = result.into_parts();

                diagnostics.push(owned_diagnostics);

                if let CandidateSelection::Selected(operation) = selection {
                    entries.push(SemanticSelectionEntry::new(
                        expression,
                        SemanticSelection::Operation(operation),
                    ));
                }
            }
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        }
    }

    let selections = match CheckedSemanticSelections::try_new(request.unit(), types, entries) {
        Ok(selections) => selections,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            );
        }
    };

    CheckerOutcome::complete(selections, DiagnosticBag::merged_all(&diagnostics))
}
