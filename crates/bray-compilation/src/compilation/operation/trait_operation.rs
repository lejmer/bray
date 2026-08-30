use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundExpression, ConversionTarget, IndexTarget, OperatorTarget, SelectedCompoundAssignment,
    SelectedConversion, SelectedOperation,
};
use bray_checker::{CompilerKnownOperationEvidence, ImplementationSelectionEvidence};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableParameterData, CallableParameterSignature, CallableSignature,
    CallableSignatureQuery, CallableTypeData, CheckedConstraint, CheckedConstraintKind,
    GenericArgument, GenericParameterSymbolId, ImplementationSelection, NamedTypeSymbolId,
    ReceiverMode, ReceiverParameterSignature, SymbolQueryRequest, TraitConstraintDispatch,
    TypeData, TypeExpressionTemplate, TypeId,
};

use super::super::Compilation;
use super::super::binder::{CompilationBindingContext, binding_query_error};
use super::super::implementation::{
    TypeValuedMemberResolution, callable_instance, implementation_callable_instance,
    implementation_fulfillments, implementation_requirement, selected_type_valued_member,
};
use crate::fact::{CancellationToken, FactQueryError, OperationSelectionQueryKey};

use super::model::{OperationResolution, TraitOperation, TraitOperationCandidate};
use super::query::expression_type;

