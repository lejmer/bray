use std::sync::Arc;

use bray_diagnostics::DiagnosticTargetPredicateValueKind;
use bray_target::{TargetProfile, TargetPropertyKind, TargetPropertyValue};

/// The literal category accepted by one target predicate property.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetPredicateValueKind {
    /// A language-defined target string.
    String,
    /// A language-defined target unsigned integer.
    UnsignedInteger,
    /// A language-defined target Boolean.
    Boolean,
}

impl TargetPredicateValueKind {
    pub(crate) const fn of_target_value(value: TargetPropertyValue<'_>) -> Self {
        match value {
            TargetPropertyValue::String(_) => Self::String,
            TargetPropertyValue::Usize(_) => Self::UnsignedInteger,
            TargetPropertyValue::Boolean(_) => Self::Boolean,
        }
    }
}

impl From<TargetPredicateValueKind> for DiagnosticTargetPredicateValueKind {
    fn from(value: TargetPredicateValueKind) -> Self {
        match value {
            TargetPredicateValueKind::String => Self::String,
            TargetPredicateValueKind::UnsignedInteger => Self::UnsignedInteger,
            TargetPredicateValueKind::Boolean => Self::Boolean,
        }
    }
}

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
    pub(crate) const fn kind(&self) -> TargetPredicateValueKind {
        match self {
            Self::String(_) => TargetPredicateValueKind::String,
            Self::Usize(_) => TargetPredicateValueKind::UnsignedInteger,
            Self::Boolean(_) => TargetPredicateValueKind::Boolean,
        }
    }

    pub(crate) fn matches(&self, value: TargetPropertyValue<'_>) -> bool {
        match (self, value) {
            (Self::String(expected), TargetPropertyValue::String(actual)) => {
                expected.as_ref() == actual
            }
            (Self::Usize(expected), TargetPropertyValue::Usize(actual)) => *expected == actual,
            (Self::Boolean(expected), TargetPropertyValue::Boolean(actual)) => *expected == actual,
            _ => false,
        }
    }

    pub(crate) fn has_kind_of(&self, value: TargetPropertyValue<'_>) -> bool {
        self.kind() == TargetPredicateValueKind::of_target_value(value)
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
    Equals(TargetPropertyKind, TargetPredicateValue),
    /// One target property must not equal one literal value.
    NotEquals(TargetPropertyKind, TargetPredicateValue),
    /// One target property must equal one member of a canonical literal set.
    In(TargetPropertyKind, Arc<[TargetPredicateValue]>),
}

impl TargetPredicate {
    pub(crate) fn evaluate(&self, profile: &TargetProfile) -> bool {
        match self {
            Self::All(children) => children.iter().all(|child| child.evaluate(profile)),
            Self::Any(children) => children.iter().any(|child| child.evaluate(profile)),
            Self::Not(child) => !child.evaluate(profile),
            Self::Equals(property, expected) => expected.matches(profile.property(*property)),
            Self::NotEquals(property, expected) => !expected.matches(profile.property(*property)),
            Self::In(property, expected) => {
                let actual = profile.property(*property);

                expected.iter().any(|value| value.matches(actual))
            }
        }
    }

    pub(crate) fn collect_properties(&self, properties: &mut Vec<TargetPropertyKind>) {
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
