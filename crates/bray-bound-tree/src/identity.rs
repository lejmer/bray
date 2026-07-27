macro_rules! define_unit_scoped_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name {
            unit: crate::BoundUnitId,
            slot: u32,
        }

        impl $name {
            /// Returns the checked semantic unit that owns this identity.
            pub const fn unit(self) -> crate::BoundUnitId {
                self.unit
            }

            /// Returns this identity's unit-local allocation ordinal.
            pub const fn ordinal(self) -> u32 {
                self.slot
            }

            #[cfg(test)]
            pub(crate) const fn from_slot(unit: crate::BoundUnitId, slot: u32) -> Self {
                Self { unit, slot }
            }
        }
    };
}

pub(crate) use define_unit_scoped_id;
