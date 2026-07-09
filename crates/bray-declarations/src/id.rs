macro_rules! define_id {
    ($(#[$meta:meta])* pub struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u32);

        impl $name {
            /// Creates an ID from its raw stable value.
            pub const fn new(raw: u32) -> Self {
                Self(raw)
            }

            /// Returns the raw stable value.
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// Returns this ID as a collection index.
            pub fn to_index(self) -> Option<usize> {
                usize::try_from(self.0).ok()
            }

            pub(crate) fn from_index(index: usize) -> Self {
                match u32::try_from(index) {
                    Ok(raw) => Self(raw),
                    Err(error) => {
                        panic!("{} index should fit in u32: {error:?}", stringify!($name));
                    }
                }
            }
        }

        impl From<$name> for u32 {
            fn from(id: $name) -> Self {
                id.raw()
            }
        }
    };
}

define_id! {
    /// Stable identity for one discovered syntax declaration.
    pub struct DeclarationId;
}

define_id! {
    /// Stable identity for one declaration container.
    pub struct ContainerId;
}

define_id! {
    /// Stable identity for one syntax contribution to a partial module.
    pub struct ModulePartId;
}

#[cfg(test)]
mod tests {
    use super::{ContainerId, DeclarationId, ModulePartId};

    #[test]
    fn declaration_ids_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<DeclarationId>(), size_of::<u32>());

        let id = DeclarationId::new(7);
        let copied = id;

        assert_eq!(copied.raw(), 7);
        assert_eq!(copied.to_index(), Some(7));
        assert_eq!(DeclarationId::from_index(8).raw(), 8);
    }

    #[test]
    fn container_ids_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<ContainerId>(), size_of::<u32>());

        let id = ContainerId::new(3);
        let copied = id;

        assert_eq!(copied.raw(), 3);
        assert_eq!(copied.to_index(), Some(3));
        assert_eq!(ContainerId::from_index(4).raw(), 4);
    }

    #[test]
    fn module_part_ids_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<ModulePartId>(), size_of::<u32>());

        let id = ModulePartId::new(11);
        let copied = id;

        assert_eq!(copied.raw(), 11);
        assert_eq!(copied.to_index(), Some(11));
        assert_eq!(ModulePartId::from_index(12).raw(), 12);
    }
}
