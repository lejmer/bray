/// Flow-sensitive analysis resource whose configured limit was exceeded.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticRefinementCapacitySurface {
    /// Distinct refinement entries retained for one checked unit.
    RefinementEntries,
    /// Bitset cells retained across the unit's control-flow states.
    RetainedStateCells,
    /// Refinement entries published at source operations.
    PublishedRefinements,
}

impl DiagnosticRefinementCapacitySurface {
    /// Returns the stable machine key for this capacity surface.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RefinementEntries => "refinement_entries",
            Self::RetainedStateCells => "retained_state_cells",
            Self::PublishedRefinements => "published_refinements",
        }
    }
}

/// Exact configured refinement-analysis capacity violation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticRefinementCapacity {
    surface: DiagnosticRefinementCapacitySurface,
    actual: u64,
    maximum: u64,
}

impl DiagnosticRefinementCapacity {
    /// Creates a capacity violation when `actual` exceeds `maximum`.
    pub const fn try_new(
        surface: DiagnosticRefinementCapacitySurface,
        actual: u64,
        maximum: u64,
    ) -> Option<Self> {
        if actual <= maximum {
            return None;
        }

        Some(Self {
            surface,
            actual,
            maximum,
        })
    }

    /// Returns the bounded refinement-analysis resource.
    pub const fn surface(self) -> DiagnosticRefinementCapacitySurface {
        self.surface
    }

    /// Returns the exact attempted resource count.
    pub const fn actual(self) -> u64 {
        self.actual
    }

    /// Returns the configured maximum resource count.
    pub const fn maximum(self) -> u64 {
        self.maximum
    }
}
