use super::super::instantiation::{CallInstantiationContext, expression_access};
use super::inputs::{OperationInput, operation_inputs};
use super::templates::operation_result_dependencies;
use crate::{CheckerRequestContext, CheckerUnitView};
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
) -> super::super::contract::ContractInstantiationResult<
    Option<(BoundCallDependencyContracts, Vec<DependencySubjectRoot>)>,
    C::UpstreamError,
> {
    if let SelectedOperation::TraitView {
        methods,
        defaults,
        requirement,
        ..
    } = operation
    {
        let mut cleanup = Vec::new();

        for method in methods.iter().flatten() {
            let template = super::templates::invocation_dependencies(
                request,
                method
                    .callable_instance()
                    .expect("view formation retains each checked implementation callable"),
                method.trait_dispatch(),
                [],
            )
            .map_err(bray_bound_tree::DependencyContractInstantiationError::Resolution)?;

            cleanup.extend_from_slice(template.synchronous_cleanup());
        }

        for (method, provider) in defaults.iter().flatten() {
            let instance = method
                .callable_instance()
                .expect("view default retains its declaration context");

            let application = request
                .semantic_values()
                .trait_application_data(requirement.trait_application());

            cleanup.push(bray_symbols::DependencyRequirement::DefaultCleanup {
                provider: bray_symbols::DefaultValueProvider::CallableParameter(*provider),
                substitution: instance.substitution(),
                contextual_self: Some((
                    bray_symbols::SelfTypeContext::Trait(application.definition()),
                    requirement.subject(),
                )),
            });
        }

        let contracts = BoundCallDependencyContracts::new(
            BoundDependencyContract::new([]),
            BoundDependencyContract::new([]),
            None,
        )
        .with_synchronous_cleanup(&cleanup);

        return Ok(Some((contracts, Vec::new())));
    }

    let Some(template) = operation_result_dependencies(request, expression, operation)
        .map_err(bray_bound_tree::DependencyContractInstantiationError::Resolution)?
    else {
        return Ok(None);
    };

    let operands = operation_inputs(request.view(), expression, operation);

    let inputs = operands
        .iter()
        .copied()
        .filter_map(|(root, input)| Some((root, input.access(storage)?, input.expression())))
        .collect::<Vec<_>>();

    let mut context = CallInstantiationContext::hidden_inputs(
        request,
        storage,
        &inputs,
        expression_access(storage, expression),
    );

    context.set_result_values(values);

    let concrete = DependencyContractTemplateData::new(
        super::super::contract::concrete_requirements(template.template.requirements()),
    );

    let result = BoundDependencyContract::try_instantiate(&concrete, &mut context)?;

    let reports = super::super::contract::instantiate_reports(&template.template, &mut context)?;

    let contracts =
        BoundCallDependencyContracts::new(BoundDependencyContract::new([]), result, None)
            .with_reports(reports)
            .with_input_count(inputs.len())
            .with_effects(template.template.effects().iter().cloned())
            .with_synchronous_cleanup(template.template.synchronous_cleanup())
            .with_carriers(template.template.carriers().iter().cloned())
            .with_returned_input(template.template.returned_input())
            .with_normal_completion(template.template.normal_completion())
            .with_normal_storage_preservation(
                template.template.preserves_storage_on_normal_return(),
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
