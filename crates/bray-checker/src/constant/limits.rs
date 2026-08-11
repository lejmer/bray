use bray_bound_tree::BoundExpressionId;

use super::diagnostic::{ConstantDiagnostic, ConstantLimitKind};
use super::{ConstantEvaluationInput, evaluation::EvaluationFailure};

/// Deterministic resource limits for one constant-evaluation request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantEvaluationLimits {
    steps: u64,
    aggregate_elements: u64,
    literal_bytes: u64,
    expansions: u64,
    integer_bits: u32,
    call_depth: u32,
}

/// Deterministic cumulative work consumed by one constant evaluation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantEvaluationUsage {
    steps: u64,
    aggregate_elements: u64,
    literal_bytes: u64,
    expansions: u64,
}

impl ConstantEvaluationUsage {
    /// Creates an explicit cumulative usage summary.
    pub const fn new(steps: u64, aggregate_elements: u64, literal_bytes: u64) -> Self {
        Self {
            steps,
            aggregate_elements,
            literal_bytes,
            expansions: 0,
        }
    }

    /// Uses an explicit number of expanded constant elements.
    pub const fn with_expansions(mut self, expansions: u64) -> Self {
        self.expansions = expansions;

        self
    }

    /// Returns the number of evaluated expression operations.
    pub const fn steps(self) -> u64 {
        self.steps
    }

    /// Returns the number of aggregate elements materialized.
    pub const fn aggregate_elements(self) -> u64 {
        self.aggregate_elements
    }

    /// Returns the number of source literal bytes decoded.
    pub const fn literal_bytes(self) -> u64 {
        self.literal_bytes
    }

    /// Returns the number of elements produced through constant expansion.
    pub const fn expansions(self) -> u64 {
        self.expansions
    }
}

impl ConstantEvaluationLimits {
    /// Creates explicit limits for operations, aggregate elements, and source literal bytes.
    pub const fn new(steps: u64, aggregate_elements: u64, literal_bytes: u64) -> Self {
        Self {
            steps,
            aggregate_elements,
            literal_bytes,
            expansions: aggregate_elements,
            integer_bits: 16 * 1024 * 1024,
            call_depth: 1_024,
        }
    }

    /// Uses an explicit maximum number of elements produced through constant expansion.
    pub const fn with_expansions(mut self, expansions: u64) -> Self {
        self.expansions = expansions;

        self
    }

    /// Uses an explicit maximum bit size for one exact integer result.
    pub const fn with_integer_bits(mut self, integer_bits: u32) -> Self {
        self.integer_bits = integer_bits;

        self
    }

    /// Uses an explicit maximum nested constant-call depth.
    pub const fn with_call_depth(mut self, call_depth: u32) -> Self {
        self.call_depth = call_depth;

        self
    }

    /// Returns the maximum number of evaluated expression operations.
    pub const fn steps(self) -> u64 {
        self.steps
    }

    /// Returns the maximum number of aggregate elements materialized by the request.
    pub const fn aggregate_elements(self) -> u64 {
        self.aggregate_elements
    }

    /// Returns the maximum total source bytes decoded from literals.
    pub const fn literal_bytes(self) -> u64 {
        self.literal_bytes
    }

    /// Returns the maximum number of elements produced through constant expansion.
    pub const fn expansions(self) -> u64 {
        self.expansions
    }

    /// Returns the maximum bit size of one exact integer result.
    pub const fn integer_bits(self) -> u32 {
        self.integer_bits
    }

    /// Returns the maximum nested constant-call depth.
    pub const fn call_depth(self) -> u32 {
        self.call_depth
    }

    pub(super) const fn nested_call(self) -> Option<Self> {
        match self.call_depth.checked_sub(1) {
            Some(call_depth) => Some(self.with_call_depth(call_depth)),
            None => None,
        }
    }
}

impl Default for ConstantEvaluationLimits {
    fn default() -> Self {
        Self::new(1_000_000, 1_000_000, 16 * 1024 * 1024)
    }
}

pub(super) struct EvaluationBudget {
    remaining_steps: u64,
    remaining_elements: u64,
    remaining_literal_bytes: u64,
    remaining_expansions: u64,
    maximum_steps: u64,
    maximum_elements: u64,
    maximum_literal_bytes: u64,
    maximum_expansions: u64,
}