impl Compilation {
    pub(super) fn resolve_operator_operation(
        &self,
        key: &OperationSelectionQueryKey,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let expression = unit
            .view()
            .expression(key.expression())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (operator, operands) = match expression {
            BoundExpression::Unary(expression) => (expression.operator(), expression.operands()),
            BoundExpression::Binary(expression) => (expression.operator(), expression.operands()),
            BoundExpression::Assignment(expression) => {
                let Some(operator) = expression.operator().binary_operator() else {
                    return Ok(None);
                };

                (operator, expression.operands())
            }
            _ => return Ok(None),
        };

        let Some(role) = bray_checker::compiler_known_operation_role(expression, operator) else {
            return Ok(None);
        };

        let operand_types = operands
            .iter()
            .copied()
            .map(|operand| expression_type(types, operand))
            .collect::<Result<Vec<_>, _>>()?;

        let Some((subject, arguments)) = operand_types
            .split_first()
            .map(|(subject, arguments)| (*subject, arguments))
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let candidate = self
            .trait_operation_candidate_data(
                binding_context,
                binding_context
                    .symbols()
                    .symbol_for_key(unit.key().declared_owner())
                    .ok_or(FactQueryError::InfrastructureFailure)?,
                role,
                subject,
                arguments,
                arguments,
                arguments,
                TraitOperation::Operator(operator),
                cancellation,
                diagnostics,
            )?
            .map(TraitOperationCandidate::into_candidate);

        if candidate.is_none() {
            let context = self.checker_context_for(key.unit(), cancellation)?;
            let mut built_in = false;

            for operand in std::iter::once(subject).chain(arguments.iter().copied()) {
                built_in |= bray_checker::built_in_operator_supported(&context, operand, operator)
                    .map_err(FactQueryError::CheckerInfrastructure)?;
            }

            if built_in {
                return Ok(None);
            }
        }

        let resolution = self.select_operation(
            key,
            binding_context,
            unit,
            types,
            operands.iter().copied(),
            candidate,
            cancellation,
            diagnostics,
        )?;

        let Some(resolution) = resolution else {
            return Ok(None);
        };

        let BoundExpression::Assignment(_) = expression else {
            return Ok(Some(resolution));
        };

        let [destination, _] = operands else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let assignment_type = expression_type(types, key.expression())?;
        let operation_type = resolution.result_type();

        let Some(SelectedOperation::Operator { target, .. }) = resolution.selection() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let selection = SelectedOperation::CompoundAssignment(SelectedCompoundAssignment::new(
            *target,
            operation_type,
            assignment_type,
        ));

        Ok(Some(OperationResolution::new(
            key.expression(),
            assignment_type,
            [(*destination, operation_type)],
            Some(selection),
        )))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "compiler-known operation matching exposes the complete trait and callable shape"
    )]
    pub(super) fn trait_operation_candidate_data(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        owner: AnySymbolId,
        role: bray_compiler_known::CompilerKnownOperationRole,
        subject: TypeId,
        trait_arguments: &[TypeId],
        callable_parameters: &[TypeId],
        operand_types: &[TypeId],
        operation: TraitOperation,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TraitOperationCandidate>, FactQueryError> {
        let contract = self
            .available_compiler_known_symbols()
            .operation_contract(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let trait_symbol = binding_context
            .symbols()
            .trait_symbol(contract.trait_definition())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let parameters = trait_symbol
            .generic_type_parameters()
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type)
            .collect::<Vec<_>>();

        if parameters.len() != trait_arguments.len() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let requirement = implementation_requirement(
            binding_context.semantic_values(),
            subject,
            contract.trait_definition(),
            parameters,
            trait_arguments.iter().copied().map(GenericArgument::Type),
        )?;

        let Some(member) = contract.callable() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let constraints =
            super::constraint::enclosing_generic_constraints(binding_context, owner, diagnostics)?;

        if let Some((generic_owner, constraint)) = constraints.iter().find(|(_, constraint)| {
            matches!(
                constraint.kind(),
                CheckedConstraintKind::TraitSatisfaction {
                    subject: constrained_subject,
                    application,
                } if constrained_subject == subject
                    && application == requirement.trait_application()
            )
        }) {
            return self.constrained_trait_operation_candidate(
                binding_context,
                role,
                subject,
                callable_parameters,
                operand_types,
                operation,
                contract,
                member,
                requirement,
                &constraints,
                TraitConstraintDispatch::new(*generic_owner, constraint.ordinal()),
                diagnostics,
            );
        }

        let selected =
            self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

        *diagnostics = diagnostics.merged(selected.diagnostics());

        let ImplementationSelection::Selected(witness) = selected.value() else {
            return Ok(None);
        };

        let instance = binding_context
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fulfillments = implementation_fulfillments(binding_context, instance.definition())?;

        let operation_result_type = match operation {
            TraitOperation::Conversion(target) => Some(target),
            TraitOperation::Operator(_) | TraitOperation::Index(_) => self.operation_result_type(
                binding_context,
                contract,
                instance.substitution(),
                fulfillments.types,
                diagnostics,
            )?,
        };

        let Some(operation_result_type) = operation_result_type else {
            return Ok(None);
        };

        let result_type =
            self.operation_expression_result_type(binding_context, role, operation_result_type)?;

        let callable_result_type =
            self.operation_callable_result_type(binding_context, operation, operation_result_type)?;

        let trait_application = binding_context
            .semantic_values()
            .trait_application_data(requirement.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let member_instance = callable_instance(
            binding_context.semantic_values(),
            member.into(),
            [trait_application.substitution()],
        )?;

        let Some(fulfillment_instance) = implementation_callable_instance(
            binding_context,
            fulfillments.callables,
            member,
            trait_application.substitution(),
            instance.substitution(),
        )?
        else {
            return Ok(None);
        };

        let fulfillment_instance = fulfillment_instance.instance();

        let selected_operation = match operation {
            TraitOperation::Operator(operator) => SelectedOperation::Operator {
                target: OperatorTarget::Trait {
                    operator,
                    member: member_instance,
                    fulfillment: fulfillment_instance,
                    requirement,
                    witness: *witness,
                },
                result_type,
            },
            TraitOperation::Index(borrow_kind) => SelectedOperation::Index {
                target: IndexTarget::Custom {
                    borrow_kind,
                    member: member_instance,
                    fulfillment: fulfillment_instance,
                    requirement,
                    witness: *witness,
                },
                result_type,
            },
            TraitOperation::Conversion(target) => {
                SelectedOperation::Conversion(SelectedConversion::new(
                    subject,
                    target,
                    ConversionTarget::Trait {
                        member: member_instance,
                        fulfillment: fulfillment_instance,
                        requirement,
                        witness: *witness,
                    },
                ))
            }
        };

        let receiver_mode = match operation {
            TraitOperation::Conversion(_) => ReceiverMode::Consuming,
            TraitOperation::Operator(_) | TraitOperation::Index(BorrowKind::Shared) => {
                ReceiverMode::Shared
            }
            TraitOperation::Index(BorrowKind::Mutable) => ReceiverMode::Mutable,
        };

        let signature = self.operation_signature(
            binding_context,
            member,
            subject,
            callable_parameters,
            callable_result_type,
            receiver_mode,
            diagnostics,
        )?;

        let evidence =
            CompilerKnownOperationEvidence::new(role, requirement, member_instance, signature);

        let key = binding_context
            .symbol_key(instance.definition().into_any())
            .map_err(binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // The candidate and its implementation evidence own shared semantic identities.
        Ok(Some(TraitOperationCandidate {
            key: key.clone(),
            operation: selected_operation,
            operand_types: std::iter::once(subject)
                .chain(operand_types.iter().copied())
                .collect(),
            implementation_selection: Some(ImplementationSelectionEvidence::new(
                requirement,
                selected.value().clone(),
            )),
            compiler_known_operation: evidence,
        }))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "generic operation selection retains the exact static contract and callable shape"
    )]
    fn constrained_trait_operation_candidate(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        role: bray_compiler_known::CompilerKnownOperationRole,
        subject: TypeId,
        callable_parameters: &[TypeId],
        operand_types: &[TypeId],
        operation: TraitOperation,
        contract: bray_symbols::CompilerKnownOperationContract,
        member: bray_symbols::TraitCallableMemberSymbolId,
        requirement: bray_symbols::ImplementationRequirementKey,
        constraints: &[(bray_symbols::GenericOwnerId, CheckedConstraint)],
        dispatch: TraitConstraintDispatch,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TraitOperationCandidate>, FactQueryError> {
        let application = binding_context
            .semantic_values()
            .trait_application_data(requirement.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let member_instance = callable_instance(
            binding_context.semantic_values(),
            member.into(),
            [application.substitution()],
        )?;

        let operation_result_type = match operation {
            TraitOperation::Conversion(target) => target,
            TraitOperation::Operator(_) | TraitOperation::Index(_) => self
                .constrained_operation_result_type(
                    binding_context,
                    contract,
                    subject,
                    requirement.trait_application(),
                    constraints,
                )?
                .ok_or(FactQueryError::InfrastructureFailure)?,
        };

        let result_type =
            self.operation_expression_result_type(binding_context, role, operation_result_type)?;

        let callable_result_type =
            self.operation_callable_result_type(binding_context, operation, operation_result_type)?;

        let selected_operation = match operation {
            TraitOperation::Operator(operator) => SelectedOperation::Operator {
                target: OperatorTarget::TraitConstraint {
                    operator,
                    member: member_instance,
                    requirement,
                    dispatch,
                },
                result_type,
            },
            TraitOperation::Index(borrow_kind) => SelectedOperation::Index {
                target: IndexTarget::TraitConstraint {
                    borrow_kind,
                    member: member_instance,
                    requirement,
                    dispatch,
                },
                result_type,
            },
            TraitOperation::Conversion(target) => {
                SelectedOperation::Conversion(SelectedConversion::new(
                    subject,
                    target,
                    ConversionTarget::TraitConstraint {
                        member: member_instance,
                        requirement,
                        dispatch,
                    },
                ))
            }
        };

        let receiver_mode = match operation {
            TraitOperation::Conversion(_) => ReceiverMode::Consuming,
            TraitOperation::Operator(_) | TraitOperation::Index(BorrowKind::Shared) => {
                ReceiverMode::Shared
            }
            TraitOperation::Index(BorrowKind::Mutable) => ReceiverMode::Mutable,
        };

        let signature = self.operation_signature(
            binding_context,
            member,
            subject,
            callable_parameters,
            callable_result_type,
            receiver_mode,
            diagnostics,
        )?;

        let evidence =
            CompilerKnownOperationEvidence::new(role, requirement, member_instance, signature);

        let key = binding_context
            .symbol_key(member.into())
            .map_err(binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok(Some(TraitOperationCandidate {
            key: key.clone(),
            operation: selected_operation,
            operand_types: std::iter::once(subject)
                .chain(operand_types.iter().copied())
                .collect(),
            implementation_selection: None,
            compiler_known_operation: evidence,
        }))
    }

    fn constrained_operation_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        contract: bray_symbols::CompilerKnownOperationContract,
        subject: TypeId,
        application: bray_symbols::TraitApplicationId,
        constraints: &[(bray_symbols::GenericOwnerId, CheckedConstraint)],
    ) -> Result<Option<TypeId>, FactQueryError> {
        if let Some(member) = contract.result_type_member() {
            let projection = binding_context
                .semantic_values()
                .intern_type(TypeData::TypeValuedMemberProjection {
                    subject,
                    application,
                    member,
                })
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            return super::constraint::normalize_type_equalities(
                binding_context.semantic_values(),
                projection,
                constraints,
            )
            .map(Some);
        }

        if let Some(definition) = contract.fixed_callable_result_type() {
            return super::super::substitution::named_type(
                binding_context.semantic_values(),
                definition,
            )
            .map(Some);
        }

        if contract.role() == bray_compiler_known::CompilerKnownOperationRole::Equality {
            return self
                .representation_type(
                    binding_context,
                    bray_compiler_known::RepresentationRole::ScalarBool,
                )
                .map(Some);
        }

        Ok(None)
    }

    fn operation_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        contract: bray_symbols::CompilerKnownOperationContract,
        substitution: bray_symbols::GenericSubstitutionId,
        fulfillments: &[bray_symbols::TraitTypeFulfillmentSymbolId],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TypeId>, FactQueryError> {
        if let Some(member) = contract.result_type_member() {
            return match selected_type_valued_member(
                binding_context,
                substitution,
                fulfillments,
                member,
                diagnostics,
            )? {
                TypeValuedMemberResolution::Resolved(ty) => Ok(Some(ty)),
                TypeValuedMemberResolution::Invalid | TypeValuedMemberResolution::Deferred => {
                    Ok(None)
                }
            };
        }

        if let Some(definition) = contract.fixed_callable_result_type() {
            return super::super::substitution::named_type(
                binding_context.semantic_values(),
                definition,
            )
            .map(Some);
        }

        if contract.role() == bray_compiler_known::CompilerKnownOperationRole::Equality {
            return self
                .representation_type(
                    binding_context,
                    bray_compiler_known::RepresentationRole::ScalarBool,
                )
                .map(Some);
        }

        Ok(None)
    }

    fn representation_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        role: bray_compiler_known::RepresentationRole,
    ) -> Result<TypeId, FactQueryError> {
        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        super::super::substitution::named_type(
            binding_context.semantic_values(),
            NamedTypeSymbolId::Struct(definition),
        )
    }

    fn operation_expression_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        role: bray_compiler_known::CompilerKnownOperationRole,
        callable_result: TypeId,
    ) -> Result<TypeId, FactQueryError> {
        if role == bray_compiler_known::CompilerKnownOperationRole::Comparison {
            return self.representation_type(
                binding_context,
                bray_compiler_known::RepresentationRole::ScalarBool,
            );
        }

        Ok(callable_result)
    }

    fn operation_callable_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        operation: TraitOperation,
        result: TypeId,
    ) -> Result<TypeId, FactQueryError> {
        let TraitOperation::Index(kind) = operation else {
            return Ok(result);
        };

        binding_context
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind,
                target: result,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation signatures require each selected receiver and callable component explicitly"
    )]
    fn operation_signature(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        member: bray_symbols::TraitCallableMemberSymbolId,
        receiver: TypeId,
        parameters: &[TypeId],
        result: TypeId,
        receiver_mode: ReceiverMode,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<CallableSignature, FactQueryError> {
        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                member.into(),
            ))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(template.diagnostics());

        let receiver_parameter = template
            .value()
            .receiver()
            .ok_or(FactQueryError::InfrastructureFailure)?
            .parameter();

        if template.value().parameters().len() != parameters.len() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let callable_type = operation_callable_type(
            binding_context.semantic_values(),
            template.value().callable_type(),
            parameters,
            result,
        )?;

        let parameters = template
            .value()
            .parameters()
            .iter()
            .copied()
            .zip(parameters.iter().copied())
            .map(|(parameter, ty)| CallableParameterSignature::new(parameter, ty));

        Ok(CallableSignature::new(
            callable_type,
            Some(ReceiverParameterSignature::new(
                receiver_parameter,
                receiver,
                receiver_mode,
            )),
            parameters,
            result,
        ))
    }
}

