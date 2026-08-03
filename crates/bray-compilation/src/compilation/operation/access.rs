use std::collections::BTreeMap;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundStructuredExpressionKind,
    IndexTarget, MemberTarget, SelectedImplementationWitness, SelectedOperation,
};
use bray_checker::{resolve_callable_signature_template, resolve_type_expression_template};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, CallableSignature,
    CallableSignatureFact, CheckedConstraintKind, ExactSymbolId, GenericConstraintsFact,
    GenericOwnerId, ImplementationSelection, ImplementationSubjectFact, MemberLookupResult,
    NamedTypeSymbolId, SelfTypeContext, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest,
    TraitApplicationId, TraitCallableMemberSymbolId, TraitConstraintDispatch, TypeData,
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

struct ResolvedCallableMember {
    signature: CallableSignature,
    instance: CallableInstanceData,
}

fn member_callable_signature(
    signature: CallableSignature,
    receiver_type: TypeId,
) -> CallableSignature {
    let receiver = signature.receiver().map(|receiver| {
        bray_symbols::ReceiverParameterSignature::new(
            receiver.parameter(),
            receiver_type,
            receiver.mode(),
        )
    });

    CallableSignature::new(
        signature.callable_type(),
        receiver,
        signature.parameters().iter().copied(),
        signature.result(),
    )
}

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

        let receiver_type = self.resolve_access_subject_type(
            facts,
            expression_type(types, receiver)?,
            diagnostics,
        )?;

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

        if matches!(data.as_ref(), TypeData::TypeParameter(_))
            && let Some(BoundMemberSelector::Name(name)) = selector
        {
            return self.resolve_constrained_member_operation(
                facts,
                unit,
                expression,
                receiver_type,
                name.as_str(),
                diagnostics,
            );
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

                (result_type, Some(operation))
            }
            member if CallableDefinitionId::try_new(member).is_some() => {
                let Some(callable) = self.resolve_callable_member_signature(
                    facts,
                    member,
                    *substitution,
                    diagnostics,
                )?
                else {
                    return Ok(None);
                };

                let result_type = callable.signature.callable_type();
                let signature = member_callable_signature(callable.signature, receiver_type);

                let target = MemberTarget::new(member, result_type, [])
                    .with_callable(callable.instance, signature);

                let operation = SelectedOperation::Member(target);

                (result_type, Some(operation))
            }
            AnySymbolId::UnionVariant(_) => (receiver_type, None),
            _ => return Ok(None),
        };

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            operation,
        )))
    }

    fn resolve_constrained_member_operation(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        expression: BoundExpressionId,
        receiver_type: TypeId,
        name: &str,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let owner = facts
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let requirements =
            self.trait_constraint_requirements(facts, owner, receiver_type, diagnostics)?;

        let mut matches = Vec::new();

        for (application, dispatch) in requirements {
            let application_data = facts
                .semantic_values()
                .trait_application_data(application)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let MemberLookupResult::Found(member) = facts
                .symbols()
                .lookup_member(application_data.definition().into(), name)
            else {
                continue;
            };

            let Some(member) = TraitCallableMemberSymbolId::try_from_any(member) else {
                continue;
            };

            matches.push((dispatch, application_data, member));
        }

        let [(dispatch, application, member)] = matches.as_slice() else {
            return Ok(None);
        };

        let callable = self.resolve_callable_signature(
            facts,
            (*member).into(),
            [application.substitution()],
            diagnostics,
        )?;

        let Some(callable) = callable else {
            return Ok(None);
        };

        let result_type = callable.signature.callable_type();
        let signature = member_callable_signature(callable.signature, receiver_type);

        let target = MemberTarget::new((*member).into(), result_type, [])
            .with_callable(callable.instance, signature)
            .with_trait_dispatch(*dispatch);

        Ok(Some(OperationResolution::new(
            expression,
            result_type,
            [],
            Some(SelectedOperation::Member(target)),
        )))
    }

    fn trait_constraint_requirements(
        &self,
        facts: &CompilationBinderFacts<'_>,
        mut owner: AnySymbolId,
        receiver_type: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<BTreeMap<TraitApplicationId, TraitConstraintDispatch>, FactQueryError> {
        let mut requirements = BTreeMap::new();

        loop {
            if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
                let constraints = facts
                    .symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(
                        generic_owner,
                    ))
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(constraints.diagnostics());

                for constraint in constraints.value().constraints() {
                    let CheckedConstraintKind::TraitSatisfaction {
                        subject,
                        application,
                    } = constraint.kind()
                    else {
                        continue;
                    };

                    if subject == receiver_type {
                        requirements.entry(application).or_insert_with(|| {
                            TraitConstraintDispatch::new(generic_owner, constraint.ordinal())
                        });
                    }
                }
            }

            let Some(container) = facts.symbols().containing_symbol(owner) else {
                break;
            };

            owner = container;
        }

        Ok(requirements)
    }

    fn resolve_access_subject_type(
        &self,
        facts: &CompilationBinderFacts<'_>,
        mut ty: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TypeId, FactQueryError> {
        loop {
            let data = facts
                .semantic_values()
                .type_data(ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match data.as_ref() {
                TypeData::Borrow { target, .. } => ty = *target,
                TypeData::ContextualSelf(SelfTypeContext::Implementation(implementation)) => {
                    ty =
                        self.resolve_implementation_self_type(facts, *implementation, diagnostics)?;
                }
                TypeData::ContextualSelf(context) => {
                    ty = contextual_self_type(facts, *context)?;
                }
                _ => return Ok(ty),
            }
        }
    }

    fn resolve_implementation_self_type(
        &self,
        facts: &CompilationBinderFacts<'_>,
        implementation: bray_symbols::ImplementationSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TypeId, FactQueryError> {
        let subject = facts
            .symbol_fact(SymbolFactRequest::<ImplementationSubjectFact>::new(
                implementation,
            ))
            .map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(subject.diagnostics());

        let checked = self.checked_constant_terms(subject.value().ty())?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        resolve_type_expression_template(
            facts.semantic_values(),
            subject.value().ty(),
            checked.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)
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

        let selected = self
            .implementation_selection_result_with_cancellation(requirement, facts.cancellation())?;

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

        let callable = self.resolve_callable_signature(
            facts,
            fulfillment.into(),
            [application_data.substitution(), instance.substitution()],
            diagnostics,
        )?;

        let Some(callable) = callable else {
            return Ok(None);
        };

        let result_type = callable.signature.callable_type();
        let signature = member_callable_signature(callable.signature, receiver_type);

        let target = MemberTarget::new(
            fulfillment.into(),
            result_type,
            [SelectedImplementationWitness::new(requirement, *witness)],
        )
        .with_callable(callable.instance, signature);

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
    ) -> Result<Option<ResolvedCallableMember>, FactQueryError> {
        self.resolve_callable_signature(facts, member, [receiver_substitution], diagnostics)
    }

    fn resolve_callable_signature(
        &self,
        facts: &CompilationBinderFacts<'_>,
        member: AnySymbolId,
        substitutions: impl IntoIterator<Item = bray_symbols::GenericSubstitutionId>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<ResolvedCallableMember>, FactQueryError> {
        let definition =
            CallableDefinitionId::try_new(member).ok_or(FactQueryError::InfrastructureFailure)?;

        let callable = definition.callable_symbol();

        let result = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
            .map_err(binder_fact_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());

        let checked = self.checked_constant_terms_for_templates_with_cancellation(
            [result.value().callable_type(), result.value().result()],
            facts.cancellation(),
        )?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let substitution = substitution_for_owner(facts.semantic_values(), member, substitutions)?;

        let signature = resolve_callable_signature_template(
            facts.semantic_values(),
            result.value(),
            substitution,
            checked.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?;

        Ok(signature.map(|signature| ResolvedCallableMember {
            signature,
            instance: CallableInstanceData::new(definition, substitution),
        }))
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
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Structured(index)) = unit.view().expression(expression) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let Some(receiver) = index.operands().first().copied() else {
            return Ok(None);
        };

        let receiver_type = self.resolve_access_subject_type(
            facts,
            expression_type(types, receiver)?,
            diagnostics,
        )?;

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
