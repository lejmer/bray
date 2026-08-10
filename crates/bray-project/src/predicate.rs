use std::sync::Arc;

use bray_target::{TargetFactKind, TargetFactValue, TargetProfile};

/// One literal value in a target dependency predicate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetPredicateValue {
    /// A language-defined target string.
    String(Arc<str>),
    /// A language-defined target unsigned integer.
    Usize(u64),
    /// A language-defined target Boolean.
    Boolean(bool),
}

impl TargetPredicateValue {
    pub(crate) fn matches(&self, value: TargetFactValue<'_>) -> bool {
        match (self, value) {
            (Self::String(expected), TargetFactValue::String(actual)) => {
                expected.as_ref() == actual
            }
            (Self::Usize(expected), TargetFactValue::Usize(actual)) => *expected == actual,
            (Self::Boolean(expected), TargetFactValue::Boolean(actual)) => *expected == actual,
            _ => false,
        }
    }

    pub(crate) const fn has_kind_of(&self, value: TargetFactValue<'_>) -> bool {
        matches!(
            (self, value),
            (Self::String(_), TargetFactValue::String(_))
                | (Self::Usize(_), TargetFactValue::Usize(_))
                | (Self::Boolean(_), TargetFactValue::Boolean(_))
        )
    }
}

/// A canonical target-property predicate retained by one project dependency edge.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetPredicate {
    /// Every child predicate must hold. An empty collection is true.
    All(Arc<[Self]>),
    /// At least one child predicate must hold. An empty collection is false.
    Any(Arc<[Self]>),
    /// The child predicate must not hold.
    Not(Box<Self>),
    /// One target property must equal one literal value.
    Equals(TargetFactKind, TargetPredicateValue),
    /// One target property must not equal one literal value.
    NotEquals(TargetFactKind, TargetPredicateValue),
    /// One target property must equal one member of a canonical literal set.
    In(TargetFactKind, Arc<[TargetPredicateValue]>),
}

impl TargetPredicate {
    pub(crate) fn evaluate(&self, profile: &TargetProfile) -> bool {
        match self {
            Self::All(children) => children.iter().all(|child| child.evaluate(profile)),
            Self::Any(children) => children.iter().any(|child| child.evaluate(profile)),
            Self::Not(child) => !child.evaluate(profile),
            Self::Equals(property, expected) => expected.matches(profile.fact(*property)),
            Self::NotEquals(property, expected) => !expected.matches(profile.fact(*property)),
            Self::In(property, expected) => {
                let actual = profile.fact(*property);

                expected.iter().any(|value| value.matches(actual))
            }
        }
    }

    pub(crate) fn collect_properties(&self, properties: &mut Vec<TargetFactKind>) {
        match self {
            Self::All(children) | Self::Any(children) => {
                for child in children.iter() {
                    child.collect_properties(properties);
                }
            }
            Self::Not(child) => child.collect_properties(properties),
            Self::Equals(property, _) | Self::NotEquals(property, _) | Self::In(property, _) => {
                properties.push(*property)
            }
        }
    }
}
