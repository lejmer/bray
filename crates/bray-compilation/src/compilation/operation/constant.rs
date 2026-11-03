use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpressionId, BoundUnit, MemberTarget, SelectedImplementationWitness, SelectedOperation,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantDeclaredTypeQuery, ConstantInstanceKey,
    GenericSubstitutionId, SymbolQueryRequest, TraitConstantFulfillmentDeclaredTypeQuery,
    TraitConstantMemberDeclaredTypeQuery,
};

use super::model::SemanticResolution;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::constant::constant_definition_id;
use crate::compilation::substitution::substitution_for_owner;
use crate::fact::FactQueryError;

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "trait constant selection retains source, subject, application, member, and diagnostics"
    )]
    pub(super) fn resolve_trait_constant_member_operation(
        &self,
        context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        expression: BoundExpressionId,
        receiver: bray_symbols::TypeId,
        application: bray_symbols::TraitApplicationId,
        member: bray_symbols::TraitConstantMemberSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<SemanticResolution>, FactQueryError> {
        let values = context.semantic_values();
        let application_data = values.trait_application_data(application);

        if !bray_binder::check_member_access(
            context,
            unit.view(),
            expression.into(),
            application_data.definition().into(),
            member.into(),
            diagnostics,
        )
        .map_err(crate::compilation::binder::binding_query_error)?
        {
            return Ok(None);
        }

        let subject = bray_checker::normalize_type_valued_members(
            &crate::compilation::checker::CompilationCheckerContext::new(*context),
            receiver,
            diagnostics,
        )
        .map_err(FactQueryError::from)?;

        let requirement = bray_symbols::ImplementationRequirementKey::new(subject, application);

        let owner = context
            .symbols()
            .symbol_for_key(unit.key().declared_owner())
            .expect("trait member unit retains its declaration owner");

        let constraints =
            super::constraint::enclosing_generic_constraints(context, owner, diagnostics)?;

        let requirements = Self::trait_constraint_requirements(&constraints, subject);

        if let Some(dispatch) = requirements.get(&application) {
            let Some(target) = self.resolve_constant_member(
                context,
                member.into(),
                application_data.substitution(),
                diagnostics,
            )?
            else {
                return Ok(None);
            };

            let ty = values
                .substitute_contextual_self(
                    target.result_type(),
                    bray_symbols::SelfTypeContext::Trait(application_data.definition()),
                    subject,
                )
                .map_err(FactQueryError::SemanticValueStore)?;

            let term = values
                .intern_constant_term(bray_symbols::ConstantTermData::TraitConstant {
                    requirement,
                    member,
                })
                .map_err(FactQueryError::SemanticValueStore)?;

            let target = MemberTarget::new(member.into(), ty, [])
                .with_constant_reference(term)
                .with_trait_dispatch(*dispatch);

            return Ok(Some(SemanticResolution::new(
                expression,
                ty,
                [],
                Some(SelectedOperation::Member(target)),
            )));
        }

        let Some(instance) =
            self.selected_trait_constant_instance(context, requirement, member, None, diagnostics)?
        else {
            if !diagnostics.has_errors() {
                diagnostics.add(super::query::member_selection_diagnostic(
                    unit,
                    expression,
                    bray_diagnostics::DiagnosticKind::CheckingNoApplicableCandidate,
                ));
            }

            return Ok(None);
        };

        let witness = instance
            .selected_implementation()
            .expect("selected trait constant retains its implementation");

        let implementation = values.implementation_instance_data(witness);

        let Some(target) = self.resolve_constant_member(
            context,
            instance.definition().into_any(),
            instance.substitution(),
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        let self_context = match instance.definition() {
            AnyConstantDefinitionId::TraitMember(_) => {
                bray_symbols::SelfTypeContext::Trait(application_data.definition())
            }
            _ => bray_symbols::SelfTypeContext::Implementation(implementation.definition()),
        };

        let ty = values
            .substitute_contextual_self(target.result_type(), self_context, subject)
            .map_err(FactQueryError::SemanticValueStore)?;

        let ty = bray_checker::normalize_type_valued_members(
            &crate::compilation::checker::CompilationCheckerContext::new(*context),
            ty,
            diagnostics,
        )
        .map_err(FactQueryError::from)?;

        let term = values
            .intern_constant_term(bray_symbols::ConstantTermData::from(instance))
            .map_err(FactQueryError::SemanticValueStore)?;

        let target = MemberTarget::new(
            instance.definition().into_any(),
            ty,
            [SelectedImplementationWitness::new(requirement, witness)],
        )
        .with_constant_reference(term);

        Ok(Some(SemanticResolution::new(
            expression,
            ty,
            [],
            Some(SelectedOperation::Member(target)),
        )))
    }

    pub(super) fn resolve_constant_member(
        &self,
        context: &CompilationBindingContext<'_>,
        member: AnySymbolId,
        inherited: GenericSubstitutionId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<MemberTarget>, FactQueryError> {
        let definition = constant_definition_id(member)
            .expect("constant member resolution receives a constant definition");

        let substitution = substitution_for_owner(context.semantic_values(), member, [inherited])?;

        let ty = match definition {
            AnyConstantDefinitionId::Constant(member) => self.resolve_member_type(
                context,
                SymbolQueryRequest::<ConstantDeclaredTypeQuery>::new(member),
                substitution,
                diagnostics,
            )?,
            AnyConstantDefinitionId::TraitMember(member) => self.resolve_member_type(
                context,
                SymbolQueryRequest::<TraitConstantMemberDeclaredTypeQuery>::new(member),
                substitution,
                diagnostics,
            )?,
            AnyConstantDefinitionId::TraitFulfillment(member) => self.resolve_member_type(
                context,
                SymbolQueryRequest::<TraitConstantFulfillmentDeclaredTypeQuery>::new(member),
                substitution,
                diagnostics,
            )?,
        };

        let Some(ty) = ty else {
            return Ok(None);
        };

        let term = context
            .semantic_values()
            .intern_constant_term(bray_symbols::ConstantTermData::from(
                ConstantInstanceKey::new(definition, substitution, None),
            ))
            .map_err(FactQueryError::SemanticValueStore)?;

        Ok(Some(
            MemberTarget::new(member, ty, []).with_constant_reference(term),
        ))
    }
}
