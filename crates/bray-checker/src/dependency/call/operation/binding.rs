use super::super::instantiation::{CallInstantiationContext, expression_access};
use super::inputs::{OperationInput, operation_inputs};
use super::templates::operation_result_dependencies;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundCallDependencyContracts, BoundDependencyContract, BoundExpressionId, SelectedOperation,
    StorageAccessPurpose, StoragePlan,
};
use bray_symbols::{DependencyContractTemplateData, DependencySubjectRoot};

pub(crate) fn selected_operation_contracts<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    expression: BoundExpressionId,
    operation: &SelectedOperation,
    values: &crate::dependency::ValueInputs,
) -> Result<
    Option<(BoundCallDependencyContracts, Vec<DependencySubjectRoot>)>,
    bray_bound_tree::DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
> {
    let Some(template) = operation_result_dependencies(request, expression, operation)
        .map_err(bray_bound_tree::DependencyContractInstantiationError::Resolution)?
    else {
        return Ok(None);
    };

    let inputs = operation_inputs(request.view(), expression, operation)
        .into_iter()
        .filter_map(|(root, input)| {
            let operand = input.expression()?;

            let subject = match input {
                OperationInput::Borrow(_, kind) => {
                    let plan = storage
                        .expression_plans(operand)
                        .find(|plan| plan.purpose() == StorageAccessPurpose::Borrow(kind))
                        .expect("selected invocation operand retains its implicit borrow");

                    plan.access()
                }
                OperationInput::Value(_) | OperationInput::Nullable(_) => {
                    expression_access(storage, operand)
                        .expect("selected invocation input retains its storage access")
                }
            };

            Some((root, subject, Some(operand)))
        })
        .collect::<Vec<_>>();

    let mut context = CallInstantiationContext::hidden_inputs(
        request,
        storage,
        &inputs,
        expression_access(storage, expression),
    );

    context.set_result_values(values);

    let concrete = DependencyContractTemplateData::new(
        super::super::contract::concrete_report_requirements(template.template.requirements()),
    );

    let result = BoundDependencyContract::try_instantiate(&concrete, &mut context)?;

    let reports = super::super::contract::instantiate_reports(
        &template.immediate_reports(false),
        &mut context,
    )?;

    Ok(Some((
        BoundCallDependencyContracts::new(BoundDependencyContract::new([]), result, None)
            .with_reports(reports)
            .with_input_count(inputs.len())
            .with_effects(
                template
                    .template
                    .effects()
                    .iter()
                    .chain(template.evaluation_effects.iter())
                    .cloned(),
            )
            .with_normal_completion(
                template.template.normal_completion() && template.evaluation_completes,
            ),
        template.escaping_evaluation_inputs,
    )))
}
