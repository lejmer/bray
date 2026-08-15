use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    BoundBlockItem, BoundExpressionId, BoundSourceAnchor, BoundUnit, BoundUnitKey, BoundUnitRoot,
};
use bray_checker::{
    CheckedConstantTerms, CheckerFactError, CheckerRequestContext, CheckerUnitView,
    ConstantEvaluationInput, ConstantEvaluator, DefaultConstantEvaluator, closed_type_is_copyable,
    evaluate_generic_constraint_template, resolve_trait_application_template,
    resolve_type_expression_template, type_is_copyable_in_context,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    ConstantValueKind, GenericConstraintObligationKey, GenericConstraintSatisfactionQuery,
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationCandidate,
    ImplementationRequirementKey, ImplementationSelection, ProofOutcome, SemanticFactResult,
    SymbolFactRequest, TraitApplicationTemplate, TraitSymbolId, TypeData, TypeExpressionTemplate,
    TypeId,
};

use super::Compilation;
use super::checker::{CompilationCheckerContext, checker_result};
use super::substitution::generic_parameter_argument;
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns whether every static constraint holds for one generic declaration instance.
    pub fn generic_constraint_satisfaction(
        &self,
        key: GenericConstraintObligationKey,
    ) -> Result<Arc<SemanticFactResult<GenericConstraintSatisfactionQuery>>, FactQueryError> {
        self.generic_constraint_satisfaction_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn generic_constraint_satisfaction_with_cancellation(
        &self,
        key: GenericConstraintObligationKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SemanticFactResult<GenericConstraintSatisfactionQuery>>, FactQueryError> {
        let cell = self.state.generic_constraint_satisfaction.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::GenericConstraintSatisfaction(key),
            cancellation,
            || {
                self.compute_generic_constraint_satisfaction(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_generic_constraint_satisfaction(
        &self,
        key: GenericConstraintObligationKey,
        cancellation: &CancellationToken,
    ) -> Result<SemanticFactResult<GenericConstraintSatisfactionQuery>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(key.substitution())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if substitution.owner() != key.owner() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let binding_context = self.binding_context(cancellation)?;

        let template = binding_context
            .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateQuery>::new(
                key.owner(),
            ))
            .map_err(super::binder::binder_fact_error)?;

        let mut outcome = ProofOutcome::Proven;
        let mut diagnostics = template.diagnostics().clone();

        for constraint in template.value().constraints() {
            cancellation.check()?;

            let result = self.evaluate_constraint(
                key.owner(),
                constraint,
                key.substitution(),
                cancellation,
            )?;

            diagnostics = diagnostics.merged(result.diagnostics());
            outcome = outcome.and(*result.value());

            if outcome == ProofOutcome::Disproven {
                break;
            }
        }

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    pub(in crate::compilation) fn implementation_candidate_constraint_outcome(
        &self,
        candidate: &ImplementationCandidate,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(candidate.substitution())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let obligation =
            GenericConstraintObligationKey::new(substitution.owner(), candidate.substitution());

        match self.generic_constraint_satisfaction_with_cancellation(obligation, cancellation) {
            Ok(result) => {
                diagnostics.add_range(result.diagnostics().clone());

                Ok(*result.value())
            }
            Err(FactQueryError::Cycle(_)) => Ok(ProofOutcome::Unknown),
            Err(error) => Err(error),
        }
    }

    pub(in crate::compilation) fn generic_declaration_may_be_satisfied(
        &self,
        template: &GenericDeclarationTemplate,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        if template.constraints().is_empty() {
            return Ok(true);
        }

        let values = self.semantic_value_store()?;
        let mut arguments = Vec::with_capacity(template.parameters().len());

        for parameter in template.parameters() {
            arguments.push(generic_parameter_argument(values, *parameter)?);
        }

        let substitution = GenericSubstitutionData::try_new(
            template.owner(),
            template.parameters().iter().copied(),
            arguments,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let substitution = values
            .intern_generic_substitution(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let obligation = GenericConstraintObligationKey::new(template.owner(), substitution);

        let outcome =
            self.generic_constraint_satisfaction_with_cancellation(obligation, cancellation)?;

        Ok(*outcome.value() != ProofOutcome::Disproven)
    }

    fn evaluate_constraint(
        &self,
        owner: bray_symbols::GenericOwnerId,
        constraint: &GenericConstraintTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        match constraint {
            GenericConstraintTemplate::Resolved(constraint) => match constraint.kind() {
                bray_symbols::CheckedConstraintKind::Predicate(_) => self
                    .evaluate_imported_predicate_constraint(
                        owner,
                        constraint.ordinal(),
                        substitution,
                        cancellation,
                    ),
                bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    let values = self.semantic_value_store()?;

                    let subject = values
                        .substitute_type(subject, substitution)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let application = values
                        .substitute_trait_application(application, substitution)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    self.evaluate_resolved_trait_satisfaction(
                        None,
                        subject,
                        application,
                        substitution,
                        cancellation,
                        DiagnosticBag::new(),
                    )
                }
                bray_symbols::CheckedConstraintKind::TypeEquality { left, right } => {
                    let values = self.semantic_value_store()?;

                    let left = values
                        .substitute_type(left, substitution)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let right = values
                        .substitute_type(right, substitution)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let left = self.normalize_type_valued_member(left, cancellation)?;
                    let right = self.normalize_type_valued_member(right, cancellation)?;

                    let diagnostics =
                        DiagnosticBag::merged_all([left.diagnostics(), right.diagnostics()]);

                    Ok(DiagnosticResult::new(
                        if left.value() == right.value() {
                            ProofOutcome::Proven
                        } else {
                            ProofOutcome::Disproven
                        },
                        diagnostics,
                    ))
                }
            },
            GenericConstraintTemplate::TraitSatisfaction {
                unit,
                subject,
                application,
                ..
            } => self.evaluate_trait_satisfaction_constraint(
                *unit,
                subject,
                application,
                substitution,
                cancellation,
            ),
            GenericConstraintTemplate::TypeEquality { left, right, .. } => {
                self.evaluate_type_equality_constraint(left, right, substitution, cancellation)
            }
            GenericConstraintTemplate::Source {
                unit, expression, ..
            } => self.evaluate_source_predicate_constraint(
                *unit,
                *expression,
                substitution,
                cancellation,
            ),
        }
    }

    fn evaluate_type_equality_constraint(
        &self,
        left: &TypeExpressionTemplate,
        right: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let mut terms = BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for occurrence in left
            .constant_expressions()
            .into_iter()
            .chain(right.constant_expressions())
        {
            let result = self.embedded_constant_term_with_cancellation(occurrence, cancellation)?;

            diagnostics = diagnostics.merged(result.diagnostics());
            terms.insert(occurrence.key(), *result.value());
        }

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let constants = CheckedConstantTerms::try_from_terms(terms)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let values = self.semantic_value_store()?;

        let left = resolve_type_expression_template(values, left, &constants)
            .map_err(FactQueryError::CheckerInfrastructure)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let right = resolve_type_expression_template(values, right, &constants)
            .map_err(FactQueryError::CheckerInfrastructure)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let left = values
            .substitute_type(left, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let right = values
            .substitute_type(right, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let left = self.normalize_type_valued_member(left, cancellation)?;
        let right = self.normalize_type_valued_member(right, cancellation)?;

        diagnostics = diagnostics.merged(left.diagnostics());
        diagnostics = diagnostics.merged(right.diagnostics());

        Ok(DiagnosticResult::new(
            if left.value() == right.value() {
                ProofOutcome::Proven
            } else {
                ProofOutcome::Disproven
            },
            diagnostics,
        ))
    }

    fn normalize_type_valued_member(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<TypeId>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } = data.as_ref()
        else {
            return Ok(DiagnosticResult::without_diagnostics(ty));
        };

        let context = CompilationCheckerContext::new(self.binding_context(cancellation)?);

        if let Some(result) =
            bray_checker::built_in_operation_result_type(&context, *subject, *application, *member)
                .map_err(FactQueryError::CheckerInfrastructure)?
        {
            return Ok(DiagnosticResult::without_diagnostics(result));
        }

        let result = context
            .selected_type_valued_member(*subject, *application, *member)
            .map_err(checker_dependency_error)?;

        let (resolved, diagnostics) = result.into_parts();

        Ok(DiagnosticResult::new(resolved.unwrap_or(ty), diagnostics))
    }

    fn evaluate_source_predicate_constraint(
        &self,
        unit: bray_declarations::SyntaxAnchor,
        expression: bray_symbols::DeclarationExpressionTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let key = self.constraint_unit_key(expression.owner(), unit)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

        let diagnostics = DiagnosticBag::merged_all([
            bound.result().diagnostics(),
            semantics.result().diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let root = constraint_expression(bound.result().value(), expression.syntax())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let references = self.concrete_call_references(
            bound.result().value(),
            &semantics.result().value().1,
            substitution,
            None,
            &BTreeMap::new(),
            bray_checker::ConstantEvaluationLimits::default(),
            cancellation,
        )?;

        let (references, reference_diagnostics) = references;

        let context = CompilationCheckerContext::new(
            self.binding_context_for(bound.result().value().key(), cancellation)?,
        );

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let request = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let resolver = super::constant::CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(
            &semantics.result().value().0,
            &semantics.result().value().1,
        )
        .with_root(root)
        .with_references(references)
        .with_call_resolver(&resolver);

        let evaluated =
            checker_result(DefaultConstantEvaluator.evaluate_constant(request, &input))?;

        let diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            &reference_diagnostics,
            evaluated.diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let outcome = constant_predicate_outcome(self.semantic_value_store()?, *evaluated.value())?;

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    fn evaluate_imported_predicate_constraint(
        &self,
        owner: bray_symbols::GenericOwnerId,
        ordinal: bray_symbols::SymbolOrdinal,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let Ok(substitution) = values.require_concrete_substitution(substitution) else {
            return Ok(DiagnosticResult::without_diagnostics(ProofOutcome::Unknown));
        };

        let binding_context = self.binding_context(cancellation)?;

        let Some(address) = binding_context
            .imported_semantic_address(owner.symbol())
            .map_err(super::binder::binder_fact_error)?
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let template_result = super::binder::imported_declaration_template_at(
            &binding_context,
            address,
            bray_bound_tree::CheckedTemplateKind::GenericConstraint,
            ordinal,
        )
        .map_err(super::binder::binder_fact_error)?;

        let Some(template) = template_result.value() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let imported = imported
            .value()
            .as_deref()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let context = CompilationCheckerContext::new(binding_context);

        let resolver =
            super::constant::CompilationConstantTemplateResolver::new(self, cancellation, imported);

        let result_type = template
            .template()
            .nodes()
            .get(
                usize::try_from(template.template().result().raw())
                    .map_err(|_| FactQueryError::InfrastructureFailure)?,
            )
            .ok_or(FactQueryError::InfrastructureFailure)?
            .ty();

        let diagnostic_span = self
            .dependency_interface_input(address.interface())
            .and_then(crate::request::DependencyInterfaceInput::dependency_span);

        let evaluated = checker_result(evaluate_generic_constraint_template(
            &context,
            template.template(),
            substitution,
            result_type,
            &resolver,
            diagnostic_span,
        ))?;

        let diagnostics =
            DiagnosticBag::merged_all([template_result.diagnostics(), evaluated.diagnostics()]);

        let Some(value) = evaluated.value() else {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        };

        let outcome = constant_predicate_outcome(values, *value)?;

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    fn evaluate_trait_satisfaction_constraint(
        &self,
        unit: bray_declarations::SyntaxAnchor,
        subject: &TypeExpressionTemplate,
        application: &TraitApplicationTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let mut terms = BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for occurrence in subject
            .constant_expressions()
            .into_iter()
            .chain(application.constant_expressions())
        {
            let result = self.embedded_constant_term_with_cancellation(occurrence, cancellation)?;

            diagnostics = diagnostics.merged(result.diagnostics());
            terms.insert(occurrence.key(), *result.value());
        }

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let constants = CheckedConstantTerms::try_from_terms(terms)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let values = self.semantic_value_store()?;

        let subject = resolve_type_expression_template(values, subject, &constants)
            .map_err(FactQueryError::CheckerInfrastructure)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let application = resolve_trait_application_template(values, application, &constants)
            .map_err(FactQueryError::CheckerInfrastructure)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let subject = values
            .substitute_type(subject, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let application = values
            .substitute_trait_application(application, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.evaluate_resolved_trait_satisfaction(
            Some(unit),
            subject,
            application,
            substitution,
            cancellation,
            diagnostics,
        )
    }

    fn evaluate_resolved_trait_satisfaction(
        &self,
        unit: Option<bray_declarations::SyntaxAnchor>,
        subject: bray_symbols::TypeId,
        application: bray_symbols::TraitApplicationId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        mut diagnostics: DiagnosticBag,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let application_data = values
            .trait_application_data(application)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let context = CompilationCheckerContext::new(self.binding_context(cancellation)?);

        let built_in =
            bray_checker::built_in_trait_constraint_outcome(&context, subject, application)
                .map_err(FactQueryError::CheckerInfrastructure)?;

        let outcome = if let Some(outcome) = built_in {
            outcome
        } else if self.is_copyable_trait(application_data.definition())? {
            self.evaluate_copyable_constraint(
                unit,
                subject,
                substitution,
                cancellation,
                &mut diagnostics,
            )?
        } else {
            let requirement = ImplementationRequirementKey::new(subject, application);

            let selection =
                self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

            diagnostics = diagnostics.merged(selection.diagnostics());

            match selection.value() {
                ImplementationSelection::Selected(_) => ProofOutcome::Proven,
                ImplementationSelection::Deferred => ProofOutcome::Unknown,
                ImplementationSelection::Unavailable | ImplementationSelection::Ambiguous(_) => {
                    ProofOutcome::Disproven
                }
            }
        };

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    fn evaluate_copyable_constraint(
        &self,
        unit: Option<bray_declarations::SyntaxAnchor>,
        subject: bray_symbols::TypeId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        let values = self.semantic_value_store()?;

        let result = match unit {
            Some(unit) => {
                let owner = values
                    .generic_substitution_data(substitution)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?
                    .owner()
                    .symbol();

                let key = self.constraint_unit_key(owner, unit)?;

                let context =
                    CompilationCheckerContext::new(self.binding_context_for(&key, cancellation)?);

                let semantic_context = bray_checker::SemanticUnitContext::Constraint(
                    bray_checker::DeclaredUnitContext::new(key, owner, owner),
                );

                checker_result(type_is_copyable_in_context(
                    &context,
                    &semantic_context,
                    subject,
                ))?
            }
            None => {
                let context = CompilationCheckerContext::new(self.binding_context(cancellation)?);

                checker_result(closed_type_is_copyable(&context, subject))?
            }
        };

        *diagnostics = diagnostics.merged(result.diagnostics());

        Ok(if *result.value() {
            ProofOutcome::Proven
        } else {
            ProofOutcome::Disproven
        })
    }

    pub(in crate::compilation) fn is_copyable_trait(
        &self,
        definition: TraitSymbolId,
    ) -> Result<bool, FactQueryError> {
        let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new("Copyable")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok(self
            .available_compiler_known_symbols()
            .declaration_symbol::<TraitSymbolId>(&key)
            == Some(definition))
    }

    pub(in crate::compilation) fn constraint_unit_key(
        &self,
        owner: bray_symbols::AnySymbolId,
        syntax: bray_declarations::SyntaxAnchor,
    ) -> Result<BoundUnitKey, FactQueryError> {
        let symbols = self.symbol_graph()?;

        let owner = symbols
            .symbol_key(owner)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source = self
            .source(syntax.source_id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source = BoundSourceAnchor::new(syntax, source.version());

        BoundUnitKey::constraint(owner.clone(), source).ok_or(FactQueryError::InfrastructureFailure)
    }
}

fn checker_dependency_error(error: CheckerFactError) -> FactQueryError {
    match error {
        CheckerFactError::Cancelled => FactQueryError::Cancelled,
        CheckerFactError::Infrastructure(error) => FactQueryError::CheckerInfrastructure(error),
    }
}

fn constant_predicate_outcome(
    values: &bray_symbols::SemanticValueStore,
    value: bray_symbols::ConstantValueId,
) -> Result<ProofOutcome, FactQueryError> {
    let value = values
        .constant_value_data(value)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    match value.kind() {
        ConstantValueKind::Boolean(true) => Ok(ProofOutcome::Proven),
        ConstantValueKind::Boolean(false) => Ok(ProofOutcome::Disproven),
        ConstantValueKind::Error => Ok(ProofOutcome::Recovered),
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

pub(in crate::compilation) fn constraint_expression(
    unit: &BoundUnit,
    syntax: bray_declarations::SyntaxAnchor,
) -> Option<BoundExpressionId> {
    let BoundUnitRoot::ExpressionSequence(root) = unit.root() else {
        return None;
    };

    let block = unit.view().block(root)?;

    block.items().iter().find_map(|item| {
        let BoundBlockItem::Expression(expression) = item else {
            return None;
        };

        unit.view()
            .expression(*expression)
            .filter(|bound| {
                let bound = bound.origin().source_anchor().syntax();

                bound.source_id() == syntax.source_id() && bound.full_range() == syntax.full_range()
            })
            .map(|_| *expression)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{GenericConstraintObligationKey, GenericOwnerId};

    use crate::test_support::{compilation, diagnostic_kinds};

    #[test]
    fn concrete_generic_constraints_participate_in_callable_selection() {
        let accepted = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            accepted.check_diagnostics().is_empty(),
            "{:?}",
            accepted.check_diagnostics()
        );

        let rejected = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(false)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(rejected.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn copyable_constraints_apply_inside_generic_bodies_and_at_instantiation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func duplicate<T>(pos value: T) -> T\n",
            "    with(T: Copyable)\n",
            "{\n",
            "    let first: T = value;\n",
            "    let second: T = value;\n",
            "\n",
            "    return second;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let result: i32 = duplicate<i32>(1);\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn copyable_constraints_reject_non_copyable_instantiations() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "\n",
            "func duplicate<T>(pos value: T) -> T\n",
            "    with(T: Copyable)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let resource: Resource = Resource { value = 1 };\n",
            "    let duplicate: Resource = duplicate<Resource>(resource);\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(compilation.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn trait_satisfaction_constraints_select_exact_implementations() {
        let accepted = compilation(concat!(
            "module app;\n",
            "\n",
            "trait Marker {}\n",
            "\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "\n",
            "impl Resource(Marker) {}\n",
            "\n",
            "func accept<T>(pos value: T) -> T\n",
            "    with(T: Marker)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let resource: Resource = Resource { value = 1 };\n",
            "    let accepted: Resource = accept<Resource>(resource);\n",
            "}\n",
        ));

        assert!(
            accepted.check_diagnostics().is_empty(),
            "{:#?}",
            accepted.check_diagnostics()
        );

        let rejected = compilation(concat!(
            "module app;\n",
            "\n",
            "trait Marker {}\n",
            "\n",
            "func accept<T>(pos value: T) -> T\n",
            "    with(T: Marker)\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let rejected: i32 = accept<i32>(1);\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(rejected.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn generic_constraint_results_are_cached_by_exact_substitution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        let graph = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let function = graph
            .functions()
            .iter()
            .find(|function| {
                graph
                    .member_name(function.id().into())
                    .is_some_and(|name| name.as_str() == "constrained")
            })
            .unwrap_or_else(|| panic!("constrained function must be declared"));

        let symbol = function.id().into();

        let owner = GenericOwnerId::try_new(symbol)
            .unwrap_or_else(|| panic!("function must be a generic owner"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let substitution = super::super::substitution::empty_substitution(values, symbol)
            .unwrap_or_else(|error| panic!("empty substitution must be available: {error:?}"));

        let obligation = GenericConstraintObligationKey::new(owner, substitution);

        let first = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint result must be available: {error:?}"));

        let second = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint result must be reusable: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
    }
}
