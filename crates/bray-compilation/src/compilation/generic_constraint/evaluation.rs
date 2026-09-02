use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockItem, BoundExpressionId, BoundSourceAnchor, BoundUnit, BoundUnitKey, BoundUnitRoot,
};
use bray_checker::{
    CheckedConstantTerms, CheckerQueryError, CheckerRequestContext, ConstantEvaluationInput,
    ConstantEvaluator, DefaultConstantEvaluator, closed_type_is_copyable,
    evaluate_generic_constraint_template, resolve_trait_application_template,
    resolve_type_expression_template, type_is_copyable_in_context,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    ConstantExpressionOccurrence, ConstantValueKind, GenericConstraintObligationKey,
    GenericConstraintTemplate, GenericDeclarationTemplate, GenericSubstitutionId,
    ImplementationRequirementKey, ImplementationSelection, ProofOutcome, TraitApplicationId,
    TraitApplicationTemplate, TraitSymbolId, TypeData, TypeExpressionTemplate, TypeId,
};

use super::super::Compilation;
use super::super::checker::{CompilationCheckerContext, checker_result};
use super::super::substitution::identity_substitution;
use super::super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn generic_declaration_may_be_satisfied(
        &self,
        template: &GenericDeclarationTemplate,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        if template.constraints().is_empty() {
            return Ok(true);
        }

        let substitution = identity_substitution(
            self.semantic_value_store()?,
            template.owner(),
            template.parameters(),
        )?;

        let obligation = GenericConstraintObligationKey::new(template.owner(), substitution);

        let outcome =
            self.generic_constraint_satisfaction_with_cancellation(obligation, cancellation)?;

        Ok(*outcome.value() != ProofOutcome::Disproven)
    }

    pub(super) fn evaluate_constraint(
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
                        .map_err(FactQueryError::SemanticValueStore)?;

                    let application = values
                        .substitute_trait_application(application, substitution)
                        .map_err(FactQueryError::SemanticValueStore)?;

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
                        .map_err(FactQueryError::SemanticValueStore)?;

                    let right = values
                        .substitute_type(right, substitution)
                        .map_err(FactQueryError::SemanticValueStore)?;

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
        let resolved = self.resolve_constraint_templates(
            left.constant_expressions()
                .into_iter()
                .chain(right.constant_expressions()),
            cancellation,
            |values, constants| {
                Ok((
                    resolve_constraint_type(values, left, constants, substitution)?,
                    resolve_constraint_type(values, right, constants, substitution)?,
                ))
            },
        )?;

        let (resolved, mut diagnostics) = resolved.into_parts();

        let Some((left, right)) = resolved else {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        };

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
            .map_err(FactQueryError::SemanticValueStore)?;

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
                .map_err(FactQueryError::from)?
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
            semantics.result().value().selections(),
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

        let request = super::super::unit::checker_unit_view(
            bound.result().value(),
            &semantic_context,
            &context,
        )?;

        let resolver =
            super::super::constant::CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::for_expression(
            semantics.result().value(),
            root,
            references,
            &resolver,
        );

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

        let substitution = match values.require_concrete_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(bray_symbols::SemanticValueStoreError::OpenSubstitution) => {
                return Ok(DiagnosticResult::without_diagnostics(ProofOutcome::Unknown));
            }
            Err(error) => return Err(FactQueryError::SemanticValueStore(error)),
        };

        let binding_context = self.binding_context(cancellation)?;

        let Some(address) = binding_context
            .imported_semantic_address(owner.symbol())
            .map_err(super::super::binder::binding_query_error)?
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let template_result = super::super::binder::imported_declaration_template_at(
            &binding_context,
            address,
            bray_bound_tree::CheckedTemplateKind::GenericConstraint,
            ordinal,
        )
        .map_err(super::super::binder::binding_query_error)?;

        let Some(template) = template_result.value() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let imported = imported
            .value()
            .as_deref()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let context = CompilationCheckerContext::new(binding_context);

        let resolver = super::super::constant::CompilationConstantTemplateResolver::new(
            self,
            cancellation,
            imported,
        );

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
        let resolved = self.resolve_trait_satisfaction_constraint(
            subject,
            application,
            substitution,
            cancellation,
        )?;

        let diagnostics = resolved.diagnostics().clone();

        let Some((subject, application)) = *resolved.value() else {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        };

        self.evaluate_resolved_trait_satisfaction(
            Some(unit),
            subject,
            application,
            substitution,
            cancellation,
            diagnostics,
        )
    }

    pub(super) fn resolve_trait_satisfaction_constraint(
        &self,
        subject: &TypeExpressionTemplate,
        application: &TraitApplicationTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<(TypeId, TraitApplicationId)>>, FactQueryError> {
        self.resolve_constraint_templates(
            subject
                .constant_expressions()
                .into_iter()
                .chain(application.constant_expressions()),
            cancellation,
            |values, constants| {
                Ok((
                    resolve_constraint_type(values, subject, constants, substitution)?,
                    resolve_constraint_trait_application(
                        values,
                        application,
                        constants,
                        substitution,
                    )?,
                ))
            },
        )
    }

    fn resolve_constraint_templates<T>(
        &self,
        occurrences: impl IntoIterator<Item = ConstantExpressionOccurrence>,
        cancellation: &CancellationToken,
        resolve: impl FnOnce(
            &bray_symbols::SemanticValueStore,
            &CheckedConstantTerms,
        ) -> Result<T, FactQueryError>,
    ) -> Result<DiagnosticResult<Option<T>>, FactQueryError> {
        let mut terms = BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for occurrence in occurrences {
            let result = self.embedded_constant_term_with_cancellation(occurrence, cancellation)?;

            diagnostics = diagnostics.merged(result.diagnostics());
            terms.insert(occurrence.key(), *result.value());
        }

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let constants = CheckedConstantTerms::try_from_terms(terms)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let value = resolve(self.semantic_value_store()?, &constants)?;

        Ok(DiagnosticResult::new(Some(value), diagnostics))
    }

    fn evaluate_resolved_trait_satisfaction(
        &self,
        unit: Option<bray_declarations::SyntaxAnchor>,
        subject: TypeId,
        application: TraitApplicationId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        mut diagnostics: DiagnosticBag,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let application_data = values
            .trait_application_data(application)
            .map_err(FactQueryError::SemanticValueStore)?;

        let context = CompilationCheckerContext::new(self.binding_context(cancellation)?);

        let built_in =
            bray_checker::built_in_trait_constraint_outcome(&context, subject, application)
                .map_err(FactQueryError::from)?;

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
        subject: TypeId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        let values = self.semantic_value_store()?;

        let result = match unit {
            Some(unit) => {
                let owner = values
                    .generic_substitution_data(substitution)
                    .map_err(FactQueryError::SemanticValueStore)?
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

fn resolve_constraint_type(
    values: &bray_symbols::SemanticValueStore,
    template: &TypeExpressionTemplate,
    constants: &CheckedConstantTerms,
    substitution: GenericSubstitutionId,
) -> Result<TypeId, FactQueryError> {
    let resolved = resolve_type_expression_template(values, template, constants)
        .map_err(FactQueryError::from)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

    values
        .substitute_type(resolved, substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

fn resolve_constraint_trait_application(
    values: &bray_symbols::SemanticValueStore,
    template: &TraitApplicationTemplate,
    constants: &CheckedConstantTerms,
    substitution: GenericSubstitutionId,
) -> Result<TraitApplicationId, FactQueryError> {
    let resolved = resolve_trait_application_template(values, template, constants)
        .map_err(FactQueryError::from)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

    values
        .substitute_trait_application(resolved, substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

fn checker_dependency_error(error: CheckerQueryError<FactQueryError>) -> FactQueryError {
    match error {
        CheckerQueryError::Cancelled => FactQueryError::Cancelled,
        CheckerQueryError::Infrastructure(error) => FactQueryError::CheckerInfrastructure(error),
        CheckerQueryError::Upstream(error) => error,
    }
}

fn constant_predicate_outcome(
    values: &bray_symbols::SemanticValueStore,
    value: bray_symbols::ConstantValueId,
) -> Result<ProofOutcome, FactQueryError> {
    let value = values
        .constant_value_data(value)
        .map_err(FactQueryError::SemanticValueStore)?;

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
