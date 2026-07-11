use bray_symbols::TypeId;

use crate::BoundNodeOrigin;

/// A checked pattern retaining its exact semantic category.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundPattern {
    /// A pattern that could not be checked successfully.
    Error(BoundErrorPattern),
}

impl BoundPattern {
    /// Returns the source or synthesized origin of this pattern.
    pub const fn origin(&self) -> BoundNodeOrigin {
        match self {
            Self::Error(pattern) => pattern.origin(),
        }
    }

    /// Returns the checked or recovery input type of this pattern.
    pub const fn input_type(&self) -> TypeId {
        match self {
            Self::Error(pattern) => pattern.input_type(),
        }
    }
}

/// A pattern preserved after semantic recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundErrorPattern {
    origin: BoundNodeOrigin,
    input_type: TypeId,
}

impl BoundErrorPattern {
    /// Creates an error pattern with the most useful input type known after recovery.
    pub const fn new(origin: BoundNodeOrigin, input_type: TypeId) -> Self {
        Self { origin, input_type }
    }

    /// Returns the source or synthesized origin of this pattern.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the useful recovery input type or canonical error type.
    pub const fn input_type(self) -> TypeId {
        self.input_type
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_type, source_anchor};
    use crate::{BoundErrorPattern, BoundNodeOrigin, BoundPattern};

    #[test]
    fn error_patterns_retain_source_and_recovery_input_type() {
        let source = source_anchor();
        let input_type = error_type();

        let pattern = BoundPattern::Error(BoundErrorPattern::new(
            BoundNodeOrigin::source(source),
            input_type,
        ));

        assert_eq!(pattern.origin().source_anchor(), source);
        assert_eq!(pattern.input_type(), input_type);
    }
}
