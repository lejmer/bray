macro_rules! define_unit_local_ids {
    ($($name:ident, $documentation:literal;)+) => {
        $(
            #[doc = $documentation]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $name {
                unit: MirUnitId,
                slot: u32,
            }

            impl $name {
                pub(crate) const fn from_slot(unit: MirUnitId, slot: u32) -> Self {
                    Self { unit, slot }
                }

                /// Returns the MIR unit that owns this ID.
                pub const fn unit(self) -> MirUnitId {
                    self.unit
                }

                /// Returns the unit-local numeric representation.
                pub const fn slot(self) -> u32 {
                    self.slot
                }

                pub(crate) fn to_index(self) -> Option<usize> {
                    usize::try_from(self.slot).ok()
                }
            }
        )+
    };
}

/// Identifies one immutable MIR unit within a compilation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirUnitId(u32);

impl MirUnitId {
    /// Creates a compilation-local MIR unit ID.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compilation-local numeric representation.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

define_unit_local_ids! {
    MirBlockId, "Identifies one control-flow block within a MIR unit.";
    MirOperationId, "Identifies one operation within a MIR unit.";
    MirStorageId, "Identifies one storage allocation within a MIR unit.";
    MirValueId, "Identifies one typed value within a MIR unit.";
}

/// Identifies one resumable state in a protected async frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirFrameStateId(u32);

impl MirFrameStateId {
    /// Creates a state ID from its descriptor-local ordinal.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the descriptor-local ordinal.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

pub(crate) fn compact_slot(index: usize) -> Option<u32> {
    u32::try_from(index).ok()
}
