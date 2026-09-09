use crate::{AnyBoundNodeId, BoundExpressionId, BoundUnitId, BoundUnitKind};

/// One semantic evaluation point of a bound node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundOperationPoint {
    /// The node's ordinary evaluation.
    Evaluation(AnyBoundNodeId),
    /// Transfer of a propagation expression's failure payload before exiting its scope.
    PropagationFailure(BoundExpressionId),
}

impl BoundOperationPoint {
    /// Returns the stable inspection tag distinguishing operations within a source node.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Evaluation(_) => "evaluation",
            Self::PropagationFailure(_) => "propagation_failure",
        }
    }

    /// Returns the source-correlated bound node containing this operation.
    pub const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::Evaluation(node) => node,
            Self::PropagationFailure(expression) => AnyBoundNodeId::Expression(expression),
        }
    }
}

/// A source-semantic way in which a bound unit's control flow can complete.
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
    /// The current run enters cancellation.
    Cancellation,
    /// Generator control yields a value.
    Yield,
    /// Recovery prevents a stronger completion classification.
    Recovered,
}

/// The immutable completion categories observed for one checked semantic unit.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ControlCompletion {
    kinds: u8,
}

/// Durable control-flow result for one exact checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CheckedControlFlow {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    completion: ControlCompletion,
}

impl CheckedControlFlow {
    /// Creates a durable control-flow result for one exact bound unit.
    pub const fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        completion: ControlCompletion,
    ) -> Self {
        Self {
            unit,
            kind,
            completion,
        }
    }

    /// Returns the exact bound unit this result describes.
    pub const fn unit(self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(self) -> BoundUnitKind {
        self.kind
    }

    /// Returns the unit's checked completion categories.
    pub const fn completion(self) -> ControlCompletion {
        self.completion
    }

    /// Returns whether conservative recovery affected control-flow checking.
    pub const fn is_recovered(self) -> bool {
        self.completion.contains(ControlCompletionKind::Recovered)
    }
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
    use crate::{BoundUnitId, BoundUnitKind};

    use super::{CheckedControlFlow, ControlCompletion, ControlCompletionKind};

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

    #[test]
    fn checked_control_flow_retains_exact_unit_category_and_completion() {
        let unit = BoundUnitId::new(4);
        let completion = ControlCompletion::from_kinds([ControlCompletionKind::Recovered]);

        let control_flow = CheckedControlFlow::new(unit, BoundUnitKind::CallableBody, completion);

        assert_eq!(control_flow.unit(), unit);
        assert_eq!(control_flow.kind(), BoundUnitKind::CallableBody);
        assert_eq!(control_flow.completion(), completion);
        assert!(control_flow.is_recovered());
    }
}
