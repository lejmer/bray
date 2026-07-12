/// A source-semantic way in which a checked unit can complete.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ControlCompletionKind {
    /// Control reaches the unit's ordinary result boundary.
    Normal,
    /// Control returns from the enclosing callable.
    Return,
    /// Control propagates a result to the enclosing callable.
    Propagation,
    /// Control does not continue through the current path.
    Divergence,
    /// Control reaches a panic boundary.
    Panic,
    /// Asynchronous control is cancelled.
    Cancellation,
    /// Generator control yields a value.
    Yield,
    /// Recovery prevents a stronger completion fact.
    Recovered,
}

/// The immutable completion categories observed for one checked semantic unit.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ControlCompletion {
    kinds: u8,
}

impl ControlCompletion {
    /// Creates a completion summary from the observed categories.
    pub fn from_kinds(kinds: impl IntoIterator<Item = ControlCompletionKind>) -> Self {
        let mut completion = Self::default();

        for kind in kinds {
            completion.kinds |= kind.mask();
        }

        completion
    }

    /// Returns whether the unit can complete in the requested way.
    pub const fn contains(self, kind: ControlCompletionKind) -> bool {
        self.kinds & kind.mask() != 0
    }

    /// Returns whether the unit can cleanly reach its ordinary result boundary.
    pub const fn can_complete_normally(self) -> bool {
        self.contains(ControlCompletionKind::Normal)
    }

    /// Returns whether no reachable unit exit was observed.
    pub const fn is_empty(self) -> bool {
        self.kinds == 0
    }
}

impl ControlCompletionKind {
    const fn mask(self) -> u8 {
        1 << self as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlCompletion, ControlCompletionKind};

    #[test]
    fn completion_summaries_deduplicate_typed_categories() {
        let completion = ControlCompletion::from_kinds([
            ControlCompletionKind::Return,
            ControlCompletionKind::Normal,
            ControlCompletionKind::Return,
        ]);

        assert!(completion.can_complete_normally());
        assert!(completion.contains(ControlCompletionKind::Return));
        assert!(!completion.contains(ControlCompletionKind::Recovered));
    }
}
