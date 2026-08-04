use std::sync::Arc;

use bray_base::shared_str;

use crate::TestSourceAnchor;

/// Structured data produced when a built-in assertion fails.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssertionFailure {
    source: TestSourceAnchor,
    message: Option<Arc<str>>,
}

impl AssertionFailure {
    /// Creates an assertion failure without a user-supplied message.
    pub const fn without_message(source: TestSourceAnchor) -> Self {
        Self {
            source,
            message: None,
        }
    }

    /// Creates an assertion failure with its evaluated user message.
    pub fn with_message(source: TestSourceAnchor, message: impl Into<Arc<str>>) -> Self {
        Self {
            source,
            message: Some(shared_str(message)),
        }
    }

    /// Returns the assertion expression that failed.
    pub const fn source(&self) -> TestSourceAnchor {
        self.source
    }

    /// Returns the evaluated user message when one was supplied.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Structured data produced by `std.testing.fail`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExplicitTestFailure {
    source: TestSourceAnchor,
    message: Arc<str>,
}

impl ExplicitTestFailure {
    /// Creates an explicit failure from its call source and consumed message.
    pub fn new(source: TestSourceAnchor, message: impl Into<Arc<str>>) -> Self {
        Self {
            source,
            message: shared_str(message),
        }
    }

    /// Returns the `std.testing.fail` call that produced the failure.
    pub const fn source(&self) -> TestSourceAnchor {
        self.source
    }

    /// Returns the evaluated failure message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};

    use super::{AssertionFailure, ExplicitTestFailure};
    use crate::TestSourceAnchor;

    #[test]
    fn failures_retain_structured_source_and_messages() {
        let source = TestSourceAnchor::new(
            SourceSpan::new(
                SourceId::new(3),
                TextRange::new(TextSize::new(7), TextSize::new(12)),
            ),
            SourceVersion::new(4),
        );

        let assertion = AssertionFailure::with_message(source, "expected equality");
        let message_free_assertion = AssertionFailure::without_message(source);
        let explicit = ExplicitTestFailure::new(source, "fixture setup failed");

        assert_eq!(assertion.source(), source);
        assert_eq!(assertion.message(), Some("expected equality"));
        assert_eq!(message_free_assertion.message(), None);
        assert_eq!(explicit.source(), source);
        assert_eq!(explicit.message(), "fixture setup failed");
    }
}
