use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use super::{ConstantTermId, DependencyValuePreservation};

/// One owned input embedded wherever its result condition holds.
/// This records identity rather than equality, borrowing, or a new trusted guarantee.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyOwnedTransfer<
    Projection = super::ConstantProjectionKind,
    Input = ConstantTermId,
> {
    input: Input,
    output: Arc<[Projection]>,
    condition: Option<Input>,
    preservation: DependencyValuePreservation<Projection>,
}

impl<Projection, Input: Clone> DependencyOwnedTransfer<Projection, Input> {
    /// Records the exact input and its owning path within the returned value.
    pub fn new(input: Input, output: impl IntoIterator<Item = Projection>) -> Self {
        Self {
            input,
            output: shared_slice(output),
            condition: None,
            preservation: DependencyValuePreservation::none(),
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

    /// Records which input contents survive the identity transfer.
    pub fn with_preservation(mut self, preservation: DependencyValuePreservation<Projection>) -> Self {
        self.preservation = preservation;

        self
    }

    /// Returns the contents-preservation mask relative to the input.
    pub fn preservation(&self) -> &DependencyValuePreservation<Projection> {
        &self.preservation
    }

    /// Compares the identity mapping independently of preserved contents.
    pub fn same_identity(&self, other: &Self) -> bool
    where Projection: PartialEq, Input: PartialEq {
        self.input == other.input && self.output == other.output && self.condition == other.condition
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
    /// Distributes guards over choices and orders them so repeated joins remain canonical.
    pub fn case(condition: Input, result: Self) -> Self {
        match result {
            Self::Choice(values) => Self::choice(
                values
                    .iter()
                    .map(|value| Self::case(condition.clone(), value.clone())),
            ),
            Self::Case {
                condition: previous,
                result,
            } if previous < condition => {
                Self::case(previous, Self::case(condition, result.as_ref().clone()))
            }
            Self::Case {
                condition: previous,
                result,
            } if previous == condition => Self::Case {
                condition: previous,
                result,
            },
            result => Self::Case {
                condition,
                result: Arc::new(result),
            },
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

    /// Returns input identities transferred in every normal owning alternative.
    /// A conditional transfer alone does not establish unconditional ownership.
    pub fn guaranteed_transfers(&self) -> std::collections::BTreeSet<DependencyOwnedTransfer<Projection, Input>>
    where Projection: Clone + Ord, Input: Clone + Ord {
        match self {
            Self::Transfer(transfer) if transfer.condition.is_none() => [transfer.clone()].into(),
            Self::Transfer(_) | Self::Case { .. } => Default::default(),
            Self::All(values) => {
                let mut shared = std::collections::BTreeMap::<_, DependencyOwnedTransfer<Projection, Input>>::new();

                for transfer in values.iter().flat_map(Self::guaranteed_transfers) {
                    shared.entry((transfer.input.clone(), transfer.output.clone(), transfer.condition.clone()))
                        .and_modify(|previous| {
                            previous.preservation = previous.preservation.intersection(&transfer.preservation);
                        })
                        .or_insert(transfer);
                }

                shared.into_values().collect()
            }
            Self::Choice(values) => {
                let mut values = values.iter();

                let Some(first) = values.next() else {
                    return Default::default();
                };

                let mut shared = first.guaranteed_transfers();

                for value in values {
                    let incoming = value.guaranteed_transfers();

                    shared = shared.into_iter().filter_map(|mut transfer| {
                        let other = incoming.iter().find(|other| transfer.same_identity(other))?;

                        transfer.preservation = transfer.preservation.intersection(&other.preservation);

                        Some(transfer)
                    }).collect();
                }

                shared
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{DependencyOwnedResult, DependencyOwnedTransfer};

    type Layout = DependencyOwnedResult<u8, u8>;

    #[test]
    fn guaranteed_identity_intersects_contents_in_choices_and_duplicate_mappings() {
        use super::DependencyValuePreservation;

        let partial = DependencyValuePreservation::new(true, [(0, DependencyValuePreservation::none())].into());
        let complete = DependencyOwnedTransfer::new(5, [0]).with_preservation(DependencyValuePreservation::all());
        let changed = complete.clone().with_preservation(partial);

        assert!(complete.same_identity(&changed));

        for layout in [
            Layout::choice([Layout::Transfer(complete.clone()), Layout::Transfer(changed.clone())]),
            Layout::all([Layout::Transfer(complete), Layout::Transfer(changed.clone())]),
        ] {
            assert_eq!(layout.guaranteed_transfers(), [changed.clone()].into());
        }
    }

    #[test]
    fn guaranteed_transfers_require_the_same_move_in_every_alternative() {
        let common = DependencyOwnedTransfer::new(5, [0]);
        let other = DependencyOwnedTransfer::new(7, [1]);
        let simultaneous = Layout::all([Layout::Transfer(common.clone()), Layout::Transfer(other.clone())]);
        let choice = Layout::choice([simultaneous.clone(), Layout::Transfer(common.clone())]);

        assert_eq!(choice.guaranteed_transfers(), [common.clone()].into());
        assert_eq!(simultaneous.guaranteed_transfers(), [common.clone(), other].into());
        assert!(Layout::case(2, simultaneous).guaranteed_transfers().is_empty());
        assert!(Layout::choice([choice, Layout::default()]).guaranteed_transfers().is_empty());
        assert!(Layout::Transfer(common.with_condition(Some(2))).guaranteed_transfers().is_empty());
    }

    #[test]
    fn guarded_loop_alternatives_reach_a_fixed_point() {
        let original = Layout::choice([
            Layout::Transfer(DependencyOwnedTransfer::new(7, [])),
            Layout::default(),
        ]);

        let expected = Layout::choice([
            original.clone(),
            Layout::case(2, original.clone()),
            Layout::case(3, original.clone()),
            Layout::case(2, Layout::case(3, original.clone())),
        ]);

        let mut current = original;

        for _ in 0..32 {
            current = Layout::choice([
                current.clone(),
                Layout::case(2, current.clone()),
                Layout::case(3, current),
            ]);
        }

        assert_eq!(current, expected);

        assert_eq!(
            Layout::case(2, current.clone()),
            Layout::case(2, Layout::case(2, current))
        );
    }

    #[test]
    fn guarded_layouts_keep_complete_alternatives_and_empty_cases() {
        let layout = Layout::all([
            Layout::Transfer(DependencyOwnedTransfer::new(5, [0])),
            Layout::Transfer(DependencyOwnedTransfer::new(7, [1])),
        ]);

        let forward = Layout::case(2, Layout::case(3, layout.clone()));
        let reverse = Layout::case(3, Layout::case(2, layout.clone()));

        assert_eq!(forward, reverse);
        assert_eq!(forward.guarded_transfers().count(), 2);

        assert!(
            forward
                .guarded_transfers()
                .all(|(_, guards)| guards == vec![&2, &3])
        );

        assert_eq!(
            Layout::case(2, Layout::default()),
            Layout::Case {
                condition: 2,
                result: std::sync::Arc::new(Layout::default()),
            }
        );

        assert_eq!(layout.transfers().count(), 2);
    }
}
