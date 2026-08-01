use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundStructuredExpressionKind,
    IndexTarget, MemberTarget, SelectedImplementationWitness, SelectedOperation,
};
use bray_checker::{resolve_callable_signature_template, resolve_type_expression_template};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableSignatureFact, ExactSymbolId,
    ImplementationSelection, MemberLookupResult, NamedTypeSymbolId, StructFieldTypeFact,
    SymbolFactContract, SymbolFactRequest, TraitCallableMemberSymbolId, TypeData,
    TypeExpressionTemplate, TypeId,
};
use bray_syntax::TraitApplicationSyntax;

use super::super::Compilation;
use super::super::binder::{CompilationBinderFacts, binder_fact_error, type_binder};
use super::super::implementation::{implementation_fulfillments, selected_callable};
use super::super::substitution::{contextual_self_type, substitution_for_owner};
use crate::fact::{CancellationToken, FactQueryError, OperationSelectionFactKey};

use super::model::{OperationResolution, TraitOperation, TraitOperationCandidate};
use super::query::expression_type;

impl Compilation {
    pub(super) fn resolve_member_operation(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        if let Some(BoundExpression::TraitQualifiedMember(member)) =
            unit.view().expression(expression)
        {
            return self.resolve_trait_qualified_member_operation(
                facts,
                unit,
                types,
                expression,
                member,
                diagnostics,
            );
        }

        let (receiver, selector) = match unit.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => (member.receiver(), member.selector()),
            _ => return Err(FactQueryError::InfrastructureFailure),
        };

        let receiver_type = access_subject_type(facts, expression_type(types, receiver)?)?;

