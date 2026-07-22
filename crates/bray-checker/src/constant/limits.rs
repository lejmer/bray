use bray_bound_tree::BoundExpressionId;
use bray_diagnostics::DiagnosticKind;

use super::{ConstantEvaluationInput, evaluation::EvaluationFailure};

/// Deterministic resource limits for one constant-evaluation request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConstantEvaluationLimits {
    steps: u64,
    aggregate_elements: u64,
    literal_bytes: u64,
    integer_bits: u32,
}

impl ConstantEvaluationLimits {
    /// Creates explicit limits for operations, aggregate elements, and source literal bytes.
    pub const fn new(steps: u64, aggregate_elements: u64, literal_bytes: u64) -> Self {
        Self {
            steps,
            aggregate_elements,
            literal_bytes,
            integer_bits: 16 * 1024 * 1024,
        }
    }

    /// Uses an explicit maximum bit size for one exact integer result.
    pub const fn with_integer_bits(mut self, integer_bits: u32) -> Self {
        self.integer_bits = integer_bits;

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

    /// Returns the maximum bit size of one exact integer result.
    pub const fn integer_bits(self) -> u32 {
        self.integer_bits
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
}

impl EvaluationBudget {
    pub(super) fn new(input: &ConstantEvaluationInput<'_>) -> Self {
        let limits = input.limits();

        Self {
            remaining_steps: limits.steps(),
            remaining_elements: limits.aggregate_elements(),
            remaining_literal_bytes: limits.literal_bytes(),
        }
    }

    pub(super) fn charge_step(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<(), EvaluationFailure> {
        charge(
            &mut self.remaining_steps,
            1,
            expression,
            DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
        )
    }

    pub(super) fn charge_elements(
        &mut self,
        expression: BoundExpressionId,
        count: usize,
    ) -> Result<(), EvaluationFailure> {
        charge(
            &mut self.remaining_elements,
            u64::try_from(count).unwrap_or(u64::MAX),
            expression,
            DiagnosticKind::CheckingConstantAggregateLimitExceeded,
        )
    }

    pub(super) fn charge_literal(
        &mut self,
        expression: BoundExpressionId,
        bytes: usize,
    ) -> Result<(), EvaluationFailure> {
        charge(
            &mut self.remaining_literal_bytes,
            u64::try_from(bytes).unwrap_or(u64::MAX),
            expression,
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        )
    }
}

fn charge(
    remaining: &mut u64,
    amount: u64,
    expression: BoundExpressionId,
    kind: DiagnosticKind,
) -> Result<(), EvaluationFailure> {
    let Some(updated) = remaining.checked_sub(amount) else {
        return Err(EvaluationFailure::Source { expression, kind });
    };

    *remaining = updated;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ConstantEvaluationLimits;

    #[test]
    fn default_limits_are_finite_and_nonzero() {
        let limits = ConstantEvaluationLimits::default();

        assert!(limits.steps() > 0);
        assert!(limits.aggregate_elements() > 0);
        assert!(limits.literal_bytes() > 0);
        assert!(limits.integer_bits() > 0);
    }

    #[test]
    fn exact_integer_limit_is_explicitly_configurable() {
        let limits = ConstantEvaluationLimits::default().with_integer_bits(9);

        assert_eq!(limits.integer_bits(), 9);
    }
}
