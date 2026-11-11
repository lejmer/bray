use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use super::ConstantTermId;

/// One unchanged owned input embedded wherever its result condition holds.
/// This records identity rather than equality, borrowing, or a new trusted guarantee.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyOwnedTransfer<
    Projection = super::ConstantProjectionKind,
    Input = ConstantTermId,
> {
    input: Input,
    output: Arc<[Projection]>,
    condition: Option<Input>,
}

impl<Projection, Input: Clone> DependencyOwnedTransfer<Projection, Input> {
    /// Records the exact input and its owning path within the returned value.
    pub fn new(input: Input, output: impl IntoIterator<Item = Projection>) -> Self {
        Self {
            input,
            output: shared_slice(output),
            condition: None,
        }
    }

    /// Limits the identity transfer to the ordinary active-result condition.
    pub fn with_condition(mut self, condition: Option<Input>) -> Self {
        self.condition = condition;

        self
    }

    /// Returns the ordinary result condition, or None for unconditional ownership.
    pub fn condition(&self) -> Option<Input> {
        self.condition.clone()
    }

    /// Returns the formal input or owned input projection.
    pub fn input(&self) -> Input {
        self.input.clone()
    }

    /// Returns the owning path relative to the complete result.
    pub fn output(&self) -> &[Projection] {
        &self.output
    }
}

/// The complete owning layouts possible at a callable's normal return.
/// Alternatives preserve complete-carrier boundaries and simultaneous field mappings.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyOwnedResult<Projection = super::ConstantProjectionKind, Input = ConstantTermId> {
    /// One complete input moves into a result subvalue.
    Transfer(DependencyOwnedTransfer<Projection, Input>),
    /// All mappings occur together. An empty collection transports no caller input.
    All(Arc<[Self]>),
    /// One of these complete owning layouts occurs.
    Choice(Arc<[Self]>),
    /// This layout occurs only under the ordinary normal-result condition.
    Case { condition: Input, result: Arc<Self> },
}

impl<Projection, Input> Default for DependencyOwnedResult<Projection, Input> {
    fn default() -> Self {
        Self::All(Arc::new([]))
    }
}

impl<Projection: Clone + Ord, Input: Clone + Ord> DependencyOwnedResult<Projection, Input> {
    /// Retains a complete alternative's ordinary shape, including empty layouts.
    pub fn case(condition: Input, result: Self) -> Self {
        let mut inner = &result;

        while let Self::Case {
            condition: previous,
            result: child,
        } = inner
        {
            if previous == &condition {
                return result;
            }

            inner = child;
        }

        Self::Case {
            condition,
            result: Arc::new(result),
        }
    }

    /// Combines simultaneous mappings without retaining redundant nested conjunctions.
    pub fn all(values: impl IntoIterator<Item = Self>) -> Self {
        let mut flattened = Vec::new();

        for value in values {
            match value {
                Self::All(values) => flattened.extend(values.iter().cloned()),
                value => flattened.push(value),
            }
        }

        let values = sorted_unique_shared_slice(flattened);

        if let [value] = values.as_ref() {
            value.clone()
        } else {
            Self::All(values)
        }
    }

    /// Combines possible layouts while preserving alternatives with no transferred input.
    pub fn choice(values: impl IntoIterator<Item = Self>) -> Self {
        let mut flattened = Vec::new();

        for value in values {
            match value {
                Self::Choice(values) => flattened.extend(values.iter().cloned()),
                value => flattened.push(value),
            }
        }

        let values = sorted_unique_shared_slice(flattened);

        match values.as_ref() {
            [] => Self::default(),
            [value] => value.clone(),
            _ => Self::Choice(values),
        }
    }
}

impl<Projection, Input> DependencyOwnedResult<Projection, Input> {
    /// Whether no caller-owned input is transported.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Transfer(_) => false,
            Self::All(values) | Self::Choice(values) => values.iter().all(Self::is_empty),
            Self::Case { result, .. } => result.is_empty(),
        }
    }

    /// Visits ordinary layout and exact-move conditions.
    pub fn conditions(&self) -> impl Iterator<Item = &Input> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            while let Some(value) = pending.pop() {
                match value {
                    Self::Transfer(transfer) => {
                        if let Some(condition) = &transfer.condition {
                            return Some(condition);
                        }
                    }
                    Self::All(values) | Self::Choice(values) => pending.extend(values.iter().rev()),
                    Self::Case { condition, result } => {
                        pending.push(result);

                        return Some(condition);
                    }
                }
            }

            None
        })
    }

    /// Visits exact moves with the ordinary conditions of their complete alternatives.
    pub fn guarded_transfers(
        &self,
    ) -> impl Iterator<Item = (&DependencyOwnedTransfer<Projection, Input>, Vec<&Input>)> {
        let mut pending = vec![(self, Vec::new())];

        std::iter::from_fn(move || {
            while let Some((value, mut conditions)) = pending.pop() {
                match value {
                    Self::Transfer(transfer) => return Some((transfer, conditions)),
                    Self::All(values) | Self::Choice(values) => {
                        pending.extend(values.iter().rev().map(|value| (value, conditions.clone())))
                    }
                    Self::Case { condition, result } => {
                        conditions.push(condition);
                        pending.push((result, conditions));
                    }
                }
            }

            None
        })
    }

    /// Visits exact moves in every possible owning layout.
    pub fn transfers(&self) -> impl Iterator<Item = &DependencyOwnedTransfer<Projection, Input>> {
        let mut pending = vec![self];

        std::iter::from_fn(move || {
            while let Some(value) = pending.pop() {
                match value {
                    Self::Transfer(transfer) => return Some(transfer),
                    Self::All(values) | Self::Choice(values) => pending.extend(values.iter().rev()),
                    Self::Case { result, .. } => pending.push(result),
                }
            }

            None
        })
    }
}

impl<Projection, Input> DependencyOwnedResult<Projection, Input> {
    /// Rewrites exact mappings while retaining simultaneous and alternative layouts.
    /// Omitting an inexpressible ordinary condition conservatively retains its possible layout.
    pub fn try_map<OtherProjection: Clone + Ord, OtherInput: Clone + Ord, Error>(
        &self,
        map: &mut impl FnMut(
            &DependencyOwnedTransfer<Projection, Input>,
        )
            -> Result<DependencyOwnedResult<OtherProjection, OtherInput>, Error>,
        condition: &mut impl FnMut(&Input) -> Result<Option<OtherInput>, Error>,
    ) -> Result<DependencyOwnedResult<OtherProjection, OtherInput>, Error> {
        match self {
            Self::Transfer(transfer) => map(transfer),
            Self::Case {
                condition: guard,
                result,
            } => {
                let guard = condition(guard)?;
                let result = result.try_map(map, condition)?;

                Ok(match guard {
                    Some(guard) => DependencyOwnedResult::case(guard, result),
                    None => result,
                })
            }
            Self::All(values) => values
                .iter()
                .map(|value| value.try_map(map, condition))
                .collect::<Result<Vec<_>, _>>()
                .map(DependencyOwnedResult::all),
            Self::Choice(values) => values
                .iter()
                .map(|value| value.try_map(map, condition))
                .collect::<Result<Vec<_>, _>>()
                .map(DependencyOwnedResult::choice),
        }
    }
}
