use super::super::instantiation::{CallInstantiationContext, expression_access};
use super::inputs::{OperationInput, operation_inputs};
use super::templates::operation_result_dependencies;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundCallDependencyContracts, BoundDependencyContract, BoundExpressionId, SelectedOperation,
    StoragePlan,
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

    let operands = operation_inputs(request.view(), expression, operation);

    let inputs = operands
        .iter()
        .copied()
        .filter_map(|(root, input)| {
            let operand = input.expression()?;

            let subject = match input {
                OperationInput::Borrow(_, _) => input
                    .borrow_plan(storage)
                    .expect("borrow input has a plan")
                    .access(),
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

    let contracts =
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
            );

    let contracts = super::super::references::with_reference_inputs(
        request,
        contracts,
        &template.template,
        None,
        |subject| {
            let (access, input) = if subject.subject_root() == DependencySubjectRoot::Result {
                (expression_access(storage, expression), None)
            } else {
                (
                    inputs
                        .iter()
                        .find(|(root, _, _)| *root == subject.subject_root())
                        .map(|(_, access, _)| *access),
                    operands
                        .iter()
                        .find(|(root, _)| *root == subject.subject_root())
                        .map(|(_, input)| *input),
                )
            };

            let Some(access) = access else {
                return Ok(None);
            };

            let mut ty = storage
                .access(access)
                .expect("operation input retains its checked access")
                .reached_type();

            let mut projections = subject.projections();

            match input {
                Some(input @ OperationInput::Borrow(_, kind)) => {
                    let capability = storage
                        .borrow_for_plan(
                            input.borrow_plan(storage).expect("borrow input has a plan"),
                        )
                        .expect("checked borrow plan retains its capability");

                    let referent = storage
                        .borrow_capability(capability)
                        .expect("checked borrow retains its record")
                        .access();

                    ty = storage
                        .access(referent)
                        .expect("checked loan retains its referent access")
                        .reached_type();

                    ty = request
                        .semantic_values()
                        .intern_type(bray_symbols::TypeData::Borrow { kind, target: ty })
                        .expect("checked operation borrow belongs to the semantic store");
                }
                Some(OperationInput::Nullable(_)) => {
                    let Some(payload) = projections
                        .strip_prefix(&[bray_symbols::DependencyProjection::NullableValue])
                    else {
                        // The synthesized nullable slot has no caller-visible storage.
                        return Ok(None);
                    };

                    projections = payload;
                }
                Some(OperationInput::Value(_)) | None => {}
            }

            let Some(projections) =
                crate::storage::resolve_storage_projections(request, projections)?
            else {
                return Ok(None);
            };

            crate::storage::reference_path(request, ty, projections.into(), None)
        },
    )
    .map_err(bray_bound_tree::DependencyContractInstantiationError::Resolution)?;

    Ok(Some((contracts, template.escaping_evaluation_inputs)))
}
