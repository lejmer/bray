macro_rules! define_catalog_enum {
    (
        $(#[$enum_meta:meta])*
        $visibility:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $spelling:literal,
            )+
        }
    ) => {
        $(#[$enum_meta])*
        $visibility enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// Every closed value in canonical declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            #[cfg(any(test, feature = "generation"))]
            pub(crate) fn from_catalog_spelling(spelling: &str) -> Option<Self> {
                match spelling {
                    $($spelling => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}
