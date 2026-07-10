use crate::BoundSourceAnchor;

/// A stable source-order ordinal for synthesized nodes sharing an anchor and role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundNodeOrdinal(u32);

impl BoundNodeOrdinal {
    /// Creates an ordinal from its stable source-semantic order.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the stable source-semantic order.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Converts the ordinal to a checked collection index for the current target.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

/// Classifies the semantic rule that requires a synthesized bound node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundSynthesisRole {
    /// A semantic conversion inserted by checking.
    Conversion,
    /// A default argument or initializer selected at a use site.
    DefaultValue,
    /// A temporary required to preserve evaluation or ownership semantics.
    Temporary,
    /// An implicit ownership operation such as a move, copy, borrow, or drop.
    OwnershipOperation,
    /// A control-flow operation required by source-shaped semantics.
    ControlFlow,
    /// A recovery node required after invalid source.
    Recovery,
}

/// Source-correlated provenance for a synthesized bound node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SynthesizedBoundNodeOrigin {
    source: BoundSourceAnchor,
    role: BoundSynthesisRole,
    ordinal: BoundNodeOrdinal,
}

impl SynthesizedBoundNodeOrigin {
    /// Creates synthesized provenance tied to a source construct and semantic role.
    pub const fn new(
        source: BoundSourceAnchor,
        role: BoundSynthesisRole,
        ordinal: BoundNodeOrdinal,
    ) -> Self {
        Self {
            source,
            role,
            ordinal,
        }
    }

    /// Returns the source construct that required the synthesized node.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns the semantic role that required the synthesized node.
    pub const fn role(self) -> BoundSynthesisRole {
        self.role
    }

    /// Returns the source-semantic ordinal within the same anchor and role.
    pub const fn ordinal(self) -> BoundNodeOrdinal {
        self.ordinal
    }
}

/// Describes how a bound node entered a checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundNodeOrigin {
    /// The node corresponds directly to source syntax.
    Source(BoundSourceAnchor),
    /// The node was synthesized to make checked semantics explicit.
    Synthesized(SynthesizedBoundNodeOrigin),
}

impl BoundNodeOrigin {
    /// Creates provenance for a source-originating node.
    pub const fn source(source: BoundSourceAnchor) -> Self {
        Self::Source(source)
    }

    /// Creates provenance for a synthesized node.
    pub const fn synthesized(
        source: BoundSourceAnchor,
        role: BoundSynthesisRole,
        ordinal: BoundNodeOrdinal,
    ) -> Self {
        Self::Synthesized(SynthesizedBoundNodeOrigin::new(source, role, ordinal))
    }

    /// Returns the source construct correlated with this node.
    pub const fn source_anchor(self) -> BoundSourceAnchor {
        match self {
            Self::Source(source) => source,
            Self::Synthesized(origin) => origin.source(),
        }
    }

    /// Returns synthesized provenance when the node was compiler-created.
    pub const fn synthesized_origin(self) -> Option<SynthesizedBoundNodeOrigin> {
        match self {
            Self::Source(_) => None,
            Self::Synthesized(origin) => Some(origin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundNodeOrdinal, BoundSynthesisRole};
    use crate::BoundNodeOrigin;
    use crate::test_support::source_anchor;

    #[test]
    fn source_and_synthesized_origins_retain_source_correlation() {
        let source = source_anchor();

        let source_origin = BoundNodeOrigin::source(source);
        let synthesized = BoundNodeOrigin::synthesized(
            source,
            BoundSynthesisRole::Conversion,
            BoundNodeOrdinal::new(2),
        );

        assert_eq!(source_origin.source_anchor(), source);
        assert_eq!(source_origin.synthesized_origin(), None);
        assert_eq!(synthesized.source_anchor(), source);

        let Some(synthesized) = synthesized.synthesized_origin() else {
            panic!("synthesized origin must retain synthesized provenance");
        };

        assert_eq!(synthesized.role(), BoundSynthesisRole::Conversion);
        assert_eq!(synthesized.ordinal(), BoundNodeOrdinal::new(2));
    }

    #[test]
    fn synthesized_ordinals_distinguish_repeated_roles() {
        let source = source_anchor();

        let first = BoundNodeOrigin::synthesized(
            source,
            BoundSynthesisRole::Temporary,
            BoundNodeOrdinal::new(0),
        );
        let second = BoundNodeOrigin::synthesized(
            source,
            BoundSynthesisRole::Temporary,
            BoundNodeOrdinal::new(1),
        );

        assert_ne!(first, second);
    }
}
