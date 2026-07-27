use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundStructuredExpressionKind,
    IndexTarget, MemberTarget, SelectedOperation,
};
use bray_checker::{resolve_callable_signature_template, resolve_type_expression_template};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableSignatureFact, MemberLookupResult,
    NamedTypeSymbolId, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest, TypeData,
    TypeExpressionTemplate, TypeId,
};

use super::super::Compilation;
use super::super::binder::{CompilationBinderFacts, binder_fact_error};
use super::super::substitution::substitution_for_owner;
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
        let (receiver, selector) = match unit.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => (member.receiver(), member.selector()),
            Some(BoundExpression::TraitQualifiedMember(member)) => {
                (member.receiver(), member.selector())
            }
            _ => return Err(FactQueryError::InfrastructureFailure),
        };

        let receiver_type = expression_type(types, receiver)?;

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
                let Some(result_type) =
                    self.resolve_callable_member_type(facts, member, *substitution, diagnostics)?
                else {
                    return Ok(None);
                };

                let operation =
                    SelectedOperation::Member(MemberTarget::new(member, result_type, []));

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

    fn resolve_callable_member_type(
        &self,
        facts: &CompilationBinderFacts<'_>,
        member: AnySymbolId,
        receiver_substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TypeId>, FactQueryError> {
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
            substitution_for_owner(facts.semantic_values(), member, [receiver_substitution])?;

        resolve_callable_signature_template(
            facts.semantic_values(),
            result.value(),
            substitution,
            checked.value(),
        )
        .map(|signature| signature.map(|signature| signature.callable_type()))
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

        let receiver_type = expression_type(types, receiver)?;

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
