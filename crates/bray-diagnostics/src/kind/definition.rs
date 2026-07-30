macro_rules! define_diagnostic_kinds {
    ($( $(#[$attribute:meta])* $kind:ident, )+) => {
        /// Locale-neutral category for a compiler diagnostic.
        ///
        /// Variants are grouped by owning phase.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum DiagnosticKind {
            $(
                $(#[$attribute])*
                $kind,
            )+
        }

        impl DiagnosticKind {
            /// Every diagnostic category in declaration order.
            pub const ALL: &'static [Self] = &[
                $(Self::$kind,)+
            ];
        }
    };
}

pub(super) use define_diagnostic_kinds;
