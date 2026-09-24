// rust-style: allow(module-too-large, reason = "constant queries share one recursive evaluation and dependency context")

use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_binder::semantic_unit_context;
use bray_bound_tree::{
    BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitKey, BoundUnitKind,
    CheckedSemanticSelections, CheckedTemplateKind,
};
use bray_checker::{
    ConstantChecker, ConstantEvaluationInput, ConstantEvaluationLimits, ConstantEvaluator,
    ConstantReferenceResolution, DefaultConstantChecker, DefaultConstantEvaluator,
    EvaluatedConstantCall, evaluate_constant_definition_template, resolve_type_expression_template,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, CallableDefinitionId, ConstantDefinition,
    ConstantDefinitionQuery, ConstantDefinitionState, ConstantInstanceKey, ConstantTermData,
    ConstantTermId, ConstantValueId, ErrorConstantDefinition, GenericSubstitutionId,
    SymbolQueryRequest, TraitConstantFulfillmentDeclaredTypeQuery,
    TraitConstantFulfillmentDefinitionQuery, TraitConstantMemberDeclaredTypeQuery,
    TraitConstantMemberDefinitionQuery,
};

use super::support::{
    ConcreteReferenceContext, collect_constant_references, constant_definition_id, expression_root,
    imported_constant_definition, selected_implementation_for_reference,
    substitute_expression_types,
};
use crate::compilation::Compilation;
use crate::compilation::binder::{
    binding_query_error, has_visible_generic_parameters, imported_declaration_template,
};
use crate::compilation::checker::checker_result;
use crate::compilation::constant::call::{
    CompilationConstantCallResolver, CompilationConstantTemplateResolver,
};
use crate::compilation::substitution::empty_substitution;

use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{
    CancellationToken, CompilationFactKey, ConstantInstanceQueryKey, FactQueryError,
    PublishedUnitResult,
};

impl Compilation {
    pub(in crate::compilation) fn resolve_surface_constant(
        &self,
        symbol: AnySymbolId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<(bray_symbols::TypeId, ConstantReferenceResolution)>, FactQueryError> {
        let Some(definition) = constant_definition_id(symbol) else {
            return Ok(None);
        };

        let definition_result =
            self.constant_definition_with_cancellation(definition, cancellation)?;

        *diagnostics = diagnostics.merged(definition_result.diagnostics());

        let ConstantDefinitionState::Defined(definition_data) = definition_result.value() else {
            return Ok(None);
        };

        let ty = definition_data.ty();
        let term = definition_data.term();

        let term_data = self.semantic_value_store()?.constant_term_data(term);

        if let ConstantTermData::Value(value) = term_data.as_ref() {
            return Ok(Some((ty, ConstantReferenceResolution::Value(*value))));
        }

        if !self.constant_can_evaluate_without_context(symbol, definition)? {
            return Ok(Some((ty, ConstantReferenceResolution::Term(term))));
        }

        let substitution = crate::compilation::substitution::empty_substitution(
            self.semantic_value_store()?,
            definition.into_any(),
        )?;

        let instance = ConstantInstanceKey::new(definition, substitution, None);

        let resolution = match self.constant_instance_with_cancellation(instance, cancellation) {
            Ok(result) => {
                *diagnostics = diagnostics.merged(result.diagnostics());

                ConstantReferenceResolution::Evaluated(*result.value())
            }
            Err(FactQueryError::Cycle(_)) => ConstantReferenceResolution::Cycle {
                definition: self.constant_definition_span(definition)?,
            },
            Err(error) => return Err(error),
        };

        Ok(Some((ty, resolution)))
    }

    fn constant_can_evaluate_without_context(
        &self,
        symbol: AnySymbolId,
        definition: AnyConstantDefinitionId,
    ) -> Result<bool, FactQueryError> {
        let symbols = self.symbol_graph()?;

        Ok(
            !matches!(definition, AnyConstantDefinitionId::TraitMember(_))
                && !has_visible_generic_parameters(symbols, symbol),
        )
    }

    /// Returns the semantic definition state of one constant declaration.
    pub fn constant_definition(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Arc<DiagnosticResult<ConstantDefinitionState>>, FactQueryError> {
        self.constant_definition_with_cancellation(definition, &self.state.cancellation)
    }

    pub(in crate::compilation) fn constant_definition_with_cancellation(
        &self,
        definition: AnyConstantDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<ConstantDefinitionState>>, FactQueryError> {
        let binding_context = self.binding_context(cancellation)?;

        match definition {
            AnyConstantDefinitionId::Constant(owner) => binding_context
                .resolve_symbol_query(SymbolQueryRequest::<ConstantDefinitionQuery>::new(owner))
                .map_err(binding_query_error),
            AnyConstantDefinitionId::TraitMember(owner) => binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitConstantMemberDefinitionQuery>::new(owner),
                )
                .map_err(binding_query_error),
            AnyConstantDefinitionId::TraitFulfillment(owner) => binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitConstantFulfillmentDefinitionQuery>::new(owner),
                )
                .map_err(binding_query_error),
        }
    }

    /// Returns the checked symbolic term for one constant definition initializer.
    pub fn symbolic_constant_term(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Arc<DiagnosticResult<ConstantTermId>>, FactQueryError> {
        let key = self.constant_template_key(definition)?.ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(definition.into_any()),
                SemanticQueryViolation::Missing(SemanticDataKind::BoundUnit),
            )
        })?;

        let published =
            self.symbolic_constant_term_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns the closed value of one exact constant instance.
    pub fn constant_instance(
        &self,
        instance: ConstantInstanceKey,
    ) -> Result<Arc<DiagnosticResult<EvaluatedConstantCall>>, FactQueryError> {
        self.constant_instance_with_cancellation(instance, &self.state.cancellation)
    }

