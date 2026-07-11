macro_rules! define_interface_value_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!("Compact artifact-local reference into the `", stringify!($name), "` table.")]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $name(u32);

            impl $name {
                /// Creates an artifact-local ID from its compact representation.
                pub const fn new(raw: u32) -> Self {
                    Self(raw)
                }

                /// Returns the compact representation.
                pub const fn raw(self) -> u32 {
                    self.0
                }

                pub(crate) fn to_index(self) -> Option<usize> {
                    usize::try_from(self.0).ok()
                }
            }
        )+
    };
}

define_interface_value_ids! {
    InterfaceTypeId,
    InterfaceConstantValueId,
    InterfaceConstantTermId,
    InterfaceGenericSubstitutionId,
    InterfaceTraitApplicationId,
    InterfaceCallableInstanceId,
    InterfaceImplementationInstanceId,
    InterfaceDependencyContractId,
}
