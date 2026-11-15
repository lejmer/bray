use std::sync::Arc;
use std::ops::Deref;

use super::ExecutionCondition;

/// Immutable operands retain their derived properties with the values that establish them.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd)]
pub struct ExecutionOperands(Arc<Operands>);

#[derive(Debug, Eq, Hash, PartialEq, PartialOrd)]
struct Operands {
    values: Box<[ExecutionCondition]>,
    contains_trusted: bool,
    contains_unknown: bool,
}

impl ExecutionOperands {
    pub(super) fn contains_trusted(&self) -> bool { self.0.contains_trusted }

    pub(super) fn contains_unknown(&self) -> bool { self.0.contains_unknown }
}

impl From<Vec<ExecutionCondition>> for ExecutionOperands {
    fn from(values: Vec<ExecutionCondition>) -> Self {
        let contains_trusted = values.iter().any(ExecutionCondition::contains_trusted);
        let contains_unknown = values.iter().any(ExecutionCondition::contains_unknown);

        Self(Arc::new(Operands { values: values.into_boxed_slice(), contains_trusted, contains_unknown }))
    }
}

impl<const N: usize> From<[ExecutionCondition; N]> for ExecutionOperands {
    fn from(values: [ExecutionCondition; N]) -> Self { Vec::from(values).into() }
}

impl FromIterator<ExecutionCondition> for ExecutionOperands {
    fn from_iter<T: IntoIterator<Item = ExecutionCondition>>(values: T) -> Self {
        Vec::from_iter(values).into()
    }
}

impl Deref for ExecutionOperands {
    type Target = [ExecutionCondition];

    fn deref(&self) -> &Self::Target { &self.0.values }
}

impl AsRef<[ExecutionCondition]> for ExecutionOperands {
    fn as_ref(&self) -> &[ExecutionCondition] { self }
}