    pub(in crate::compilation) fn compute_constant_definition(
        &self,
        definition: AnyConstantDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ConstantDefinitionState>, FactQueryError> {
        if let Some(value) = self.target_constant_value(definition)? {
            let values = self.semantic_value_store()?;

            let ty = values.constant_value_data(value).ty();

            let term = values
                .intern_constant_term(ConstantTermData::Value(value))
                .map_err(FactQueryError::SemanticValueStore)?;

            return Ok(DiagnosticResult::without_diagnostics(
                ConstantDefinitionState::Defined(ConstantDefinition::new(ty, term)),
            ));
        }

        let Some(key) = self.constant_template_key(definition)? else {
            let binding_context = self.binding_context(cancellation)?;

            let imported = binding_context
                .imported_semantic_address(definition.into_any())
                .map_err(binding_query_error)?;

            if let Some(address) = imported {
                let result = imported_declaration_template(
                    &binding_context,
                    address,
                    CheckedTemplateKind::ConstantDefinition,
                )
                .map_err(binding_query_error)?;

                let mut diagnostics = result.diagnostics().clone();

                if result.value().is_none() && diagnostics.has_errors() {
                    let declared = self.constant_error_definition_type(
                        &binding_context,
                        definition,
                        cancellation,
                    )?;

                    diagnostics = diagnostics.merged(declared.diagnostics());

                    return Ok(DiagnosticResult::new(
                        ConstantDefinitionState::Error(ErrorConstantDefinition::new(
                            *declared.value(),
                        )),
                        diagnostics,
                    ));
                }

                let state = imported_constant_definition(definition, result.value().as_ref())?;

                return Ok(DiagnosticResult::new(state, diagnostics));
            }

            return match definition {
                AnyConstantDefinitionId::TraitMember(_) => Ok(
                    DiagnosticResult::without_diagnostics(ConstantDefinitionState::Required),
                ),
                AnyConstantDefinitionId::Constant(_)
                | AnyConstantDefinitionId::TraitFulfillment(_) => {
                    Err(SemanticQueryFailure::contract(
                        SemanticQueryContext::Symbol(definition.into_any()),
                        SemanticQueryViolation::Missing(SemanticDataKind::ConstantDefinition),
                    )
                    .into())
                }
            };
        };

        let term = self.symbolic_constant_term_with_cancellation(key.clone(), cancellation)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let root = expression_root(bound.result().value())?;
        let expressions = self.expression_semantics_with_cancellation(key, cancellation)?;

        let Some(root_type) = expressions.result().value().types().expression(root) else {
            return Err(SemanticQueryFailure::contract(
                SemanticQueryContext::Expression {
                    unit: bound.result().value().key().clone(),
                    expression: root,
                },
                SemanticQueryViolation::Missing(SemanticDataKind::Type),
            )
            .into());
        };

        let diagnostics = term.result().diagnostics().clone();

        let state = if diagnostics.has_errors() {
            ConstantDefinitionState::Error(ErrorConstantDefinition::new(root_type.ty()))
        } else {
            ConstantDefinitionState::Defined(ConstantDefinition::new(
                root_type.ty(),
                *term.result().value(),
            ))
        };

        Ok(DiagnosticResult::new(state, diagnostics))
    }

    fn constant_error_definition_type(
        &self,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        definition: AnyConstantDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<bray_symbols::TypeId>, FactQueryError> {
        let template = match definition {
            AnyConstantDefinitionId::Constant(owner) => binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<bray_symbols::ConstantDeclaredTypeQuery>::new(owner),
                )
                .map_err(binding_query_error)?,
            AnyConstantDefinitionId::TraitMember(owner) => binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<TraitConstantMemberDeclaredTypeQuery>::new(owner),
                )
                .map_err(binding_query_error)?,
            AnyConstantDefinitionId::TraitFulfillment(owner) => binding_context
                .resolve_symbol_query(SymbolQueryRequest::<
                    TraitConstantFulfillmentDeclaredTypeQuery,
                >::new(owner))
                .map_err(binding_query_error)?,
        };

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [template.value()],
            cancellation,
        )?;

