macro_rules! define_interface_ids {
    ($($name:ident, $documentation:literal;)+) => {
        $(
            #[doc = $documentation]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $name(u32);

            impl $name {
                /// Number of distinct values in this compact interface identity domain.
                pub const CAPACITY: u64 = 4_294_967_296;

                /// Creates an ID from its compact numeric representation.
                pub const fn new(raw: u32) -> Self {
                    Self(raw)
                }

                /// Returns the compact numeric representation.
                pub const fn raw(self) -> u32 {
                    self.0
                }

                /// Converts this ID to a checked collection index for the current target.
                pub fn to_index(self) -> Option<usize> {
                    usize::try_from(self.0).ok()
                }

                /// Creates an ID when the collection index fits the compact representation.
                pub fn try_from_index(index: usize) -> Option<Self> {
                    u32::try_from(index).ok().map(Self)
                }
            }
        )+
    };
}

define_interface_ids! {
    InterfaceSymbolId,
    "A compact reference into one compiled package interface's symbol identity table.";
    InterfaceSupportEntityId,
    "A compact reference into one compiled package interface's private support-entity table.";
    ImportedInterfaceId,
    "A compilation-local handle for one loaded and validated package interface.";
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{ImportedInterfaceId, InterfaceSupportEntityId, InterfaceSymbolId};

    #[test]
    fn interface_ids_are_compact_and_checked() {
        assert_eq!(size_of::<InterfaceSymbolId>(), size_of::<u32>());
        assert_eq!(size_of::<InterfaceSupportEntityId>(), size_of::<u32>());
        assert_eq!(size_of::<ImportedInterfaceId>(), size_of::<u32>());

        let Some(id) = InterfaceSymbolId::try_from_index(17) else {
            panic!("small interface symbol index must fit in u32");
        };

        assert_eq!(id.raw(), 17);
        assert_eq!(id.to_index(), Some(17));

        if usize::BITS > u32::BITS {
            assert_eq!(
                InterfaceSymbolId::try_from_index((u32::MAX as usize) + 1),
                None
            );
        }
    }
}