impl EvaluationBudget {
    pub(super) fn new(input: &ConstantEvaluationInput<'_>) -> Self {
        Self::from_limits(input.limits())
    }

    pub(super) const fn from_limits(limits: ConstantEvaluationLimits) -> Self {
        Self {
            remaining_steps: limits.steps(),
            remaining_elements: limits.aggregate_elements(),
            remaining_literal_bytes: limits.literal_bytes(),
            remaining_expansions: limits.expansions(),
            maximum_steps: limits.steps(),
            maximum_elements: limits.aggregate_elements(),
            maximum_literal_bytes: limits.literal_bytes(),
            maximum_expansions: limits.expansions(),
        }
    }

    pub(super) fn charge_step(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<(), EvaluationFailure> {
        self.try_charge_step()
            .map_err(|diagnostic| EvaluationFailure::Source {
                expression,
                diagnostic,
            })
    }

    pub(super) fn charge_elements(
        &mut self,
        expression: BoundExpressionId,
        count: usize,
    ) -> Result<(), EvaluationFailure> {
        self.try_charge_elements(count)
            .map_err(|diagnostic| EvaluationFailure::Source {
                expression,
                diagnostic,
            })
    }

    pub(super) fn charge_literal(
        &mut self,
        expression: BoundExpressionId,
        bytes: usize,
    ) -> Result<(), EvaluationFailure> {
        self.try_charge_literal(bytes)
            .map_err(|diagnostic| EvaluationFailure::Source {
                expression,
                diagnostic,
            })
    }

    pub(super) fn charge_expansion(
        &mut self,
        expression: BoundExpressionId,
        count: usize,
    ) -> Result<(), EvaluationFailure> {
        self.try_charge_expansion(count)
            .map_err(|diagnostic| EvaluationFailure::Source {
                expression,
                diagnostic,
            })
    }

    pub(super) fn charge_usage(
        &mut self,
        expression: BoundExpressionId,
        usage: ConstantEvaluationUsage,
    ) -> Result<(), EvaluationFailure> {
        self.try_charge_usage(usage)
            .map_err(|diagnostic| EvaluationFailure::Source {
                expression,
                diagnostic,
            })
    }

    pub(super) fn try_charge_step(&mut self) -> Result<(), ConstantDiagnostic> {
        charge(
            &mut self.remaining_steps,
            self.maximum_steps,
            1,
            ConstantLimitKind::EvaluationSteps,
        )
    }

    pub(super) fn try_charge_elements(&mut self, count: usize) -> Result<(), ConstantDiagnostic> {
        charge(
            &mut self.remaining_elements,
            self.maximum_elements,
            u64::try_from(count).unwrap_or(u64::MAX),
            ConstantLimitKind::AggregateElements,
        )
    }

    pub(super) fn try_charge_literal(&mut self, bytes: usize) -> Result<(), ConstantDiagnostic> {
        charge(
            &mut self.remaining_literal_bytes,
            self.maximum_literal_bytes,
            u64::try_from(bytes).unwrap_or(u64::MAX),
            ConstantLimitKind::LiteralBytes,
        )
    }

    pub(super) fn try_charge_expansion(&mut self, count: usize) -> Result<(), ConstantDiagnostic> {
        charge(
            &mut self.remaining_expansions,
            self.maximum_expansions,
            u64::try_from(count).unwrap_or(u64::MAX),
            ConstantLimitKind::ExpandedElements,
        )
    }

    pub(super) fn try_charge_usage(
        &mut self,
        usage: ConstantEvaluationUsage,
    ) -> Result<(), ConstantDiagnostic> {
        charge(
            &mut self.remaining_steps,
            self.maximum_steps,
            usage.steps(),
            ConstantLimitKind::EvaluationSteps,
        )?;

        charge(
            &mut self.remaining_elements,
            self.maximum_elements,
            usage.aggregate_elements(),
            ConstantLimitKind::AggregateElements,
        )?;

        charge(
            &mut self.remaining_literal_bytes,
            self.maximum_literal_bytes,
            usage.literal_bytes(),
            ConstantLimitKind::LiteralBytes,
        )?;

        charge(
            &mut self.remaining_expansions,
            self.maximum_expansions,
            usage.expansions(),
            ConstantLimitKind::ExpandedElements,
        )
    }

    pub(super) const fn remaining_limits(
        &self,
        limits: ConstantEvaluationLimits,
    ) -> ConstantEvaluationLimits {
        ConstantEvaluationLimits {
            steps: self.remaining_steps,
            aggregate_elements: self.remaining_elements,
            literal_bytes: self.remaining_literal_bytes,
            expansions: self.remaining_expansions,
            integer_bits: limits.integer_bits,
            call_depth: limits.call_depth,
        }
    }

    pub(super) const fn usage(&self, limits: ConstantEvaluationLimits) -> ConstantEvaluationUsage {
        ConstantEvaluationUsage::new(
            limits.steps - self.remaining_steps,
            limits.aggregate_elements - self.remaining_elements,
            limits.literal_bytes - self.remaining_literal_bytes,
        )
        .with_expansions(limits.expansions - self.remaining_expansions)
    }
}

fn charge(
    remaining: &mut u64,
    maximum: u64,
    amount: u64,
    resource: ConstantLimitKind,
) -> Result<(), ConstantDiagnostic> {
    let Some(updated) = remaining.checked_sub(amount) else {
        let consumed = maximum.saturating_sub(*remaining);

        return Err(ConstantDiagnostic::limit(
            resource,
            consumed.saturating_add(amount),
            maximum,
        ));
    };

    *remaining = updated;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConstantEvaluationLimits, ConstantEvaluationUsage, EvaluationBudget};
    use crate::constant::diagnostic::{ConstantDiagnostic, ConstantLimitKind};

