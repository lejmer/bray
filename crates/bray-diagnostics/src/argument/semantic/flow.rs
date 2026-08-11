use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact propagation-boundary failure argument.
    pub fn propagation_problem(problem: crate::DiagnosticPropagationProblem) -> Self {
        Self::new(
            DiagnosticArgName::PropagationProblem,
            DiagnosticArgValue::PropagationProblem(problem),
        )
    }

    /// Creates an exact fixed-array generator cardinality failure argument.
    pub fn array_generator_cardinality_problem(
        problem: crate::DiagnosticArrayGeneratorCardinalityProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::ArrayGeneratorCardinalityProblem,
            DiagnosticArgValue::ArrayGeneratorCardinalityProblem(problem),
        )
    }

    /// Creates an exact configured refinement-analysis capacity violation.
    pub const fn refinement_capacity(capacity: crate::DiagnosticRefinementCapacity) -> Self {
        Self::new(
            DiagnosticArgName::RefinementCapacity,
            DiagnosticArgValue::RefinementCapacity(capacity),
        )
    }

    /// Creates an exact compiler-provided memory operation argument.
    pub const fn memory_operation(operation: crate::DiagnosticMemoryOperation) -> Self {
        Self::new(
            DiagnosticArgName::MemoryOperation,
            DiagnosticArgValue::MemoryOperation(operation),
        )
    }

    /// Creates an exact callback-state contract failure argument.
    pub const fn callback_state_problem(problem: crate::DiagnosticCallbackStateProblem) -> Self {
        Self::new(
            DiagnosticArgName::CallbackStateProblem,
            DiagnosticArgValue::CallbackStateProblem(problem),
        )
    }

    /// Creates an exact source-semantic storage access argument.
    pub fn storage_access(access: crate::DiagnosticStorageAccess) -> Self {
        Self::new(
            DiagnosticArgName::StorageAccess,
            DiagnosticArgValue::StorageAccess(access),
        )
    }

    /// Creates exact missing pattern-coverage context.
    pub fn pattern_coverage(coverage: crate::DiagnosticPatternCoverage) -> Self {
        Self::new(
            DiagnosticArgName::PatternCoverage,
            DiagnosticArgValue::PatternCoverage(coverage),
        )
    }

    /// Creates an exact pattern-unreachability reason.
    pub const fn pattern_unreachability(reason: crate::DiagnosticPatternUnreachability) -> Self {
        Self::new(
            DiagnosticArgName::PatternUnreachability,
            DiagnosticArgValue::PatternUnreachability(reason),
        )
    }
}
