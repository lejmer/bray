macro_rules! define_template_id {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u32);

        impl $name {
            /// Creates a template-local identity from its compact representation.
            pub const fn new(raw: u32) -> Self {
                Self(raw)
            }

            /// Returns the compact template-local representation.
            pub const fn raw(self) -> u32 {
                self.0
            }

            pub(crate) fn to_index(self) -> Option<usize> {
                usize::try_from(self.0).ok()
            }

            pub(crate) fn try_from_index(index: usize) -> Option<Self> {
                u32::try_from(index).ok().map(Self)
            }
        }
    };
}

define_template_id!(
    CheckedTemplateInputId,
    "Identifies one explicit contextual or generic input within a checked template."
);
define_template_id!(
    CheckedTemplateNodeId,
    "Identifies one operation within a source-independent checked template."
);
define_template_id!(
    CheckedTemplateTemporaryId,
    "Identifies one explicitly materialized temporary within a checked template."
);

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};

    #[test]
    fn template_ids_are_compact_and_category_specific() {
        assert_eq!(size_of::<CheckedTemplateInputId>(), size_of::<u32>());
        assert_eq!(size_of::<CheckedTemplateNodeId>(), size_of::<u32>());
        assert_eq!(size_of::<CheckedTemplateTemporaryId>(), size_of::<u32>());

        assert_eq!(CheckedTemplateInputId::new(4).raw(), 4);
        assert_eq!(CheckedTemplateNodeId::new(4).raw(), 4);
        assert_eq!(CheckedTemplateTemporaryId::new(4).raw(), 4);
    }
}