        let ty = resolve_type_expression_template(
            self.semantic_value_store()?,
            template.value(),
            constants.value(),
        )
        .map_err(FactQueryError::from)?
        .ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(definition.into_any()),
                SemanticQueryViolation::Missing(SemanticDataKind::Type),
            )
        })?;

        Ok(DiagnosticResult::new(
            ty,
            template.diagnostics().merged(constants.diagnostics()),
        ))
    }

    pub(in crate::compilation) fn symbolic_constant_term_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<ConstantTermId>>, FactQueryError> {
        if !matches!(
            key.kind(),
            BoundUnitKind::ConstantTemplate | BoundUnitKind::EmbeddedConstant
        ) {
            return Err(SemanticQueryFailure::contract(
                SemanticQueryContext::Unit(key),
                SemanticQueryViolation::Unsupported(SemanticDataKind::ConstantTerm),
            )
            .into());
        }

        self.unit_query(
            &self.state.symbolic_constant_terms,
            CompilationFactKey::SymbolicConstantTerm(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let semantics =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let references = self.symbolic_references(
                    bound.result().value(),
                    semantics.result().value().selections(),
                )?;

                let resolver = CompilationConstantCallResolver::new(self, cancellation);

                let input = ConstantEvaluationInput::new(
                    semantics.result().value().types(),
                    semantics.result().value().selections(),
                )
                .with_references(references)
                .with_call_resolver(&resolver)
                .for_definition();

                let unit = bray_checker::CheckerUnitView::new(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                );

                let checked =
                    checker_result(DefaultConstantChecker.check_constant_term(unit, &input))?;

                let diagnostics = DiagnosticBag::merged_all([
                    semantics.result().diagnostics(),
                    checked.diagnostics(),
                ]);

                Ok((
                    DiagnosticResult::new(*checked.value(), diagnostics),
                    Box::new([]),
                ))
            },
        )
    }

    pub(in crate::compilation) fn constant_instance_with_cancellation(
        &self,
        instance: ConstantInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<EvaluatedConstantCall>>, FactQueryError> {
        self.constant_instance_with_limits(
            instance,
            ConstantEvaluationLimits::default(),
            cancellation,
        )
    }

    pub(in crate::compilation) fn constant_instance_with_limits(
        &self,
        instance: ConstantInstanceKey,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<EvaluatedConstantCall>>, FactQueryError> {
        let target = self.requested_target().profile().clone();
        let key = ConstantInstanceQueryKey::new(instance, target, limits);
        let cell = self.state.constant_instances.cell(key.clone())?;

        let published = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ConstantInstance(key),
            cancellation,
            || {
                self.compute_constant_instance(instance, limits, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(published))
    }

    fn compute_constant_instance(
        &self,
        instance: ConstantInstanceKey,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<EvaluatedConstantCall>, FactQueryError> {
        if let Some(value) = self.target_constant_value(instance.definition())? {
            return Ok(DiagnosticResult::without_diagnostics(
                EvaluatedConstantCall::new(value, Default::default()),
            ));
        }

        let Some(key) = self.constant_template_key(instance.definition())? else {
            return self.compute_imported_constant_instance(instance, limits, cancellation);
        };

        let term = self.symbolic_constant_term_with_cancellation(key.clone(), cancellation)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(&key, cancellation)?;

        let semantic_context = semantic_unit_context(context.symbols(), bound.result().value());

        let types = substitute_expression_types(
            self.semantic_value_store()?,
            semantics.result().value().types(),
            instance.substitution(),
        )?;

        if term.result().diagnostics().has_errors() {
            let root = expression_root(bound.result().value())?;

            let ty = types
                .expression(root)
                .ok_or_else(|| {
                    SemanticQueryFailure::contract(
                        SemanticQueryContext::Expression {
                            unit: bound.result().value().key().clone(),
                            expression: root,
                        },
                        SemanticQueryViolation::Missing(SemanticDataKind::Type),
                    )
                })?
                .ty();

            let value = self
                .semantic_value_store()?
                .intern_error_constant_value(ty)
                .map_err(FactQueryError::SemanticValueStore)?;

            return Ok(DiagnosticResult::without_diagnostics(
                EvaluatedConstantCall::new(value, Default::default()),
            ));
        }

        let (references, dependency_diagnostics) = self.concrete_references(
            bound.result().value(),
            semantics.result().value().selections(),
            instance,
            limits,
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(&types, semantics.result().value().selections())
            .with_references(references)
            .with_call_resolver(&resolver)
            .with_limits(limits)
            .for_definition();

        let unit =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context);

        let evaluated = checker_result(
            DefaultConstantEvaluator.evaluate_constant_with_references(unit, &input),
        )?;

        let diagnostics = dependency_diagnostics.merged(evaluated.diagnostics());

        Ok(DiagnosticResult::new(
            EvaluatedConstantCall::new(evaluated.value().value(), evaluated.value().usage()),
            diagnostics,
        ))
    }

    fn compute_imported_constant_instance(
        &self,
        instance: ConstantInstanceKey,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<EvaluatedConstantCall>, FactQueryError> {
        let binding_context = self.binding_context(cancellation)?;

        let address = binding_context
            .imported_semantic_address(instance.definition().into_any())
            .map_err(binding_query_error)?
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(instance.definition().into_any()),
                    SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
                )
            })?;

        let template = imported_declaration_template(
            &binding_context,
            address,
            CheckedTemplateKind::ConstantDefinition,
        )
        .map_err(binding_query_error)?;

        let definition_result =
            self.constant_definition_with_cancellation(instance.definition(), cancellation)?;

        let definition = match definition_result.value() {
            ConstantDefinitionState::Defined(definition) => *definition,
            ConstantDefinitionState::Error(error) => {
                return recovered_error_constant_instance(
                    self.semantic_value_store()?,
                    *error,
                    instance.substitution(),
                    DiagnosticBag::merged_all([
                        template.diagnostics(),
                        definition_result.diagnostics(),
                    ]),
                );
            }
            ConstantDefinitionState::Required => {
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(instance.definition().into_any()),
                    SemanticQueryViolation::Unsupported(SemanticDataKind::ConstantDefinition),
                )
                .into());
            }
        };

        let imported = template.value().as_ref().ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(instance.definition().into_any()),
                SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
            )
        })?;

        let imported_symbols =
            self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(symbols) = imported_symbols.value().as_deref() else {
            if imported_symbols.diagnostics().has_errors() {
                return recovered_error_constant_instance(
                    self.semantic_value_store()?,
                    ErrorConstantDefinition::new(definition.ty()),
                    instance.substitution(),
                    DiagnosticBag::merged_all([
                        template.diagnostics(),
                        definition_result.diagnostics(),
                        imported_symbols.diagnostics(),
                    ]),
                );
            }

            return Err(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(instance.definition().into_any()),
                SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
            )
            .into());
        };

        let context = self.checker_context(cancellation)?;

        let resolver = CompilationConstantTemplateResolver::new(self, cancellation, symbols);

        let diagnostic_span = self
            .dependency_interface_input(address.interface())
            .and_then(crate::request::DependencyInterfaceInput::dependency_span);

        let evaluated = checker_result(evaluate_constant_definition_template(
            &context,
            imported.template(),
            instance.substitution(),
            definition.ty(),
            &resolver,
            diagnostic_span,
            limits,
        ))?;

        let diagnostics = DiagnosticBag::merged_all([
            template.diagnostics(),
            definition_result.diagnostics(),
            imported_symbols.diagnostics(),
            evaluated.diagnostics(),
        ]);

        if let Some(evaluated) = evaluated.value() {
            return Ok(DiagnosticResult::new(*evaluated, diagnostics));
        }

        recovered_error_constant_instance(
            self.semantic_value_store()?,
            ErrorConstantDefinition::new(definition.ty()),
            instance.substitution(),
            diagnostics,
        )
    }

    pub(in crate::compilation) fn symbolic_references(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
        self.symbolic_references_with_arguments(bound, selections, &BTreeMap::new())
    }

    pub(in crate::compilation) fn symbolic_references_with_arguments(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
        arguments: &BTreeMap<AnySymbolId, bray_symbols::SymbolOrdinal>,
    ) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
        let values = self.semantic_value_store()?;

        collect_constant_references(bound, selections, |_, target| {
            if let Some(ordinal) = match target {
                BoundReferenceTarget::Surface(symbol) => arguments.get(&symbol),
                BoundReferenceTarget::Local(_) => None,
            } {
                return values
                    .intern_constant_term(ConstantTermData::CallableArgument(*ordinal))
                    .map(ConstantReferenceResolution::Term)
                    .map_err(FactQueryError::SemanticValueStore);
            }

            match target {
                BoundReferenceTarget::Surface(AnySymbolId::GenericConstParameter(parameter)) => {
                    values
                        .intern_constant_term(ConstantTermData::Parameter(parameter))
                        .map(ConstantReferenceResolution::Term)
                        .map_err(FactQueryError::SemanticValueStore)
                }
                BoundReferenceTarget::Surface(symbol) => {
                    let Some(definition) = constant_definition_id(symbol) else {
                        return Ok(ConstantReferenceResolution::Invalid);
                    };

                    let substitution = empty_substitution(values, definition.into_any())?;

                    let term = values
                        .intern_constant_term(ConstantTermData::DefinitionApplication {
                            definition,
                            substitution,
                            selected_implementation: None,
                        })
                        .map_err(FactQueryError::SemanticValueStore)?;

                    Ok(ConstantReferenceResolution::Term(term))
                }
                BoundReferenceTarget::Local(_) => Ok(ConstantReferenceResolution::Invalid),
            }
        })
    }

    fn concrete_references(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
        instance: ConstantInstanceKey,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        self.collect_concrete_references(
            bound,
            selections,
            ConcreteReferenceContext {
                substitution: Some(instance.substitution()),
                selected_implementation: instance.selected_implementation(),
                parameters: &BTreeMap::new(),
                current_constant: Some(instance),
            },
            limits,
            cancellation,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "constant call evaluation keeps semantic inputs, limits, and cancellation explicit"
    )]
    pub(in crate::compilation) fn concrete_call_references(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
        substitution: GenericSubstitutionId,
        selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
        parameters: &BTreeMap<AnySymbolId, ConstantValueId>,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        self.collect_concrete_references(
            bound,
            selections,
            ConcreteReferenceContext {
                substitution: Some(substitution),
                selected_implementation,
                parameters,
                current_constant: None,
            },
            limits,
            cancellation,
        )
    }

    pub(in crate::compilation) fn concrete_embedded_references(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        self.collect_concrete_references(
            bound,
            selections,
            ConcreteReferenceContext {
                substitution: None,
                selected_implementation: None,
                parameters: &BTreeMap::new(),
                current_constant: None,
            },
            limits,
            cancellation,
        )
    }

    fn collect_concrete_references(
        &self,
        bound: &BoundUnit,
        selections: &CheckedSemanticSelections,
        context: ConcreteReferenceContext<'_>,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;

        let substitution = context
            .substitution
            .map(|substitution| values.generic_substitution_data(substitution));

        let mut dependency_diagnostics = DiagnosticBag::new();
        let unit = bound.key().clone();

        let references =
            collect_constant_references(bound, selections, |expression, target| match target {
                BoundReferenceTarget::Surface(symbol)
                    if context.parameters.contains_key(&symbol) =>
                {
                    context
                        .parameters
                        .get(&symbol)
                        .copied()
                        .map(ConstantReferenceResolution::Value)
                        .ok_or_else(|| {
                            SemanticQueryFailure::contract(
                                SemanticQueryContext::Expression {
                                    unit: unit.clone(),
                                    expression,
                                },
                                SemanticQueryViolation::Missing(SemanticDataKind::LiteralValue),
                            )
                            .into()
                        })
                }
                BoundReferenceTarget::Surface(AnySymbolId::GenericConstParameter(parameter)) => {
                    let Some(substitution) = &substitution else {
                        return Err(SemanticQueryFailure::contract(
                            SemanticQueryContext::Expression {
                                unit: unit.clone(),
                                expression,
                            },
                            SemanticQueryViolation::Missing(SemanticDataKind::GenericSubstitution),
                        )
                        .into());
                    };

                    let Some(bray_symbols::GenericArgument::Constant(term)) = substitution
                        .argument_for(bray_symbols::GenericParameterSymbolId::Const(parameter))
                    else {
                        return Err(SemanticQueryFailure::contract(
                            SemanticQueryContext::Expression {
                                unit: unit.clone(),
                                expression,
                            },
                            SemanticQueryViolation::Missing(SemanticDataKind::ConstantTerm),
                        )
                        .into());
                    };

                    let data = values.constant_term_data(term);

                    match data.as_ref() {
                        ConstantTermData::Value(value) => {
                            Ok(ConstantReferenceResolution::Value(*value))
                        }
                        _ => Err(SemanticQueryFailure::contract(
                            SemanticQueryContext::Expression {
                                unit: unit.clone(),
                                expression,
                            },
                            SemanticQueryViolation::Unsupported(SemanticDataKind::ConstantTerm),
                        )
                        .into()),
                    }
                }
                BoundReferenceTarget::Surface(symbol) => {
                    let Some(definition) = constant_definition_id(symbol) else {
                        return Err(SemanticQueryFailure::contract(
                            SemanticQueryContext::Symbol(symbol),
                            SemanticQueryViolation::Unsupported(
                                SemanticDataKind::ConstantDefinition,
                            ),
                        )
                        .into());
                    };

                    let dependency = if let Some(instance) = context
                        .current_constant
                        .filter(|instance| definition == instance.definition())
                    {
                        instance
                    } else {
                        ConstantInstanceKey::new(
                            definition,
                            crate::compilation::substitution::empty_substitution(
                                values,
                                definition.into_any(),
                            )?,
                            selected_implementation_for_reference(
                                definition,
                                context.selected_implementation,
                            ),
                        )
                    };

                    match self.constant_instance_with_limits(dependency, limits, cancellation) {
                        Ok(result) => {
                            dependency_diagnostics =
                                dependency_diagnostics.merged(result.diagnostics());

                            Ok(ConstantReferenceResolution::Evaluated(*result.value()))
                        }
                        Err(FactQueryError::Cycle(_)) => Ok(ConstantReferenceResolution::Cycle {
                            definition: self.constant_definition_span(definition)?,
                        }),
                        Err(error) => Err(error),
                    }
                }
                BoundReferenceTarget::Local(_) => Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::Expression {
                        unit: unit.clone(),
                        expression,
                    },
                    SemanticQueryViolation::Unsupported(SemanticDataKind::ConstantDefinition),
                )
                .into()),
            })?;

        Ok((references, dependency_diagnostics))
    }

    pub(in crate::compilation) fn constant_template_key(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        self.declared_unit_key(definition.into_any(), BoundUnitKind::ConstantTemplate)
    }

    pub(in crate::compilation) fn constant_definition_span(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Option<SourceSpan>, FactQueryError> {
        let symbols = self.symbol_graph()?;

        let Some(declaration) = symbols
            .symbol_key(definition.into_any())
            .and_then(bray_symbols::SymbolKey::source_declaration_id)
        else {
            return Ok(None);
        };

        let source_graph = self.product_source_graph()?;

        let record = source_graph
            .declarations()
            .declaration(declaration)
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(definition.into_any()),
                    SemanticQueryViolation::Missing(SemanticDataKind::SourceAnchor),
                )
            })?;

        let anchor = record.syntax_anchor();

        Ok(Some(SourceSpan::new(
            anchor.source_id(),
            anchor.full_range(),
        )))
    }

    pub(in crate::compilation) fn callable_body_key(
        &self,
        definition: CallableDefinitionId,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        self.declared_unit_key(
            definition.callable_symbol().into_any(),
            BoundUnitKind::CallableBody,
        )
    }
}

