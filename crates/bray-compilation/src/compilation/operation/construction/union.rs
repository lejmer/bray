use bray_binder::BinderFactContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget,
    ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget,
};
use bray_checker::{ConstructionInputSurface, OperationCandidate, OperationCandidateState};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, MemberLookupResult, NamedTypeSymbolId, RuntimeDefaultPresence, SymbolFactRequest,
    TypeData, TypeId, UnionPayloadFieldTypeFact,
};

use super::super::super::Compilation;
use super::super::super::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

impl Compilation {
    pub(super) fn leading_dot_variant_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        result_type: TypeId,
        selector: Option<&BoundMemberSelector>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let Some(BoundMemberSelector::Name(name)) = selector else {
            return Ok(None);
        };

        let data = facts
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) = facts
            .symbols()
            .lookup_member((*union).into(), name.as_str())
        else {
            return Ok(None);
        };

        self.union_variant_candidate(facts, variant, result_type, *substitution, diagnostics)
    }

    pub(super) fn union_variant_construction_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        callee: BoundExpressionId,
        result_type: TypeId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let variant = match unit.view().expression(callee) {
            Some(BoundExpression::LeadingDotVariant(variant)) => {
                return self.leading_dot_variant_candidate(
                    facts,
                    result_type,
                    variant.selector(),
                    diagnostics,
                );
            }
            Some(BoundExpression::Name(name)) => {
                let BoundReferenceTarget::Surface(AnySymbolId::UnionVariant(variant)) =
                    name.target()
                else {
                    return Ok(None);
                };

                variant
            }
            _ => return Ok(None),
        };

        let data = facts
            .semantic_values()
            .type_data(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let record = facts
            .symbols()
            .union_variant(variant)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if record.union() != *union {
            return Ok(None);
        }

        self.union_variant_candidate(facts, variant, result_type, *substitution, diagnostics)
    }

    fn union_variant_candidate(
        &self,
        facts: &CompilationBinderFacts<'_>,
        variant: bray_symbols::UnionVariantSymbolId,
        result_type: TypeId,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationCandidate>, FactQueryError> {
        let record = facts
            .symbols()
            .union_variant(variant)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let inputs = record
            .payload_fields()
            .iter()
            .copied()
            .map(|field| self.union_payload_input(facts, field, substitution, diagnostics))
            .collect::<Result<Option<Vec<_>>, _>>()?;

        let Some(inputs) = inputs else {
            return Ok(None);
        };

        let is_recovered = inputs.iter().any(|(_, is_recovered)| *is_recovered);
        let inputs = inputs.into_iter().map(|(input, _)| input);

        let key = facts
            .symbol_key(variant.into())
            .map_err(binder_fact_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // The candidate owns the shared key returned by the immutable symbol table.
        Ok(Some(OperationCandidate::symbol_construction(
            key.clone(),
            ConstructionTarget::UnionVariant(variant),
            result_type,
            inputs,
            if is_recovered {
                OperationCandidateState::Recovered
            } else {
                OperationCandidateState::Available
            },
        )))
    }

    fn union_payload_input(
        &self,
        facts: &CompilationBinderFacts<'_>,
        field: bray_symbols::UnionPayloadFieldSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<(ConstructionInputSurface, bool)>, FactQueryError> {
        let record = facts
            .symbols()
            .union_payload_field(field)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(name) = facts.symbols().member_name(field.into()).cloned() else {
            return Ok(None);
        };

        let Some(ty) = self.resolve_member_type(
            facts,
            SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(field),
            substitution,
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let presence = record.default_presence();

        let default = record
            .default_provider()
            .map(ConstructionDefaultProvider::UnionPayload);

        Ok(Some((
            ConstructionInputSurface::new(
                ConstructionInputId::UnionPayloadField(field),
                name,
                record.position(),
                ty,
                default,
                record.ordinal(),
            ),
            presence == RuntimeDefaultPresence::Recovered,
        )))
    }
}
