use bray_symbols::{ConstantTermData, ConstantTermId, ConstantTest, SemanticValueStoreError};

use crate::constant::pattern::{PatternTermError, PatternTerms};
use crate::contract::MAX_CONDITION_STEPS;

use super::super::model::AnalysisRefinement;
use super::flow::{DomainState, GuaranteeDomain};

impl GuaranteeDomain<'_> {
    pub(super) fn refinement_condition(
        &self,
        state: &DomainState,
        refinement: Option<AnalysisRefinement>,
    ) -> Result<Option<(ConstantTermId, bool)>, SemanticValueStoreError> {
        match refinement {
            Some(AnalysisRefinement::ResultOutcome {
                expression,
                is_success,
            }) => {
                let Some(representation) = self.result_representation else {
                    return Ok(None);
                };

                let Some(subject) = self.propagation_subject(state, expression)? else {
                    return Ok(None);
                };

                let variant = if is_success {
                    representation.success_variant()
                } else {
                    representation.error_variant()
                };

                let condition = self.values.intern_constant_term(ConstantTermData::Test {
                    subject,
                    kind: ConstantTest::ActiveUnionVariant(variant),
                })?;

                Ok(Some((condition, true)))
            }
            Some(AnalysisRefinement::Condition { expression, value }) => Ok(self
                .current_expression(state, expression)?
                .map(|condition| (condition, value))),
            Some(AnalysisRefinement::NullablePresence {
                expression,
                is_present,
            }) => {
                let Some(subject) = self.current_expression(state, expression)? else {
                    return Ok(None);
                };

                let condition = self.values.intern_constant_term(ConstantTermData::Test {
                    subject,
                    kind: ConstantTest::NullablePresent,
                })?;

                Ok(Some((condition, is_present)))
            }
            Some(AnalysisRefinement::PatternOutcome {
                subject,
                pattern,
                value,
            }) => {
                let Some(subject) = self.current_expression(state, subject)? else {
                    return Ok(None);
                };

                let terms = PatternTerms {
                    view: self.view,
                    patterns: self.patterns,
                    values: self.values,
                    boolean: self.boolean,
                    retain_types: false,
                };

                let mut remaining = MAX_CONDITION_STEPS;

                let condition = terms.test(pattern, subject, || {
                    remaining = remaining.checked_sub(1).ok_or(())?;

                    Ok(())
                });

                match condition {
                    Ok(condition) => Ok(Some((condition, value))),
                    Err(PatternTermError::Semantic(error)) => Err(error),
                    Err(
                        PatternTermError::Step(())
                        | PatternTermError::MissingPattern(_)
                        | PatternTermError::MissingCheckedPattern(_)
                        | PatternTermError::Unsupported,
                    ) => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }
}
