use bray_bound_tree::{BoundExpressionId, BoundPatternId};
use bray_symbols::{ConstantTermId, TypeId};

use crate::constant::pattern::{PatternTermError, PatternTerms};
use crate::{CheckerConstantEvaluationFailure, CheckerInfrastructureError, CheckerRequestContext};

use super::engine::Evaluator;
use super::support::EvaluationFailure;

impl<'view, 'input, 'types, C: CheckerRequestContext + ?Sized> Evaluator<'view, 'input, 'types, C> {
    pub(super) fn symbolic_pattern_test(
        &mut self,
        owner: BoundExpressionId,
        pattern: BoundPatternId,
        subject: ConstantTermId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let patterns = self.input.patterns().ok_or(EvaluationFailure::constant(
            CheckerConstantEvaluationFailure::MissingPatternInput { pattern },
        ))?;

        let terms = PatternTerms {
            view: self.request.view(),
            patterns,
            values: self.request.semantic_values(),
            boolean: ty,
            retain_types: self.input.retains_nested_term_types(),
        };

        let request = self.request;
        let budget = &mut self.budget;

        terms
            .test(pattern, subject, || {
                if request.is_cancelled() {
                    return Err(EvaluationFailure::Cancelled);
                }

                budget.charge_step(owner)
            })
            .map_err(|error| match error {
                PatternTermError::Step(error) => error,
                PatternTermError::Semantic(error) => EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueStore(error),
                ),
                PatternTermError::MissingPattern(pattern) => {
                    EvaluationFailure::constant(CheckerConstantEvaluationFailure::MissingPattern {
                        pattern,
                    })
                }
                PatternTermError::MissingCheckedPattern(pattern) => {
                    EvaluationFailure::constant(CheckerConstantEvaluationFailure::MissingPattern {
                        pattern,
                    })
                }
                PatternTermError::Unsupported => EvaluationFailure::invalid_expression(owner),
            })
    }
}
