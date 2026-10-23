use bray_bound_tree::{
    BoundCallDependencyContracts, BoundDependencyContract, BoundDependencyRequirement,
    BoundDependencyRequirementKind, BoundDependencySubject, BoundExpressionId,
    DependencyContractInstantiationError, SemanticSelection, StorageAccessId, StoragePlan,
};
use bray_symbols::{DependencyContractTemplateData, DependencySubjectRoot, SymbolOrdinal};

use super::instantiation::{CallInstantiationContext, expression_access};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

/// Instantiates each input evaluation against the preceding staged inputs.
pub(crate) fn selected_input_contracts<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    expression: BoundExpressionId,
    selection: &SemanticSelection,
    values: &crate::dependency::ValueInputs,
) -> Result<
    Vec<(
        bray_bound_tree::BoundExecutionSite,
        BoundCallDependencyContracts,
    )>,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
> {
    if !selection.evaluates_defaults() {
        return Ok(Vec::new());
    }

    let count = selection.inputs().count();
    let mut slots = vec![None; count];

    for input in selection.inputs() {
        let ordinal = selection
            .input_ordinal(input.root())
            .expect("selected input has its ordinal");

        let index = usize::try_from(ordinal.raw()).expect("input ordinal fits host");

        slots[index] = Some((input.root(), input_slot(storage, expression, ordinal), None));
    }

    let slots = slots
        .into_iter()
        .map(|input| input.expect("normalized invocation has every declared input"))
        .collect::<Vec<Input>>();

    let mut result = Vec::with_capacity(count);

    for input in selection.inputs() {
        let ordinal = selection
            .input_ordinal(input.root())
            .expect("selected input has its ordinal");

        let index = usize::try_from(ordinal.raw()).expect("input ordinal fits host");

        let template = selected_input_template(request, selection, ordinal)
            .map_err(DependencyContractInstantiationError::Resolution)?;

        let site = bray_bound_tree::BoundExecutionSite::CallInput {
            expression,
            ordinal,
        };

        let template = if values.is_independent(site) {
            template.with_requirements([])
        } else {
            template
        };

        let contract = if let Some(source) = input.expression() {
            let access = expression_access(storage, source)
                .expect("evaluated input retains its source access");

            let inputs = [(DependencySubjectRoot::Receiver, access, Some(source))];

            input_contract(request, storage, &inputs, slots[index].1, &template)?
        } else {
            let inputs = if matches!(
                input.default_provider(),
                Some(bray_bound_tree::DefaultValueProvider::CallableParameter(_))
            ) {
                &slots[..index]
            } else {
                &[]
            };

            input_contract(request, storage, inputs, slots[index].1, &template)?
        };

        result.push((
            bray_bound_tree::BoundExecutionSite::CallInput {
                expression,
                ordinal,
            },
            contract,
        ));
    }

    Ok(result)
}

type Input = (
    DependencySubjectRoot,
    StorageAccessId,
    Option<BoundExpressionId>,
);

fn input_slot(
    storage: &StoragePlan,
    expression: BoundExpressionId,
    ordinal: SymbolOrdinal,
) -> StorageAccessId {
    storage
        .invocation_input(expression, ordinal)
        .expect("selected call input has its exact staged slot")
}

fn input_contract<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    inputs: &[Input],
    result: StorageAccessId,
    template: &DependencyContractTemplateData,
) -> Result<
    BoundCallDependencyContracts,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
> {
    let mut context =
        CallInstantiationContext::hidden_inputs(request, storage, inputs, Some(result));

    let concrete = DependencyContractTemplateData::new(super::contract::concrete_requirements(
        template.requirements(),
    ));

    let retained = BoundDependencyContract::try_instantiate(&concrete, &mut context)?;
    let reports = super::contract::instantiate_reports(template, &mut context)?;

    let invocation = BoundDependencyContract::new(inputs.iter().map(|(_, access, _)| {
        BoundDependencyRequirement::Direct {
            subject: BoundDependencySubject::StorageAccess(*access),
            kind: BoundDependencyRequirementKind::StorageAlive,
        }
    }));

    let contracts = BoundCallDependencyContracts::new(invocation, retained, None)
        .with_input_count(inputs.len())
        .with_effects(template.effects().iter().cloned())
        .with_carriers(template.carriers().iter().cloned())
        .with_returned_input(template.returned_input())
        .with_normal_completion(template.normal_completion())
        .with_normal_storage_preservation(template.preserves_storage_on_normal_return())
        .with_reports(reports);

    super::references::with_reference_inputs(request, contracts, template, None, |subject| {
        let access = if matches!(
            subject.subject_root(),
            DependencySubjectRoot::Result | DependencySubjectRoot::EvaluationStorage
        ) {
            Some(result)
        } else {
            inputs
                .iter()
                .find_map(|(root, access, _)| (*root == subject.subject_root()).then_some(*access))
        };

        let Some(access) = access else {
            return Ok(None);
        };

        let ty = storage
            .access(access)
            .expect("input contract has its checked access")
            .reached_type();

        super::references::reference_subject_path(request, subject, ty)
    })
    .map_err(DependencyContractInstantiationError::Resolution)
}

pub(crate) fn selected_input_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
    ordinal: SymbolOrdinal,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let input = selection
        .input(ordinal)
        .expect("selected invocation retains its exact input");

    if let Some(conversion) = input.conversion() {
        return super::operation::conversion_dependencies(request, conversion);
    }

    if let Some(provider) = input.default_provider() {
        let mut template = request.default_result_dependencies(provider)?;

        if selection.input_instance().is_none()
            && let Some(ty) = selection.result_type()
            && let bray_symbols::TypeData::Named { substitution, .. } =
                request.semantic_values().type_data(ty).as_ref()
        {
            template = request
                .semantic_values()
                .substitute_dependency_contract(template, *substitution)
                .expect("field default retains its selected type substitution");
        }

        return crate::dependency::resolved_result_template(
            request,
            selection.input_instance(),
            template,
        );
    }

    let ty = input.ty(request.semantic_values());

    super::operation::conversion_dependencies(
        request,
        &bray_bound_tree::SelectedConversion::new(
            ty,
            ty,
            bray_bound_tree::ConversionTarget::Identity,
        ),
    )
}
