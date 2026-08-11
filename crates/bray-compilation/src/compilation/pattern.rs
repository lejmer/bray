use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundExpression, BoundExpressionId, BoundPatternId,
    BoundPatternTarget, BoundReferenceTarget, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
    CheckedExpressionTypes, CheckedSemanticSelections, walk_bound_unit_view,
};
use bray_checker::{
    CheckerOutcome, CheckerUnitView, ConstantEvaluationInput, ConstantEvaluator,
    ConstantReferenceResolution, DefaultConstantEvaluator, GuardConstantEvidence,
    PatternCheckInput, PatternConstantEvidence,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, ConstantTermData, ConstantTermId, ConstantValueId,
    ConstantValueKind, LocalConstantSymbolId, TypeId,
};

use super::Compilation;
use super::checker::CompilationCheckerContext;
use super::constant::{CompilationConstantCallResolver, collect_constant_references_from};
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
        let mut local_constants = LocalPatternConstants::new(sites.local_constants);
        let mut diagnostics = DiagnosticBag::new();

        for (pattern, target) in sites.patterns {
            if cancellation.is_cancelled() {
                return Err(FactQueryError::Cancelled);
            }

            let constant = match target {
                BoundPatternTarget::Surface(symbol) => {
                    self.surface_pattern_constant(symbol, cancellation, &mut diagnostics)?
                }
                BoundPatternTarget::Local(AnyLocalSymbolId::Constant(local)) => self
                    .local_pattern_constant(
                        request,
                        types,
                        selections,
                        local,
                        &mut local_constants,
                        cancellation,
                    )?,
                BoundPatternTarget::Local(_) => None,
            };

            if let Some((ty, term)) = constant {
                patterns.push(PatternConstantEvidence::new(pattern, ty, term));
            }
        }

        if !sites.guards.is_empty() {
            self.evaluate_local_constants(
                request,
                types,
                selections,
                &mut local_constants,
                cancellation,
            )?;
        }

        for guard in sites.guards {
            if cancellation.is_cancelled() {
                return Err(FactQueryError::Cancelled);
            }

            if let Some(value) = self.evaluate_closed_expression(
                request,
                types,
                selections,
                guard,
                &local_constants.values,
                cancellation,
            )? {
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
        let Some((ty, resolution)) =
            self.resolve_surface_constant(symbol, cancellation, diagnostics)?
        else {
            return Ok(None);
        };

        let term = match resolution {
            ConstantReferenceResolution::Value(value) => self.constant_value_term(value)?,
            ConstantReferenceResolution::Evaluated(result) => {
                self.constant_value_term(result.value())?
            }
            ConstantReferenceResolution::Term(term) => term,
            ConstantReferenceResolution::Cycle { .. } | ConstantReferenceResolution::Invalid => {
                return Ok(None);
            }
        };

        Ok(Some((ty, term)))
    }

    fn local_pattern_constant(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        local: LocalConstantSymbolId,
        constants: &mut LocalPatternConstants,
        cancellation: &CancellationToken,
    ) -> Result<Option<(TypeId, ConstantTermId)>, FactQueryError> {
        self.evaluate_local_constants_through(
            request,
            types,
            selections,
            local,
            constants,
            cancellation,
        )?;

        Ok(constants.values.get(&local).copied())
    }

    fn evaluate_local_constants(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        constants: &mut LocalPatternConstants,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        let Some(last) = constants
            .initializers
            .last_key_value()
            .map(|(local, _)| *local)
        else {
            return Ok(());
        };

        self.evaluate_local_constants_through(
            request,
            types,
            selections,
            last,
            constants,
            cancellation,
        )
    }

    fn evaluate_local_constants_through(
        &self,
        request: CheckerUnitView<'_, CompilationCheckerContext<'_>>,
        types: &CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        last: LocalConstantSymbolId,
        constants: &mut LocalPatternConstants,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        let pending = constants
            .initializers
            .range(..=last)
            .filter(|(local, _)| !constants.values.contains_key(local))
            .map(|(local, initializer)| (*local, *initializer))
            .collect::<Vec<_>>();

        for (local, initializer) in pending {
            let Some(value) = self.evaluate_closed_expression(
                request,
                types,
                selections,
                initializer,
                &constants.values,
                cancellation,
            )?
            else {
                continue;
            };

            let data = self
                .semantic_value_store()?
                .constant_value_data(value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            constants
                .values
                .insert(local, (data.ty(), self.constant_value_term(value)?));
        }

        Ok(())
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
        local_constants: &BTreeMap<LocalConstantSymbolId, (TypeId, ConstantTermId)>,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConstantValueId>, FactQueryError> {
        let mut dependency_diagnostics = DiagnosticBag::new();

        let references = collect_constant_references_from(
            request.unit(),
            selections,
            expression,
            |_, target| {
                let BoundReferenceTarget::Surface(symbol) = target else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                if let AnySymbolId::GenericConstParameter(parameter) = symbol {
                    let term = self
                        .semantic_value_store()?
                        .intern_constant_term(ConstantTermData::Parameter(parameter))
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    return Ok(ConstantReferenceResolution::Term(term));
                }

                let Some((_, resolution)) = self.resolve_surface_constant(
                    symbol,
                    cancellation,
                    &mut dependency_diagnostics,
                )?
                else {
                    return Ok(ConstantReferenceResolution::Invalid);
                };

                Ok(resolution)
            },
        )?;

        if dependency_diagnostics.has_errors() {
            return Ok(None);
        }

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(types, selections)
            .with_root(expression)
            .with_references(references)
            .with_local_terms(
                local_constants
                    .iter()
                    .map(|(local, (_, term))| (AnyLocalSymbolId::from(*local), *term)),
            )
            .with_call_resolver(&resolver);

        let result = match DefaultConstantEvaluator.evaluate_constant(request, &input) {
            CheckerOutcome::Complete(result) => result,
            CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
            CheckerOutcome::InfrastructureFailure(error) => {
                return Err(FactQueryError::CheckerInfrastructure(error));
            }
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

struct LocalPatternConstants {
    initializers: BTreeMap<LocalConstantSymbolId, BoundExpressionId>,
    values: BTreeMap<LocalConstantSymbolId, (TypeId, ConstantTermId)>,
}

impl LocalPatternConstants {
    fn new(initializers: BTreeMap<LocalConstantSymbolId, BoundExpressionId>) -> Self {
        Self {
            initializers,
            values: BTreeMap::new(),
        }
    }
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