        let data = facts
            .semantic_values()
            .type_data(receiver_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if let (TypeData::Tuple(elements), Some(BoundMemberSelector::TupleElement(index))) =
            (data.as_ref(), selector)
        {
            let Some(result_type) = elements.get(*index as usize).copied() else {
                return Ok(None);
            };

            return Ok(Some(OperationResolution::new(
                expression,
                result_type,
                [],
                None,
            )));
        }

        let (
            TypeData::Named {
                definition,
                substitution,
            },
            Some(BoundMemberSelector::Name(name)),
        ) = (data.as_ref(), selector)
        else {
            return Ok(None);
        };

        let owner = match definition {
            NamedTypeSymbolId::Struct(structure) => AnySymbolId::from(*structure),
            NamedTypeSymbolId::Union(union) => AnySymbolId::from(*union),
        };

        let MemberLookupResult::Found(member) = facts.symbols().lookup_member(owner, name.as_str())
        else {
            return Ok(None);
        };

        let (result_type, operation) = match member {
            AnySymbolId::StructField(field) => {
                let result_type = self.resolve_member_type(
                    facts,
                    SymbolFactRequest::<StructFieldTypeFact>::new(field),
                    *substitution,
                    diagnostics,
                )?;

                let Some(result_type) = result_type else {
                    return Ok(None);
                };

                let operation =
                    SelectedOperation::Member(MemberTarget::new(field.into(), result_type, []));

                (result_type, operation)
            }
            member if CallableDefinitionId::try_new(member).is_some() => {
                let Some(signature) = self.resolve_callable_member_signature(
                    facts,
                    member,
                    *substitution,
                    diagnostics,
                )?
                else {
                    return Ok(None);
                };

                let result_type = signature.callable_type();
                let mut target = MemberTarget::new(member, result_type, []);

                if let Some(receiver) = signature.receiver() {
                    target = target.with_receiver(bray_symbols::ReceiverParameterSignature::new(
                        receiver.parameter(),
                        receiver_type,
                        receiver.mode(),
                    ));
                }

                let operation = SelectedOperation::Member(target);

                (result_type, operation)
            }
            _ => return Ok(None),
        };

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            Some(operation),
        )))
    }

    fn resolve_trait_qualified_member_operation(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
        member: &bray_bound_tree::BoundTraitQualifiedMemberExpression,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundMemberSelector::Name(name)) = member.selector() else {
            return Ok(None);
        };

        let receiver_type = expression_type(types, member.receiver())?;

        let owner = facts
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let syntax = member
            .trait_syntax()
            .find_descendant::<TraitApplicationSyntax>(facts.syntax())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let bound = type_binder(facts, owner)
            .map_err(binder_fact_error)?
            .bind_trait_application(&syntax)
            .map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(bound.diagnostics());

        let application = type_binder(facts, owner)
            .map_err(binder_fact_error)?
            .resolve_trait_application_template(bound.value())
            .map_err(binder_fact_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let application_data = facts
            .semantic_values()
            .trait_application_data(application)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let MemberLookupResult::Found(trait_member) = facts
            .symbols()
            .lookup_member(application_data.definition().into(), name.as_str())
        else {
            return Ok(None);
        };

        let Some(trait_member) = TraitCallableMemberSymbolId::try_from_any(trait_member) else {
            return Ok(None);
        };

        let requirement =
            bray_symbols::ImplementationRequirementKey::new(receiver_type, application);

        let selected = self.implementation_selection_result_with_cancellation(
            requirement,
            facts.cancellation(),
        )?;

        *diagnostics = diagnostics.merged(selected.diagnostics());

        let ImplementationSelection::Selected(witness) = selected.value() else {
            return Ok(None);
        };

        let instance = facts
            .semantic_values()
            .implementation_instance_data(*witness)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fulfillments = implementation_fulfillments(facts, instance.definition())?;

        let Some(fulfillment) = selected_callable(facts, fulfillments.callables, trait_member)
        else {
            return Ok(None);
        };

        let signature = self.resolve_callable_signature(
            facts,
            fulfillment.into(),
            [application_data.substitution(), instance.substitution()],
            diagnostics,
        )?;

        let Some(signature) = signature else {
            return Ok(None);
        };

        let result_type = signature.callable_type();

        let mut target = MemberTarget::new(
            fulfillment.into(),
            result_type,
            [SelectedImplementationWitness::new(requirement, *witness)],
        );

        if let Some(receiver) = signature.receiver() {
            target = target.with_receiver(bray_symbols::ReceiverParameterSignature::new(
                receiver.parameter(),
                receiver_type,
                receiver.mode(),
            ));
        }

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            Some(SelectedOperation::Member(target)),
        )))
    }

    fn resolve_callable_member_signature(
        &self,
        facts: &CompilationBinderFacts<'_>,
        member: AnySymbolId,
        receiver_substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_symbols::CallableSignature>, FactQueryError> {
        self.resolve_callable_signature(
            facts,
            member,
            [receiver_substitution],
            diagnostics,
        )
    }

    fn resolve_callable_signature(
        &self,
        facts: &CompilationBinderFacts<'_>,
        member: AnySymbolId,
        substitutions: impl IntoIterator<Item = bray_symbols::GenericSubstitutionId>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_symbols::CallableSignature>, FactQueryError> {
        let callable = CallableDefinitionId::try_new(member)
            .ok_or(FactQueryError::InfrastructureFailure)?
            .callable_symbol();

        let result = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
            .map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());

        let checked = self.checked_constant_terms_for_templates_with_cancellation(
            [result.value().callable_type(), result.value().result()],
            facts.cancellation(),
        )?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let substitution =
            substitution_for_owner(facts.semantic_values(), member, substitutions)?;

        resolve_callable_signature_template(
            facts.semantic_values(),
            result.value(),
            substitution,
            checked.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)
    }

    pub(super) fn resolve_member_type<'facts, F>(
        &self,
        facts: &CompilationBinderFacts<'facts>,
        request: SymbolFactRequest<F>,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TypeId>, FactQueryError>
    where
        F: SymbolFactContract<Value = TypeExpressionTemplate>,
        CompilationBinderFacts<'facts>: SymbolFactProvider<F>,
    {
        let result = facts.symbol_fact(request).map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());

        let checked = self.checked_constant_terms(result.value())?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let Some(ty) = resolve_type_expression_template(
            facts.semantic_values(),
            result.value(),
            checked.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        else {
            return Ok(None);
        };

        facts
            .semantic_values()
            .substitute_type(ty, substitution)
            .map(Some)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    pub(super) fn resolve_index_operation(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Structured(index)) = unit.view().expression(expression) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let Some(receiver) = index.operands().first().copied() else {
            return Ok(None);
        };

        let receiver_type = access_subject_type(facts, expression_type(types, receiver)?)?;

        let data = facts
            .semantic_values()
            .type_data(receiver_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let built_in = match (index.kind(), data.as_ref()) {
            (BoundStructuredExpressionKind::ElementIndex, TypeData::Array { element, .. }) => {
                Some((IndexTarget::ArrayElement, *element))
            }
            (BoundStructuredExpressionKind::ElementIndex, TypeData::Slice(element)) => {
                Some((IndexTarget::SliceElement, *element))
            }
            (BoundStructuredExpressionKind::SliceIndex, TypeData::Array { element, .. }) => Some((
                IndexTarget::ArraySlice,
                facts
                    .semantic_values()
                    .intern_type(TypeData::Slice(*element))
                    .map_err(|_| FactQueryError::InfrastructureFailure)?,
            )),
            (BoundStructuredExpressionKind::SliceIndex, TypeData::Slice(element)) => Some((
                IndexTarget::Slice,
                facts
                    .semantic_values()
                    .intern_type(TypeData::Slice(*element))
                    .map_err(|_| FactQueryError::InfrastructureFailure)?,
            )),
            _ => None,
        };

        if let Some((target, result_type)) = built_in {
            let usize_type = self
                .available_compiler_known_symbols()
                .representation_symbol::<bray_symbols::StructSymbolId>(
                    bray_compiler_known::RepresentationRole::ScalarUsize,
                )
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let substitution = super::super::substitution::empty_substitution(
                facts.semantic_values(),
                usize_type.into(),
            )?;

            let usize_type = facts
                .semantic_values()
                .intern_type(TypeData::Named {
                    definition: NamedTypeSymbolId::Struct(usize_type),
                    substitution,
                })
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let expectations = index
                .operands()
                .iter()
                .copied()
                .skip(1)
                .map(|operand| (operand, usize_type));

            let operation = SelectedOperation::Index {
                target,
                result_type,
            };

            return Ok(Some(OperationResolution::new(
                expression,
                result_type,
                expectations,
                Some(operation),
            )));
        }

        Ok(None)
    }

    pub(super) fn resolve_custom_index_operation(
        &self,
        key: &OperationSelectionFactKey,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Structured(index)) = unit.view().expression(key.expression())
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let operand_types = index
            .operands()
            .iter()
            .copied()
            .map(|operand| expression_type(types, operand))
            .collect::<Result<Vec<_>, _>>()?;

        let Some((subject, selectors)) = operand_types
            .split_first()
            .map(|(subject, selectors)| (*subject, selectors))
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let (role, trait_arguments, callable_parameters, operand_types) = match index.kind() {
            BoundStructuredExpressionKind::ElementIndex => (
                bray_compiler_known::CompilerKnownOperationRole::ElementIndex,
                selectors.to_vec(),
                selectors.to_vec(),
                selectors.to_vec(),
            ),
            BoundStructuredExpressionKind::SliceIndex => {
                let Some(bound) = selectors.first().copied() else {
                    return Ok(None);
                };

                let nullable_bound = facts
                    .semantic_values()
                    .intern_type(TypeData::Nullable(bound))
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                (
                    bray_compiler_known::CompilerKnownOperationRole::SliceIndex,
                    vec![bound],
                    vec![nullable_bound, nullable_bound],
                    vec![bound; selectors.len()],
                )
            }
            _ => return Err(FactQueryError::InfrastructureFailure),
        };

        let candidate = self
            .trait_operation_candidate_data(
                facts,
                role,
                subject,
                &trait_arguments,
                &callable_parameters,
                &operand_types,
                TraitOperation::Index,
                cancellation,
                diagnostics,
            )?
            .map(TraitOperationCandidate::into_candidate);

        self.select_operation(
            key,
            facts,
            unit,
            types,
            index.operands().iter().copied(),
            candidate,
            cancellation,
            diagnostics,
        )
    }
}

fn access_subject_type(
    facts: &CompilationBinderFacts<'_>,
    mut ty: TypeId,
) -> Result<TypeId, FactQueryError> {
    loop {
        let data = facts
            .semantic_values()
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::Borrow { target, .. } => ty = *target,
            TypeData::ContextualSelf(context) => {
                ty = contextual_self_type(facts, *context)?;
            }
            _ => return Ok(ty),
        }
    }
}
