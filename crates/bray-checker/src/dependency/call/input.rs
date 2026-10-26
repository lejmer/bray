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
) -> super::contract::ContractInstantiationResult<
    Vec<(bray_bound_tree::BoundExecutionSite, BoundCallDependencyContracts)>,
    C::UpstreamError,
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
                Some(bray_symbols::DefaultValueProvider::CallableParameter(_))
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
        .with_synchronous_cleanup(template.synchronous_cleanup())
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
        let store = request.semantic_values();
        let template = request.default_result_dependencies(provider)?;
        let data = store.dependency_contract_template_data(template);
        let cleanup = !data.synchronous_cleanup().is_empty();

        let template = if cleanup {
            store
                .intern_dependency_contract_template(
                    data.as_ref().clone().with_synchronous_cleanup([]),
                )
                .expect("selected default cleanup is retained by its provider edge")
        } else {
            template
        };

        let substitution = selection
            .input_instance()
            .map(|instance| instance.substitution())
            .or_else(|| {
                selection
                    .result_type()
                    .and_then(|ty| match store.type_data(ty).as_ref() {
                        bray_symbols::TypeData::Named { substitution, .. } => Some(*substitution),
                        _ => None,
                    })
            })
            .expect("checked runtime default input retains its enclosing declaration substitution");

        let contextual_self = default_contextual_self(request, selection, provider, substitution)?;

        let template = store
            .substitute_dependency_contract(template, substitution)
            .expect("selected default retains its enclosing generic substitution");

        let template = match contextual_self {
            Some((context, subject)) => store
                .substitute_contextual_self_in_dependency_contract(template, context, subject)
                .expect("selected default retains its lexical Self context"),
            None => template,
        };

        let template = crate::dependency::resolved_result_template(request, None, template, None)?;

        // Formation certifies every selected provider before its concrete Self is erased.
        let erased = matches!(selection, SemanticSelection::Call(call) if matches!(call.target(), bray_bound_tree::BoundCallableTarget::TraitView { .. }));

        return Ok(if cleanup && !erased {
            template.with_synchronous_cleanup([
                bray_symbols::DependencyRequirement::DefaultCleanup {
                    provider,
                    substitution,
                    contextual_self,
                },
            ])
        } else {
            template
        });
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

fn default_contextual_self<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
    provider: bray_symbols::DefaultValueProvider,
    substitution: bray_symbols::GenericSubstitutionId,
) -> Result<
    Option<(bray_symbols::SelfTypeContext, bray_symbols::TypeId)>,
    CheckerQueryError<C::UpstreamError>,
> {
    let Some(context) = request
        .symbols()
        .enclosing_self_type_context(provider.symbol())
    else {
        return Ok(None);
    };

    let values = request.semantic_values();

    let subject = match selection {
        SemanticSelection::Call(call) => {
            match (call.resolution().trait_dispatch(), call.receiver()) {
                (Some(dispatch), _) => request.context().dispatch_requirement(dispatch)?.subject(),
                (None, Some(receiver)) => values.unborrowed_type(receiver.target_type()),
                (None, None) => {
                    let subject = match context {
                        bray_symbols::SelfTypeContext::NamedType(definition) => values
                            .intern_open_named_type(request.symbols(), definition)
                            .expect("selected default's named Self type is well formed")
                            .expect("selected default's named Self declaration exists"),
                        bray_symbols::SelfTypeContext::Implementation(implementation) => {
                            let subject = request
                                .context()
                                .implementation_subject_type(implementation)?;

                            assert!(
                                !subject.diagnostics().has_errors(),
                                "selected default belongs to valid implementation {implementation:?}"
                            );

                            *subject.value()
                        }
                        bray_symbols::SelfTypeContext::Trait(_) => {
                            panic!(
                                "trait default {provider:?} retains neither dispatch nor receiver"
                            )
                        }
                    };

                    values
                        .substitute_type(subject, substitution)
                        .expect("selected default Self retains its enclosing generic arguments")
                }
            }
        }
        SemanticSelection::Operation(bray_bound_tree::SelectedOperation::Construction(
            construction,
        )) => match construction.target() {
            bray_bound_tree::ConstructionTarget::TypeForm { requirement, .. } => {
                requirement.subject()
            }
            _ => construction.result_type(),
        },
        _ => panic!("a selected default input belongs to a call or construction"),
    };

    Ok(Some((context, subject)))
}