    #[test]
    fn default_limits_are_finite_and_nonzero() {
        let limits = ConstantEvaluationLimits::default();

        assert!(limits.steps() > 0);
        assert!(limits.aggregate_elements() > 0);
        assert!(limits.literal_bytes() > 0);
        assert!(limits.expansions() > 0);
        assert!(limits.integer_bits() > 0);
        assert!(limits.call_depth() > 0);
    }

    #[test]
    fn exact_integer_limit_is_explicitly_configurable() {
        let limits = ConstantEvaluationLimits::default().with_integer_bits(9);

        assert_eq!(limits.integer_bits(), 9);
    }

    #[test]
    fn transitive_usage_charges_every_cumulative_limit() {
        let cases = [
            (
                ConstantEvaluationUsage::new(4, 0, 0),
                ConstantLimitKind::EvaluationSteps,
            ),
            (
                ConstantEvaluationUsage::new(0, 4, 0),
                ConstantLimitKind::AggregateElements,
            ),
            (
                ConstantEvaluationUsage::new(0, 0, 4),
                ConstantLimitKind::LiteralBytes,
            ),
            (
                ConstantEvaluationUsage::new(0, 0, 0).with_expansions(4),
                ConstantLimitKind::ExpandedElements,
            ),
        ];

        for (usage, expected) in cases {
            let limits = ConstantEvaluationLimits::new(3, 3, 3).with_expansions(3);
            let mut budget = EvaluationBudget::from_limits(limits);

            assert_eq!(
                budget.try_charge_usage(usage),
                Err(ConstantDiagnostic::limit(expected, 4, 3))
            );
        }
    }

    #[test]
    fn remaining_limits_preserve_noncumulative_policies() {
        let limits = ConstantEvaluationLimits::new(8, 9, 10)
            .with_expansions(13)
            .with_integer_bits(11)
            .with_call_depth(12);

        let mut budget = EvaluationBudget::from_limits(limits);

        budget
            .try_charge_usage(ConstantEvaluationUsage::new(1, 2, 3).with_expansions(4))
            .unwrap_or_else(|kind| panic!("usage must fit the available budget: {kind:?}"));

        let remaining = budget.remaining_limits(limits);

        assert_eq!(remaining.steps(), 7);
        assert_eq!(remaining.aggregate_elements(), 7);
        assert_eq!(remaining.literal_bytes(), 7);
        assert_eq!(remaining.expansions(), 9);
        assert_eq!(remaining.integer_bits(), 11);
        assert_eq!(remaining.call_depth(), 12);
    }
}
