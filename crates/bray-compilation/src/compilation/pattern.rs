use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundExpression, BoundExpressionId, BoundPatternId,
    BoundPatternTarget, BoundReferenceTarget, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
    CheckedExpressionTypes, CheckedSemanticSelections, walk_bound_unit_view,
};
use bray_checker::{
    CheckerOutcome, CheckerUnitView, ConstantEvaluationInput, ConstantEvaluator,
    DefaultConstantEvaluator, GuardConstantEvidence, PatternCheckInput, PatternConstantEvidence,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, ConstantDefinitionState, ConstantInstanceKey, ConstantTermData,
    ConstantTermId, ConstantValueId, ConstantValueKind, LocalConstantSymbolId, TypeId,
};

use super::Compilation;
use super::binder::has_visible_generic_parameters;
use super::checker::CompilationCheckerContext;
use super::constant::{
    CompilationConstantCallResolver, collect_constant_references_from, constant_definition_id,
    empty_concrete_substitution,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn add_constant_pattern_evidence(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        input: PatternCheckInput,
        cancellation: &CancellationToken,
    ) -> Result<(PatternCheckInput, DiagnosticBag), FactQueryError> {
        let sites = collect_constant_sites(request)?;
        let mut patterns = Vec::new();
        let mut guards = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for (pattern, target) in sites.patterns {
            if cancellation.is_cancelled() {
                return Err(FactQueryError::Cancelled);
            }

            let constant = match target {
                BoundPatternTarget::Surface(symbol) => {
                    self.surface_pattern_constant(symbol, cancellation, &mut diagnostics)?
                }
                BoundPatternTarget::Local(AnyLocalSymbolId::Constant(local)) => {
                    let Some(initializer) = sites.local_constants.get(&local).copied() else {
                        continue;
                    };

                    self.local_pattern_constant(
                        request,
                        types,
                        selections,
                        initializer,
                        cancellation,
                    )?
                }
                BoundPatternTarget::Local(_) => None,
            };

            if let Some((ty, term)) = constant {
                patterns.push(PatternConstantEvidence::new(pattern, ty, term));
            }
        }

        for guard in sites.guards {
            if cancellation.is_cancelled() {
                return Err(FactQueryError::Cancelled);
            }

            if let Some(value) =
                self.evaluate_closed_expression(request, types, selections, guard, cancellation)?
            {
                guards.push(GuardConstantEvidence::new(guard, value));
            }
        }

        Ok((
            input
                .with_constant_patterns(patterns)
                .with_constant_guards(guards),
            diagnostics,
        ))
    }

    fn surface_pattern_constant(
        &self,
        symbol: AnySymbolId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<(TypeId, ConstantTermId)>, FactQueryError> {
        let Some(definition) = constant_definition_id(symbol) else {
            return Ok(None);
        };

        let definition_result =
            self.constant_definition_with_cancellation(definition, cancellation)?;

        *diagnostics = diagnostics.merged(definition_result.diagnostics());

        let ConstantDefinitionState::Defined(definition_data) = definition_result.value() else {
            return Ok(None);
        };

        let mut ty = definition_data.ty();
        let mut term = definition_data.term();

        if !constant_can_evaluate_without_context(self, symbol, definition)? {
            return Ok(Some((ty, term)));
        }

        let substitution = empty_concrete_substitution(self.semantic_value_store()?, definition)?;

        let instance = ConstantInstanceKey::new(definition, substitution, None);

        match self.constant_instance_with_cancellation(instance, cancellation) {
            Ok(result) => {
                *diagnostics = diagnostics.merged(result.diagnostics());

                let value = *result.value();

                let data = self
                    .semantic_value_store()?
                    .constant_value_data(value)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                ty = data.ty();
                term = self.constant_value_term(value)?;
            }
            Err(FactQueryError::Cancelled) => return Err(FactQueryError::Cancelled),
            Err(FactQueryError::Cycle(_)) => {}
            Err(error) => return Err(error),
        }

        Ok(Some((ty, term)))
    }

    fn local_pattern_constant(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        initializer: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Option<(TypeId, ConstantTermId)>, FactQueryError> {
        let Some(value) =
            self.evaluate_closed_expression(request, types, selections, initializer, cancellation)?
        else {
            return Ok(None);
        };

        let data = self
            .semantic_value_store()?
            .constant_value_data(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(Some((data.ty(), self.constant_value_term(value)?)))
    }

    fn constant_value_term(
        &self,
        value: ConstantValueId,
    ) -> Result<ConstantTermId, FactQueryError> {
        self.semantic_value_store()?
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    fn evaluate_closed_expression(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        expression: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConstantValueId>, FactQueryError> {
        let mut dependency_diagnostics = DiagnosticBag::new();

        let references = match collect_constant_references_from(
            request.unit(),
            selections,
            expression,
            |_, target| {
                let BoundReferenceTarget::Surface(symbol) = target else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let Some(definition) = constant_definition_id(symbol) else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                if !constant_can_evaluate_without_context(self, symbol, definition)? {
                    return Err(FactQueryError::InfrastructureFailure);
                }

                let substitution =
                    empty_concrete_substitution(self.semantic_value_store()?, definition)?;

                let instance = ConstantInstanceKey::new(definition, substitution, None);
                let result = self.constant_instance_with_cancellation(instance, cancellation)?;

                dependency_diagnostics = dependency_diagnostics.merged(result.diagnostics());

                Ok(bray_checker::ConstantReferenceResolution::Value(
                    *result.value(),
                ))
            },
        ) {
            Ok(references) => references,
            Err(FactQueryError::Cancelled) => return Err(FactQueryError::Cancelled),
            Err(_) => return Ok(None),
        };

        if dependency_diagnostics.has_errors() {
            return Ok(None);
        }

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(types, selections)
            .with_root(expression)
            .with_references(references)
            .with_call_resolver(&resolver);

        let result = match DefaultConstantEvaluator.evaluate_constant(request, &input) {
            CheckerOutcome::Complete(result) => result,
            CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
            CheckerOutcome::InfrastructureFailure(_) => return Ok(None),
        };

        if result.diagnostics().has_errors() {
            return Ok(None);
        }

        let value = *result.value();

        let data = self
            .semantic_value_store()?
            .constant_value_data(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok((!matches!(data.kind(), ConstantValueKind::Error)).then_some(value))
    }
}

struct ConstantPatternSites {
    patterns: Vec<(BoundPatternId, BoundPatternTarget)>,
    guards: BTreeSet<BoundExpressionId>,
    local_constants: BTreeMap<LocalConstantSymbolId, BoundExpressionId>,
}

fn collect_constant_sites(
    request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
) -> Result<ConstantPatternSites, FactQueryError> {
    let mut patterns = Vec::new();
    let mut guards = BTreeSet::new();
    let mut local_constants = BTreeMap::new();

    let outcome = walk_bound_unit_view(request.view(), request.unit().root(), |event| {
        let BoundWalkEvent::Enter(node) = event else {
            return BoundWalkControl::Continue;
        };

        match node {
            AnyBoundNodeId::Pattern(pattern) => {
                let Some(bound) = request.view().pattern(pattern) else {
                    return BoundWalkControl::Stop;
                };

                if let Some(target) = bound.target()
                    && target.is_constant()
                {
                    patterns.push((pattern, target));
                }
            }
            AnyBoundNodeId::Expression(expression) => {
                let Some(BoundExpression::Match(matched)) = request.view().expression(expression)
                else {
                    return BoundWalkControl::Continue;
                };

                guards.extend(matched.arms().iter().filter_map(|arm| arm.guard()));
            }
            AnyBoundNodeId::Block(block) => {
                let Some(block) = request.view().block(block) else {
                    return BoundWalkControl::Stop;
                };

                for item in block.items() {
                    let BoundBlockItem::LocalConstant(local) = item else {
                        continue;
                    };

                    if let Some(symbol) = local.symbol() {
                        local_constants.insert(symbol, local.initializer());
                    }
                }
            }
            AnyBoundNodeId::CallableBody(_) => {}
        }

        BoundWalkControl::Continue
    });

    match outcome {
        BoundWalkOutcome::Completed => Ok(ConstantPatternSites {
            patterns,
            guards,
            local_constants,
        }),
        BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
            Err(FactQueryError::InfrastructureFailure)
        }
    }
}

fn constant_can_evaluate_without_context(
    compilation: &Compilation,
    symbol: AnySymbolId,
    definition: bray_symbols::AnyConstantDefinitionId,
) -> Result<bool, FactQueryError> {
    let symbols = compilation.symbol_graph()?;

    Ok(!matches!(
        definition,
        bray_symbols::AnyConstantDefinitionId::TraitMember(_)
    ) && !has_visible_generic_parameters(symbols, symbol))
}