fn recovered_error_constant_instance(
    values: &bray_symbols::SemanticValueStore,
    definition: ErrorConstantDefinition,
    substitution: GenericSubstitutionId,
    diagnostics: DiagnosticBag,
) -> Result<DiagnosticResult<EvaluatedConstantCall>, FactQueryError> {
    let ty = values
        .substitute_type(definition.ty(), substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    let value = values
        .intern_error_constant_value(ty)
        .map_err(FactQueryError::SemanticValueStore)?;

    Ok(DiagnosticResult::new(
        EvaluatedConstantCall::new(value, Default::default()),
        diagnostics,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticRelatedLocationKind};
    use bray_symbols::{
        AnyConstantDefinitionId, CallableDefinitionId, CallableInstanceData,
        CallableParameterSignature, CallableParameterSymbolId, CallableSignature,
        ConstantDefinitionState, ConstantField, ConstantInstanceKey, ConstantTermData,
        ConstantValueData, ConstantValueId, ConstantValueKind, ErrorConstantDefinition,
        FunctionSymbolId, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
        GenericSubstitutionData, GenericTypeParameterSymbolId, IntegerConstant, IntegerSign,
        NamedTypeSymbolId, ReceiverMode, ReceiverParameterSignature, ReceiverParameterSymbolId,
        SymbolId, SymbolOrigin, TypeData, UnionSymbolId,
    };
    use bray_target::{TargetIdentity, TargetProfile};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::support::{call_parameter_values, constant_callable_root};
    use super::recovered_error_constant_instance;
    use crate::SelectedTarget;
    use crate::fact::{ConstantCallQueryKey, ConstantInstanceQueryKey, FactCellTestEvent};
    use crate::test_support::{FactTestGate, compilation};
    use bray_checker::{ConstantCallRequest, ConstantEvaluationLimits};

    #[test]
    fn ordinary_constant_rejects_owned_lifecycle_value_but_static_accepts_it() {
        let source = concat!(
            "module app;\n",
            "struct Guard\n",
            "{\n",
            "    value: i32;\n",
            "    destruct() { panic(\"cleanup\"); }\n",
            "}\n",
            "const G: Guard = Guard { value = 1, };\n",
            "static STORED: Guard = Guard { value = 2, };\n",
            "func use_constant() { let value = G; }\n",
        );

        let compilation = compilation(source);
        let diagnostics = compilation.check_diagnostics();

        assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingNonMaterializableConstant,
        );

        let rejected: Vec<_> = diagnostics
            .by_kind(DiagnosticKind::CheckingNonMaterializableConstant)
            .collect();

        assert_eq!(rejected.len(), 1, "{diagnostics:?}");
        assert!(rejected[0].primary_span().is_some());
    }

    #[test]
    fn ordinary_constant_checks_active_values_and_const_calls() {
        const COMMON: &str = r#"
            module app;
            struct Guard
            {
                value: i32;
                destruct() { panic("cleanup"); }
            }
            union Choice
            {
                Empty;
                Owned(value: Guard);
            }
        "#;

        for definition in [
            "const BAD: (Guard, i32) = (Guard { value = 1, }, 2);",
            "const BAD: [Guard; 1] = [Guard { value = 1, }];",
            "struct Wrapped { inner: Guard; } const BAD: Wrapped = Wrapped { inner = Guard { value = 1, }, };",
            "const BAD: Guard? = Guard { value = 1, };",
            "const BAD: Choice = .Owned(value = Guard { value = 1, });",
            "const func take(pos value: Guard) -> i32 { return 1; } const BAD: i32 = take(Guard { value = 1, });",
            "const func make() -> Guard { return Guard { value = 1, }; } const BAD: Guard = make();",
            "const func same<T>(pos value: T) -> T { return value; } const BAD: Guard = same<Guard>(Guard { value = 1, });",
            "struct Wrapped { inner: Guard; count: i32; } const func make() -> Wrapped { return Wrapped { inner = Guard { value = 1, }, count = 2, }; } const BAD: i32 = make().count;",
            "const FIRST: Guard = Guard { value = 1, }; const BAD: Guard = FIRST;",
        ] {
            let source = format!("{COMMON}{definition}");
            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingNonMaterializableConstant,
            );

            assert!(
                diagnostics
                    .by_kind(DiagnosticKind::CheckingNonMaterializableConstant)
                    .all(|diagnostic| diagnostic.primary_span().is_some()),
                "{definition}: {diagnostics:?}"
            );
        }

        for definition in [
            "const GOOD: Choice = .Empty;",
            "const GOOD: Guard? = none;",
            "const GOOD: (string, i32) = (\"text\", 2);",
        ] {
            let source = format!("{COMMON}{definition}");
            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{definition}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn constant_instances_demand_only_referenced_definitions_and_reuse_publications() {
        let compilation = Arc::new(compilation(concat!(
            "module app;\n",
            "const first: i32 = 1;\n",
            "const second: i32 = first;\n",
            "const unrelated: i32 = 99;\n",
        )));

        let definitions = source_constant_definitions(&compilation);

        let [first, second, unrelated] = definitions.as_slice() else {
            panic!("test source must produce three constant definitions");
        };

        let first = instance_key(&compilation, *first);
        let second = instance_key(&compilation, *second);
        let unrelated = instance_key(&compilation, *unrelated);

        let second_unit = compilation
            .constant_template_key(second.definition())
            .unwrap_or_else(|error| panic!("second constant key must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("second constant must have a body"));

        assert!(!instance_is_published(&compilation, first));
        assert!(!instance_is_published(&compilation, second));
        assert!(!instance_is_published(&compilation, unrelated));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);
        let cache_key = instance_semantic_key(&compilation, second);

        compilation
            .state
            .constant_instances
            .cell(cache_key)
            .unwrap_or_else(|error| panic!("constant instance cell must exist: {error:?}"))
            .set_test_observer(gate.observer())
            .unwrap_or_else(|error| panic!("constant instance observer must attach: {error:?}"));

        let (left, right) = std::thread::scope(|scope| {
            let left_compilation = Arc::clone(&compilation);
            let left = scope.spawn(move || left_compilation.constant_instance(second));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let right_compilation = Arc::clone(&compilation);
            let right = scope.spawn(move || right_compilation.constant_instance(second));

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            (
                left.join()
                    .unwrap_or_else(|_| panic!("first constant worker must not panic")),
                right
                    .join()
                    .unwrap_or_else(|_| panic!("second constant worker must not panic")),
            )
        });

        let left = left.unwrap_or_else(|error| panic!("constant instance must publish: {error:?}"));

        let right =
            right.unwrap_or_else(|error| panic!("constant instance must publish: {error:?}"));

        assert!(Arc::ptr_eq(&left, &right));
        assert_eq!(integer_value(&compilation, left.value().value()), 1);
        assert!(left.diagnostics().is_empty());

        assert!(instance_is_published(&compilation, first));
        assert!(instance_is_published(&compilation, second));
        assert!(!instance_is_published(&compilation, unrelated));

        assert_eq!(
            compilation
                .state
                .checked_control_flow
                .is_published(&second_unit),
            Ok(false)
        );
    }

    #[test]
    fn selected_constant_calls_evaluate_source_bodies_once_per_exact_request() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func answer() -> i32\n",
            "{\n",
            "    return 42;\n",
            "}\n",
        ));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let ty = i32_type(&compilation);

        let (definition, request) = source_constant_call_request(&compilation, [], ty);

        let callable = values
            .intern_callable_instance(request.callable())
            .unwrap_or_else(|error| panic!("constant callable instance must intern: {error:?}"));

        let target = compilation.requested_target().profile().clone();

        let shallow = ConstantCallQueryKey::new(
            callable,
            None,
            Arc::from(request.arguments()),
            ty,
            target.clone(),
            ConstantEvaluationLimits::default().with_call_depth(1),
        );

        let deep = ConstantCallQueryKey::new(
            callable,
            None,
            Arc::from(request.arguments()),
            ty,
            target,
            ConstantEvaluationLimits::default().with_call_depth(8),
        );

        assert_ne!(shallow, deep);
        assert_eq!(shallow.dependency_key(), deep.dependency_key());

        let body_key = compilation
            .callable_body_key(definition)
            .unwrap_or_else(|error| panic!("constant callable body key must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("source constant callable must have a body"));

        let body = compilation
            .bound_unit(body_key)
            .unwrap_or_else(|error| panic!("constant callable body must bind: {error:?}"));

        assert!(
            constant_callable_root(body.value()).is_some(),
            "unexpected constant callable body: {:#?}",
            body.value()
        );

        let first = compilation
            .constant_call_with_cancellation(&request, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("selected constant call must publish: {error:?}"));

        let second = compilation
            .constant_call_with_cancellation(&request, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("selected constant call must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let Some(value) = *first.value() else {
            panic!("constant callable must be eligible for evaluation");
        };

        assert_eq!(integer_value(&compilation, value.value()), 42);
    }

    #[test]
    fn pattern_conditions_evaluate_constant_calls_and_conditional_bindings() {
        let compilation = compilation(
            "module app; const func choose(pos input: i32) -> i32 { if input matches 0 | 1 { return 10; } else if let value = input && value > 5 && let next = value + 1 { return next; } else { return 99; } }",
        );

        let result_type = i32_type(&compilation);

        for (input, expected) in [(0, 10), (1, 10), (3, 99), (7, 8)] {
            let input = integer_constant(&compilation, result_type, input);

            let (_, request) = source_constant_call_request(&compilation, [input], result_type);

            let result = compilation
                .constant_call_with_cancellation(&request, &compilation.state.cancellation)
                .unwrap();

            assert!(
                result.diagnostics().is_empty(),
                "{:?}",
                result.diagnostics()
            );

            assert_eq!(
                integer_value(&compilation, result.value().unwrap().value()),
                expected
            );
        }
    }

    #[test]
    fn constant_calls_evaluate_parameters_conditionals_and_local_constants() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func choose(pos condition: bool, pos left: i32, pos right: i32) -> i32\n",
            "{\n",
            "    const fallback: i32 = right;\n",
            "\n",
            "    if condition\n",
            "    {\n",
            "        return left;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        return fallback;\n",
            "    }\n",
            "}\n",
        ));

        let result_type = i32_type(&compilation);
        let condition_type = bool_type(&compilation);
        let condition = boolean_constant(&compilation, condition_type, false);
        let left = integer_constant(&compilation, result_type, 7);
        let right = integer_constant(&compilation, result_type, 9);

        let (definition, request) =
            source_constant_call_request(&compilation, [condition, left, right], result_type);

        let body_key = compilation
            .callable_body_key(definition)
            .unwrap_or_else(|error| panic!("callable body key must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("source constant callable must have a body"));

        let body = compilation
            .bound_unit(body_key)
            .unwrap_or_else(|error| panic!("constant callable body must bind: {error:?}"));

        assert!(
            constant_callable_root(body.value()).is_some(),
            "unexpected constant callable body: {:#?}",
            body.value()
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .constant_call_with_cancellation(&request, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("constant call must publish: {error:?}"));

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let Some(value) = *result.value() else {
            panic!("constant callable must be eligible for evaluation");
        };

        assert_eq!(integer_value(&compilation, value.value()), 9);
    }

    #[test]
    fn constant_calls_propagate_closed_result_values() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func forward(pos value: Result<i32, i32>) -> Result<i32, i32>\n",
            "{\n",
            "    const unwrapped: i32 = try value;\n",
            "\n",
            "    return value;\n",
            "}\n",
        ));

        let integer_type = i32_type(&compilation);
        let result_type = result_type(&compilation, integer_type, integer_type);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must publish: {error:?}"));

        let result = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(bray_compiler_known::RepresentationRole::Result)
            .unwrap_or_else(|| panic!("Result must be available"));

        let result = symbols
            .union(result)
            .unwrap_or_else(|| panic!("Result symbol must resolve"));

        for error in [false, true] {
            let input = result_value(&compilation, result_type, integer_type, error, 7);

            let (_, request) = source_constant_call_request(&compilation, [input], result_type);

            let checked = compilation
                .constant_call_with_cancellation(&request, &compilation.state.cancellation)
                .unwrap_or_else(|failure| {
                    panic!("constant result propagation must publish: {failure:?}")
                });

            assert!(
                checked.diagnostics().is_empty(),
                "{:?}",
                checked.diagnostics()
            );

            let Some(value) = *checked.value() else {
                panic!("constant callable must be eligible for evaluation");
            };

            let value = compilation
                .semantic_value_store()
                .unwrap_or_else(|failure| panic!("semantic values must publish: {failure:?}"))
                .constant_value_data(value.value());

            let ConstantValueKind::Union { variant, fields } = value.kind() else {
                panic!("constant callable must return a result value");
            };

            assert_eq!(variant, &result.variants()[usize::from(error)]);
            assert_eq!(fields.len(), 1);
        }
    }

    #[test]
    fn constant_call_arguments_map_the_receiver_before_ordinary_parameters() {
        let compilation = compilation("module app;");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let ty = i32_type(&compilation);

        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(70));
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(71));

        let signature = CallableSignature::new(
            ty,
            Some(ReceiverParameterSignature::new(
                receiver,
                ty,
                ReceiverMode::Shared,
            )),
            [CallableParameterSignature::new(parameter, ty)],
            ty,
        );

        let first = integer_constant(&compilation, ty, 1);
        let second = integer_constant(&compilation, ty, 2);

        let parameters = call_parameter_values(
            values,
            &signature,
            &[first, second],
            crate::compilation::SemanticQueryContext::Symbol(receiver.into()),
        )
        .unwrap_or_else(|error| panic!("constant call arguments must map: {error:?}"));

        assert_eq!(parameters.get(&receiver.into()), Some(&first));
        assert_eq!(parameters.get(&parameter.into()), Some(&second));
    }

    #[test]
    fn constant_call_argument_count_failure_retains_call_context_and_counts() {
        let compilation = compilation("module app;");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let ty = i32_type(&compilation);
        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(72));
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(73));

        let signature = CallableSignature::new(
            ty,
            Some(ReceiverParameterSignature::new(
                receiver,
                ty,
                ReceiverMode::Shared,
            )),
            [CallableParameterSignature::new(parameter, ty)],
            ty,
        );

        let argument = integer_constant(&compilation, ty, 1);
        let context = crate::compilation::SemanticQueryContext::Symbol(receiver.into());

        let expected: crate::FactQueryError = crate::compilation::SemanticQueryFailure::contract(
            context.clone(),
            crate::compilation::SemanticQueryViolation::CountMismatch {
                data: crate::compilation::SemanticDataKind::ConstantTerm,
                expected: 2,
                actual: 1,
            },
        )
        .into();

        assert_eq!(
            call_parameter_values(values, &signature, &[argument], context),
            Err(expected)
        );
    }

    #[test]
    fn constant_call_argument_type_failure_retains_parameter_and_types() {
        let compilation = compilation("module app;");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let expected_type = i32_type(&compilation);
        let actual_type = bool_type(&compilation);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(74));

        let signature = CallableSignature::new(
            expected_type,
            None,
            [CallableParameterSignature::new(parameter, expected_type)],
            expected_type,
        );

        let argument = boolean_constant(&compilation, actual_type, true);
        let context = crate::compilation::SemanticQueryContext::Symbol(parameter.into());

        let expected: crate::FactQueryError = crate::compilation::SemanticQueryFailure::contract(
            context.clone(),
            crate::compilation::SemanticQueryViolation::TypeMismatch {
                expected: expected_type,
                actual: actual_type,
            },
        )
        .into();

        assert_eq!(
            call_parameter_values(values, &signature, &[argument], context),
            Err(expected)
        );
    }

    #[test]
    fn malformed_imported_constant_definition_builds_typed_recovery_value() {
        let compilation = compilation("module app;");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let ty = i32_type(&compilation);
        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(75));

        let template_type = values
            .intern_type(TypeData::TypeParameter(parameter))
            .unwrap_or_else(|error| panic!("template type must intern: {error:?}"));

        let owner =
            GenericOwnerId::try_new(FunctionSymbolId::from_symbol_id(SymbolId::new(76)).into())
                .unwrap_or_else(|| panic!("function must own a generic substitution"));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Type(parameter)],
            [GenericArgument::Type(ty)],
        )
        .unwrap_or_else(|error| panic!("generic substitution must validate: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("generic substitution must intern: {error:?}"));

        let result = recovered_error_constant_instance(
            values,
            ErrorConstantDefinition::new(template_type),
            substitution,
            DiagnosticBag::new(),
        )
        .unwrap_or_else(|error| panic!("malformed imported definition must recover: {error:?}"));

        let value = constant_value(&compilation, result.value().value());

        assert_eq!(value.ty(), ty);
        assert_eq!(value.kind(), &ConstantValueKind::Error);
    }

    #[test]
    fn nullable_state_queries_evaluate_as_constants() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const present_value: i32? = 42;\n",
            "const absent_value: i32? = none;\n",
            "const present: bool = present_value.is_present();\n",
            "const absent: bool = absent_value.is_absent();\n",
        ));

        let definitions = source_constant_definitions(&compilation);

        let [present_value, absent_value, present, absent] = definitions.as_slice() else {
            panic!("test source must produce four constant definitions");
        };

        let present_value = compilation
            .constant_instance(instance_key(&compilation, *present_value))
            .unwrap_or_else(|error| panic!("present nullable constant must evaluate: {error:?}"));

        let absent_value = compilation
            .constant_instance(instance_key(&compilation, *absent_value))
            .unwrap_or_else(|error| panic!("absent nullable constant must evaluate: {error:?}"));

        assert!(
            !matches!(
                constant_value(&compilation, present_value.value().value()).kind(),
                ConstantValueKind::NullableAbsent | ConstantValueKind::Error
            ),
            "present nullable constant must retain a present state; diagnostics: {:#?}",
            present_value.diagnostics()
        );

        assert_eq!(
            constant_value(&compilation, absent_value.value().value()).kind(),
            &ConstantValueKind::NullableAbsent
        );

        for definition in [present, absent] {
            let template = compilation
                .constant_definition(*definition)
                .unwrap_or_else(|error| panic!("nullable query template must publish: {error:?}"));

            let result = compilation
                .constant_instance(instance_key(&compilation, *definition))
                .unwrap_or_else(|error| panic!("nullable query constant must evaluate: {error:?}"));

            assert!(
                result.diagnostics().is_empty(),
                "{:#?}",
                result.diagnostics()
            );

            let value = constant_value(&compilation, result.value().value());

            assert_eq!(
                value.kind(),
                &ConstantValueKind::Boolean(true),
                "unexpected template: {:#?}; template diagnostics: {:#?}; instance diagnostics: {:#?}",
                template.value(),
                template.diagnostics(),
                result.diagnostics()
            );
        }
    }

    #[test]
    fn constant_calls_adapt_present_arguments_and_results_to_nullable_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func accepts(pos value: i32?) -> bool\n",
            "{\n",
            "    return value.is_present();\n",
            "}\n",
            "const func returns_present() -> i32?\n",
            "{\n",
            "    return 2;\n",
            "}\n",
            "const argument_query: bool = accepts(1);\n",
            "const returned_value: i32? = returns_present();\n",
            "const returned_query: bool = returned_value.is_present();\n",
        ));

        let definitions = source_constant_definitions(&compilation);

        let [argument_query, returned_value, returned_query] = definitions.as_slice() else {
            panic!("test source must produce three constant definitions");
        };

        for definition in [argument_query, returned_query] {
            let result = compilation
                .constant_instance(instance_key(&compilation, *definition))
                .unwrap_or_else(|error| panic!("nullable query must evaluate: {error:?}"));

            assert!(
                result.diagnostics().is_empty(),
                "{:#?}",
                result.diagnostics()
            );

            assert_eq!(
                constant_value(&compilation, result.value().value()).kind(),
                &ConstantValueKind::Boolean(true)
            );
        }

        let returned_value = compilation
            .constant_instance(instance_key(&compilation, *returned_value))
            .unwrap_or_else(|error| panic!("nullable return value must evaluate: {error:?}"));

        assert!(returned_value.diagnostics().is_empty());

        assert!(matches!(
            constant_value(&compilation, returned_value.value().value()).kind(),
            ConstantValueKind::NullablePresent(_)
        ));
    }

    #[test]
    fn constant_definitions_are_cached_without_closing_instances() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = 1;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let first = compilation
            .constant_definition(*definition)
            .unwrap_or_else(|error| panic!("constant template must publish: {error:?}"));

        let second = compilation
            .constant_definition(*definition)
            .unwrap_or_else(|error| panic!("constant template must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());
        assert!(matches!(first.value(), ConstantDefinitionState::Defined(_)));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *definition)
        ));
    }

    #[test]
    fn symbolic_terms_do_not_evaluate_constant_instances() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const first: i32 = 1;\n",
            "const second: i32 = first;\n",
        ));

        let definitions = source_constant_definitions(&compilation);

        let [first, second] = definitions.as_slice() else {
            panic!("test source must produce two constant definitions");
        };

        let term = compilation
            .symbolic_constant_term(*second)
            .unwrap_or_else(|error| panic!("symbolic constant term must publish: {error:?}"));

        assert!(
            term.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            term.diagnostics()
        );

        let data = compilation
            .semantic_value_store()
            .map(|values| values.constant_term_data(*term.value()))
            .unwrap_or_else(|error| panic!("symbolic constant term must be interned: {error:?}"));

        assert!(matches!(
            data.as_ref(),
            ConstantTermData::DefinitionApplication { .. }
        ));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *first)
        ));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *second)
        ));
    }

    #[test]
    fn constant_instance_cycles_publish_one_deterministic_diagnostic() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = value;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let result = compilation
            .constant_instance(instance_key(&compilation, *definition))
            .unwrap_or_else(|error| panic!("cyclic constant must recover: {error:?}"));

        let repeated = compilation
            .constant_instance(instance_key(&compilation, *definition))
            .unwrap_or_else(|error| panic!("cyclic constant must remain cached: {error:?}"));

        assert!(Arc::ptr_eq(&result, &repeated));
        assert_eq!(result.diagnostics(), repeated.diagnostics());

        assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::CheckingCyclicConstantDefinition,
        );

        let diagnostic = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingCyclicConstantDefinition)
            .next()
            .unwrap_or_else(|| panic!("cyclic constant must publish its diagnostic"));

        assert!(diagnostic.related_locations().iter().any(|location| {
            location.kind() == DiagnosticRelatedLocationKind::FirstDeclaration
                && Some(location.span()) != diagnostic.primary_span()
        }));

        assert!(matches!(
            constant_value(&compilation, result.value().value()).kind(),
            ConstantValueKind::Error
        ));
    }

    #[test]
    fn generic_constant_instances_are_separated_by_concrete_substitution() {
        let compilation = Arc::new(compilation(concat!(
            "module app;\n",
            "struct Buffer<const count: i32>\n",
            "{\n",
            "    const size: i32 = count;\n",
            "}\n",
        )));

        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"));

        let structure = symbols
            .structures()
            .iter()
            .find(|structure| structure.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must produce one structure"));

        let [parameter] = structure.generic_const_parameters() else {
            panic!("test structure must produce one const parameter");
        };

        let ty = i32_type(&compilation);

        let first = concrete_substitution(&compilation, structure.id().into(), *parameter, ty, 3);
        let second = concrete_substitution(&compilation, structure.id().into(), *parameter, ty, 7);

        let first_key = ConstantInstanceKey::new(*definition, first, None);
        let second_key = ConstantInstanceKey::new(*definition, second, None);

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        for instance in [first_key, second_key] {
            compilation
                .state
                .constant_instances
                .cell(instance_semantic_key(&compilation, instance))
                .unwrap_or_else(|error| panic!("constant instance cell must exist: {error:?}"))
                .set_test_observer(gate.observer())
                .unwrap_or_else(|error| {
                    panic!("constant instance observer must attach: {error:?}")
                });
        }

        let (first, second) = std::thread::scope(|scope| {
            let first_compilation = Arc::clone(&compilation);
            let first = scope.spawn(move || first_compilation.constant_instance(first_key));

            let second_compilation = Arc::clone(&compilation);
            let second = scope.spawn(move || second_compilation.constant_instance(second_key));

            gate.wait_until_observed(FactCellTestEvent::Computing, 2);
            gate.release();

            (
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first constant worker must not panic")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second constant worker must not panic")),
            )
        });

        let first =
            first.unwrap_or_else(|error| panic!("first generic instance must publish: {error:?}"));

        let second = second
            .unwrap_or_else(|error| panic!("second generic instance must publish: {error:?}"));

        assert!(
            first.diagnostics().is_empty(),
            "unexpected first diagnostics: {:?}",
            first.diagnostics()
        );

        assert!(
            second.diagnostics().is_empty(),
            "unexpected second diagnostics: {:?}",
            second.diagnostics()
        );

        assert_eq!(integer_value(&compilation, first.value().value()), 3);
        assert_eq!(integer_value(&compilation, second.value().value()), 7);
        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn constant_instance_semantic_keys_include_the_complete_target_profile() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = 1;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let instance = instance_key(&compilation, *definition);
        let baseline = SelectedTarget::baseline();

        let alternate_identity = TargetIdentity::try_new("alternate-test-target")
            .unwrap_or_else(|| panic!("alternate target identity must be valid"));

        let alternate = TargetProfile::try_new(
            alternate_identity,
            baseline.profile().machine().clone(),
            baseline.profile().properties().clone(),
        )
        .unwrap_or_else(|error| panic!("alternate target profile must be valid: {error:?}"));

        assert_ne!(
            ConstantInstanceQueryKey::new(
                instance,
                baseline.profile().clone(),
                ConstantEvaluationLimits::default(),
            ),
            ConstantInstanceQueryKey::new(instance, alternate, ConstantEvaluationLimits::default(),)
        );
    }

    fn source_constant_definitions(
        compilation: &crate::Compilation,
    ) -> Vec<AnyConstantDefinitionId> {
        compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"))
            .constants()
            .iter()
            .filter(|constant| constant.origin() == SymbolOrigin::Source)
            .map(|constant| AnyConstantDefinitionId::Constant(constant.id()))
            .collect()
    }

    fn instance_key(
        compilation: &crate::Compilation,
        definition: AnyConstantDefinitionId,
    ) -> ConstantInstanceKey {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let substitution =
            crate::compilation::substitution::empty_substitution(values, definition.into_any())
                .unwrap_or_else(|error| panic!("empty substitution must be concrete: {error:?}"));

        ConstantInstanceKey::new(definition, substitution, None)
    }

    fn instance_semantic_key(
        compilation: &crate::Compilation,
        instance: ConstantInstanceKey,
    ) -> ConstantInstanceQueryKey {
        ConstantInstanceQueryKey::new(
            instance,
            compilation.requested_target().profile().clone(),
            ConstantEvaluationLimits::default(),
        )
    }

    fn instance_is_published(
        compilation: &crate::Compilation,
        instance: ConstantInstanceKey,
    ) -> bool {
        compilation
            .state
            .constant_instances
            .is_published(&instance_semantic_key(compilation, instance))
            .unwrap_or_else(|error| panic!("constant cache must be readable: {error:?}"))
    }

    fn concrete_substitution(
        compilation: &crate::Compilation,
        owner: bray_symbols::AnySymbolId,
        parameter: bray_symbols::GenericConstParameterSymbolId,
        ty: bray_symbols::TypeId,
        value: u8,
    ) -> bray_symbols::GenericSubstitutionId {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let value = integer_constant(compilation, ty, value);

        let term = values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("constant argument term must be interned: {error:?}"));

        let owner = GenericOwnerId::try_new(owner)
            .unwrap_or_else(|| panic!("test generic owner must accept parameters"));

        let data = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Const(parameter)],
            [GenericArgument::Constant(term)],
        )
        .unwrap_or_else(|error| panic!("generic substitution must be valid: {error:?}"));

        let substitution = values
            .intern_generic_substitution(data)
            .unwrap_or_else(|error| panic!("generic substitution must be interned: {error:?}"));

        substitution
    }

    fn source_constant_call_request(
        compilation: &crate::Compilation,
        arguments: impl IntoIterator<Item = ConstantValueId>,
        result_type: bray_symbols::TypeId,
    ) -> (CallableDefinitionId, ConstantCallRequest) {
        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"));

        let function = symbols
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must produce one function"));

        let definition = CallableDefinitionId::try_new(function.id().into())
            .unwrap_or_else(|| panic!("source function must be callable"));

        let substitution = GenericSubstitutionData::try_new(
            GenericOwnerId::try_new(function.id().into())
                .unwrap_or_else(|| panic!("source function must own a substitution")),
            [],
            [],
        )
        .unwrap_or_else(|error| panic!("empty function substitution must validate: {error:?}"));

        let substitution = compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .intern_generic_substitution(substitution)
                    .map_err(crate::FactQueryError::SemanticValueStore)
            })
            .unwrap_or_else(|error| panic!("empty function substitution must intern: {error:?}"));

        let request = ConstantCallRequest::new(
            CallableInstanceData::new(definition, substitution),
            None,
            arguments,
            result_type,
            ConstantEvaluationLimits::default(),
        );

        (definition, request)
    }

    fn i32_type(compilation: &crate::Compilation) -> bray_symbols::TypeId {
        scalar_type(
            compilation,
            bray_compiler_known::RepresentationRole::ScalarI32,
        )
    }

    fn bool_type(compilation: &crate::Compilation) -> bray_symbols::TypeId {
        scalar_type(
            compilation,
            bray_compiler_known::RepresentationRole::ScalarBool,
        )
    }

    fn result_type(
        compilation: &crate::Compilation,
        success: bray_symbols::TypeId,
        error: bray_symbols::TypeId,
    ) -> bray_symbols::TypeId {
        let definition = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(bray_compiler_known::RepresentationRole::Result)
            .unwrap_or_else(|| panic!("Result representation must be available"));

        let symbol = compilation
            .symbol_graph()
            .unwrap_or_else(|failure| panic!("symbol graph must publish: {failure:?}"))
            .union(definition)
            .unwrap_or_else(|| panic!("Result symbol must resolve"));

        let parameters = symbol
            .generic_type_parameters()
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type);

        let substitution = GenericSubstitutionData::try_new(
            GenericOwnerId::try_new(definition.into())
                .unwrap_or_else(|| panic!("Result must own generic parameters")),
            parameters,
            [GenericArgument::Type(success), GenericArgument::Type(error)],
        )
        .unwrap_or_else(|failure| panic!("Result substitution must be valid: {failure:?}"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|failure| panic!("semantic values must publish: {failure:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|failure| panic!("Result substitution must intern: {failure:?}"));

        values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Union(definition),
                substitution,
            })
            .unwrap_or_else(|failure| panic!("Result type must intern: {failure:?}"))
    }

    fn result_value(
        compilation: &crate::Compilation,
        result_type: bray_symbols::TypeId,
        payload_type: bray_symbols::TypeId,
        error: bool,
        payload: u8,
    ) -> ConstantValueId {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|failure| panic!("semantic values must publish: {failure:?}"));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|failure| panic!("symbol graph must publish: {failure:?}"));

        let result = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(bray_compiler_known::RepresentationRole::Result)
            .unwrap_or_else(|| panic!("Result representation must be available"));

        let result = symbols
            .union(result)
            .unwrap_or_else(|| panic!("Result symbol must resolve"));

        let variant = result.variants()[usize::from(error)];

        let variant_symbol = symbols
            .union_variant(variant)
            .unwrap_or_else(|| panic!("Result variant must resolve"));

        let [field] = variant_symbol.payload_fields() else {
            panic!("Result variant must have one payload field");
        };

        let payload = integer_constant(compilation, payload_type, payload);

        values
            .intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::union(variant, [ConstantField::new(*field, payload)]),
            ))
            .unwrap_or_else(|failure| panic!("Result value must intern: {failure:?}"))
    }

    fn scalar_type(
        compilation: &crate::Compilation,
        role: bray_compiler_known::RepresentationRole,
    ) -> bray_symbols::TypeId {
        let definition = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(role)
            .unwrap_or_else(|| panic!("scalar representation must be available: {role:?}"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let owner = GenericOwnerId::try_new(definition.into())
            .unwrap_or_else(|| panic!("i32 must be a generic owner"));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        )
        .unwrap_or_else(|error| panic!("i32 substitution must be valid: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("i32 substitution must be interned: {error:?}"));

        values
            .intern_type(bray_symbols::TypeData::Named {
                definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("i32 type must be interned: {error:?}"))
    }

    fn integer_constant(
        compilation: &crate::Compilation,
        ty: bray_symbols::TypeId,
        value: u8,
    ) -> ConstantValueId {
        compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .intern_constant_value(ConstantValueData::new(
                        ty,
                        ConstantValueKind::Integer(IntegerConstant::new(
                            IntegerSign::NonNegative,
                            [value],
                        )),
                    ))
                    .map_err(crate::FactQueryError::SemanticValueStore)
            })
            .unwrap_or_else(|error| panic!("constant value must be interned: {error:?}"))
    }

    fn boolean_constant(
        compilation: &crate::Compilation,
        ty: bray_symbols::TypeId,
        value: bool,
    ) -> ConstantValueId {
        compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .intern_constant_value(ConstantValueData::new(
                        ty,
                        ConstantValueKind::Boolean(value),
                    ))
                    .map_err(crate::FactQueryError::SemanticValueStore)
            })
            .unwrap_or_else(|error| panic!("constant value must be interned: {error:?}"))
    }

    fn integer_value(compilation: &crate::Compilation, value: ConstantValueId) -> u8 {
        let value = constant_value(compilation, value);

        let ConstantValueKind::Integer(integer) = value.kind() else {
            panic!("constant value must be an integer: {value:?}");
        };

        let [value] = integer.magnitude() else {
            panic!("test integer must fit in one byte");
        };

        *value
    }

    fn constant_value(
        compilation: &crate::Compilation,
        value: ConstantValueId,
    ) -> Arc<ConstantValueData> {
        compilation
            .semantic_value_store()
            .map(|values| values.constant_value_data(value))
            .unwrap_or_else(|error| panic!("constant value must be interned: {error:?}"))
    }
}