fn operation_callable_type(
    values: &bray_symbols::SemanticValueStore,
    template: &TypeExpressionTemplate,
    parameters: &[TypeId],
    result: TypeId,
) -> Result<TypeId, FactQueryError> {
    let (parameter_surface, constness, trust, abi, dependencies, phase_behaviors) = match template {
        TypeExpressionTemplate::Callable(callable) => {
            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    (
                        parameter.name().clone(),
                        parameter.position(),
                        parameter.mode(),
                    )
                })
                .collect::<Vec<_>>();

            (
                parameters,
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependencies(),
                callable.phase_behaviors().clone(),
            )
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    (
                        parameter.name().clone(),
                        parameter.position(),
                        parameter.mode(),
                    )
                })
                .collect::<Vec<_>>();

            (
                parameters,
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependency_contracts(),
                callable.phase_behaviors().clone(),
            )
        }
        _ => return Err(FactQueryError::InfrastructureFailure),
    };

    if parameter_surface.len() != parameters.len() {
        return Err(FactQueryError::InfrastructureFailure);
    }

    let parameters = parameter_surface
        .into_iter()
        .zip(parameters.iter().copied())
        .map(|((name, position, mode), ty)| CallableParameterData::new(name, position, mode, ty));

    let callable = CallableTypeData::new(parameters, result, constness, trust, abi, dependencies)
        .with_phase_behaviors(phase_behaviors);

    values
        .intern_type(TypeData::Callable(callable))
        .map_err(|_| FactQueryError::InfrastructureFailure)
}
