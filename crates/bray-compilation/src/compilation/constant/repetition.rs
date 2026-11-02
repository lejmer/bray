use std::collections::{BTreeMap, BTreeSet};

use bray_binder::semantic_unit_context;
use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, BoundUnitKey, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedExpressionSemantics, walk_bound_unit_view,
};
use bray_checker::{
    CheckerRequestContext, CheckerUnitView, ConstantChecker, ConstantEvaluationInput,
    DefaultConstantChecker,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnyLocalSymbolId, ConstantTermId};

use super::CompilationConstantCallResolver;
use crate::compilation::Compilation;
use crate::compilation::checker::checker_result;
use crate::fact::{CancellationToken, FactQueryError};

type RepeatedArrayLengths = Vec<(BoundExpressionId, ConstantTermId)>;

impl Compilation {
    pub(in crate::compilation) fn repeated_array_lengths(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<Option<(RepeatedArrayLengths, DiagnosticBag)>, FactQueryError> {
        let repetitions = bound
            .tree()
            .expressions()
            .filter_map(|(id, expression)| {
                let BoundExpression::Structured(expression) = expression else {
                    return None;
                };

                if expression.kind() != BoundStructuredExpressionKind::RepeatedArray {
                    return None;
                }

                let [_, count] = expression.operands() else {
                    return None;
                };

                Some((id, *count))
            })
            .collect::<Vec<_>>();

        if repetitions.is_empty() {
            return Ok(None);
        }

        let context = self.checker_context_for(key, cancellation)?;
        let semantic_context = semantic_unit_context(context.symbols(), bound);
        let request = CheckerUnitView::new(bound, &semantic_context, &context);
        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let initializers = bound
            .tree()
            .blocks()
            .flat_map(|(_, block)| block.items())
            .filter_map(|item| {
                let BoundBlockItem::LocalConstant(local) = item else {
                    return None;
                };

                Some((AnyLocalSymbolId::from(local.symbol()?), local.initializer()))
            })
            .collect::<BTreeMap<_, _>>();

        let mut pending = repetitions
            .iter()
            .map(|(_, count)| *count)
            .collect::<Vec<_>>();

        let mut required = BTreeSet::new();

        while let Some(root) = pending.pop() {
            cancellation.check()?;

            let outcome = walk_bound_unit_view(bound.view(), root, |event| {
                if let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event
                    && let Some(BoundExpression::Name(name)) = bound.view().expression(id)
                    && let BoundReferenceTarget::Local(local @ AnyLocalSymbolId::Constant(_)) =
                        name.target()
                    && required.insert(local)
                {
                    pending.push(
                        *initializers
                            .get(&local)
                            .expect("local constant retains its initializer"),
                    );
                }

                BoundWalkControl::Continue
            });

            assert_eq!(
                outcome,
                BoundWalkOutcome::Completed,
                "bound count dependencies form a complete tree"
            );
        }

        let mut input = ConstantEvaluationInput::new(semantics.types(), semantics.selections())
            .with_call_resolver(&resolver);

        let mut diagnostics = DiagnosticBag::new();

        // Local identities follow declaration order. Binding excludes forward local references.
        for local in required {
            let initializer = initializers[&local];

            if semantics
                .types()
                .expression(initializer)
                .is_none_or(|ty| ty.is_recovered())
            {
                continue;
            }

            input = input.with_root(initializer)
                .with_references(self.symbolic_references(
                    bound,
                    semantics.selections(),
                    initializer,
                )?);

            let result =
                checker_result(DefaultConstantChecker.check_constant_term(request, &input))?;

            if !result.diagnostics().has_errors() {
                input = input.with_local_terms([(local, *result.value())]);
            }

            diagnostics.extend(result.diagnostics().iter().cloned());
        }

        let mut lengths = Vec::new();

        for (expression, count) in repetitions {
            cancellation.check()?;

            if semantics
                .types()
                .expression(count)
                .is_none_or(|ty| ty.is_recovered())
            {
                continue;
            }

            input = input.with_root(count)
                .with_references(self.symbolic_references(bound, semantics.selections(), count)?);

            let result =
                checker_result(DefaultConstantChecker.check_constant_term(request, &input))?;

            let term = *result.value();
            let failed = result.diagnostics().has_errors();

            diagnostics.extend(result.diagnostics().iter().cloned());

            if failed {
                continue;
            }

            let normalized = context.normalize_array_length(term)?;

            if !normalized.diagnostics().has_errors() {
                lengths.push((expression, *normalized.value()));
            }

            diagnostics.extend(normalized.diagnostics().iter().cloned());
        }

        Ok(Some((lengths, diagnostics)))
    }
}
