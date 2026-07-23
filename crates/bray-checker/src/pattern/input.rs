use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::BoundPatternId;
use bray_symbols::TypeId;

/// The selected element type supplied to one iteration pattern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IterationPatternType {
    pattern: BoundPatternId,
    element_type: TypeId,
    is_recovered: bool,
}

impl IterationPatternType {
    /// Creates one iteration-pattern type input.
    pub const fn new(pattern: BoundPatternId, element_type: TypeId, is_recovered: bool) -> Self {
        Self {
            pattern,
            element_type,
            is_recovered,
        }
    }

    /// Returns the exact iteration pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the selected iteration element type.
    pub const fn element_type(self) -> TypeId {
        self.element_type
    }

    /// Returns whether iteration selection recovered.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// Contextual types unavailable from the bound unit alone.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PatternCheckInput {
    iteration_patterns: Arc<[IterationPatternType]>,
}

impl PatternCheckInput {
    /// Creates an empty pattern-checking input.
    pub fn new() -> Self {
        Self {
            iteration_patterns: Arc::from([]),
        }
    }

    /// Returns this input with selected iteration element types.
    pub fn with_iteration_patterns(
        mut self,
        patterns: impl IntoIterator<Item = IterationPatternType>,
    ) -> Self {
        self.iteration_patterns = shared_slice(patterns);

        self
    }

    pub(super) fn iteration_patterns(&self) -> &[IterationPatternType] {
        &self.iteration_patterns
    }
}
