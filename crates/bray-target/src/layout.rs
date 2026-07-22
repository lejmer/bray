use std::num::NonZeroU64;

/// The physical layout contract selected for one value representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetLayoutContract {
    /// Compiler-selected default layout.
    Default,
    /// Bray stable layout for the selected target profile.
    Stable,
    /// C-compatible layout for the selected target ABI.
    C,
    /// Layout identical to one represented storage field.
    Transparent,
}

/// A selected physical value layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetValueLayout {
    size: u64,
    alignment: NonZeroU64,
    contract: TargetLayoutContract,
}

impl TargetValueLayout {
    /// Creates a selected physical layout.
    pub const fn new(size: u64, alignment: NonZeroU64, contract: TargetLayoutContract) -> Self {
        Self {
            size,
            alignment,
            contract,
        }
    }

    /// Returns the represented size in bytes.
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Returns the required alignment in bytes.
    pub const fn alignment(self) -> NonZeroU64 {
        self.alignment
    }

    /// Returns the selected physical layout contract.
    pub const fn contract(self) -> TargetLayoutContract {
        self.contract
    }
}
