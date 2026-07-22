use std::num::NonZeroU64;

use super::{TargetScalarFacts, TargetScalarKind};

const ALL_SCALARS: u32 = (1 << 22) - 1;
const OPTIONAL_SCALARS: u32 = TargetScalarKind::R16.bit()
    | TargetScalarKind::R128.bit()
    | TargetScalarKind::C32.bit()
    | TargetScalarKind::C256.bit();
const REQUIRED_SCALARS: u32 = ALL_SCALARS & !OPTIONAL_SCALARS;

/// Scalar representations accepted by one foreign callable ABI.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAbiScalarFacts {
    bits: u32,
}

impl TargetAbiScalarFacts {
    /// Creates the required portable scalar ABI surface.
    pub const fn required() -> Self {
        Self {
            bits: REQUIRED_SCALARS,
        }
    }

    /// Creates a scalar ABI surface containing every language scalar.
    pub const fn all() -> Self {
        Self { bits: ALL_SCALARS }
    }

    /// Returns a copy with one scalar accepted or rejected.
    pub const fn with_scalar(mut self, scalar: TargetScalarKind, accepted: bool) -> Self {
        if accepted {
            self.bits |= scalar.bit();
        } else {
            self.bits &= !scalar.bit();
        }

        self
    }

    /// Returns whether this ABI accepts the scalar by value.
    pub const fn supports(self, scalar: TargetScalarKind) -> bool {
        self.bits & scalar.bit() != 0
    }

    pub(crate) const fn is_supported_by(self, scalars: TargetScalarFacts) -> bool {
        (!self.supports(TargetScalarKind::R16) || scalars.real16())
            && (!self.supports(TargetScalarKind::R128) || scalars.real128())
            && (!self.supports(TargetScalarKind::C32) || scalars.complex32())
            && (!self.supports(TargetScalarKind::C256) || scalars.complex256())
    }
}

/// By-value representation contract for one available foreign callable ABI.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetForeignAbiFacts {
    scalars: TargetAbiScalarFacts,
    raw_pointers: bool,
    qualified_callables: bool,
    c_layout: bool,
    transparent_layout: bool,
    max_alignment: NonZeroU64,
}

impl TargetForeignAbiFacts {
    /// Creates a complete foreign ABI acceptance contract.
    pub const fn new(
        scalars: TargetAbiScalarFacts,
        raw_pointers: bool,
        qualified_callables: bool,
        c_layout: bool,
        transparent_layout: bool,
        max_alignment: NonZeroU64,
    ) -> Self {
        Self {
            scalars,
            raw_pointers,
            qualified_callables,
            c_layout,
            transparent_layout,
            max_alignment,
        }
    }

    /// Returns the scalar representations accepted by value.
    pub const fn scalars(self) -> TargetAbiScalarFacts {
        self.scalars
    }

    /// Returns whether raw pointers are accepted by value.
    pub const fn raw_pointers(self) -> bool {
        self.raw_pointers
    }

    /// Returns whether callable values qualified with this ABI are accepted by value.
    pub const fn qualified_callables(self) -> bool {
        self.qualified_callables
    }

    /// Returns whether C-layout aggregates are accepted by value.
    pub const fn c_layout(self) -> bool {
        self.c_layout
    }

    /// Returns whether transparent-layout aggregates are accepted by value.
    pub const fn transparent_layout(self) -> bool {
        self.transparent_layout
    }

    /// Returns the largest aggregate alignment accepted by this ABI.
    pub const fn max_alignment(self) -> NonZeroU64 {
        self.max_alignment
    }
}

/// Callable ABI families and their by-value contracts on one target.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAbiFacts {
    c: Option<TargetForeignAbiFacts>,
    system: Option<TargetForeignAbiFacts>,
}

impl TargetAbiFacts {
    /// Creates the available foreign callable ABI contracts.
    pub const fn new(
        c: Option<TargetForeignAbiFacts>,
        system: Option<TargetForeignAbiFacts>,
    ) -> Self {
        Self { c, system }
    }

    /// Returns whether the target C ABI is available.
    pub const fn c(self) -> bool {
        self.c.is_some()
    }

    /// Returns the selected target's C ABI contract.
    pub const fn c_contract(self) -> Option<TargetForeignAbiFacts> {
        self.c
    }

    /// Returns whether the target system ABI is available.
    pub const fn system(self) -> bool {
        self.system.is_some()
    }

    /// Returns the selected target's system ABI contract.
    pub const fn system_contract(self) -> Option<TargetForeignAbiFacts> {
        self.system
    }

    pub(crate) const fn is_supported_by(self, scalars: TargetScalarFacts) -> bool {
        let c_valid = match self.c {
            Some(contract) => contract.scalars().is_supported_by(scalars),
            None => true,
        };
        let system_valid = match self.system {
            Some(contract) => contract.scalars().is_supported_by(scalars),
            None => true,
        };

        c_valid && system_valid
    }
}
