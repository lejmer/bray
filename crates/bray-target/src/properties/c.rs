use super::{TargetScalarSupport, TargetScalarKind};

const C_SCALAR_KIND_COUNT: usize = 18;

/// C scalar identities described by the language-defined `target.c` properties.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetCScalarKind {
    /// Plain C `char`.
    Char,
    /// C `signed char`.
    SignedChar,
    /// C `unsigned char`.
    UnsignedChar,
    /// C `short`.
    Short,
    /// C `unsigned short`.
    UnsignedShort,
    /// C `int`.
    Int,
    /// C `unsigned int`.
    UnsignedInt,
    /// C `long`.
    Long,
    /// C `unsigned long`.
    UnsignedLong,
    /// C `long long`.
    LongLong,
    /// C `unsigned long long`.
    UnsignedLongLong,
    /// C `size_t`.
    Size,
    /// C `ptrdiff_t`.
    PointerDifference,
    /// C `wchar_t`.
    WideChar,
    /// C `_Bool`.
    Bool,
    /// C `float`.
    Float,
    /// C `double`.
    Double,
    /// C `long double`.
    LongDouble,
}

impl TargetCScalarKind {
    const fn index(self) -> usize {
        match self {
            Self::Char => 0,
            Self::SignedChar => 1,
            Self::UnsignedChar => 2,
            Self::Short => 3,
            Self::UnsignedShort => 4,
            Self::Int => 5,
            Self::UnsignedInt => 6,
            Self::Long => 7,
            Self::UnsignedLong => 8,
            Self::LongLong => 9,
            Self::UnsignedLongLong => 10,
            Self::Size => 11,
            Self::PointerDifference => 12,
            Self::WideChar => 13,
            Self::Bool => 14,
            Self::Float => 15,
            Self::Double => 16,
            Self::LongDouble => 17,
        }
    }
}

/// Exact Bray scalar mappings for one target's C scalar data model.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetCDataModel {
    mappings: [Option<TargetScalarKind>; C_SCALAR_KIND_COUNT],
}

impl TargetCDataModel {
    /// Creates a C model from the target-dependent scalar choices.
    ///
    /// Returns absence when the choices do not form one of Bray's supported C data models.
    pub const fn try_new(
        char: TargetScalarKind,
        long: TargetScalarKind,
        unsigned_long: TargetScalarKind,
        wide_char: TargetScalarKind,
        long_double: Option<TargetScalarKind>,
    ) -> Option<Self> {
        if !matches!(char, TargetScalarKind::I8 | TargetScalarKind::U8)
            || !matches!(long, TargetScalarKind::I32 | TargetScalarKind::I64)
            || !matches!(unsigned_long, TargetScalarKind::U32 | TargetScalarKind::U64)
            || !matching_long_pair(long, unsigned_long)
            || !matches!(wide_char, TargetScalarKind::I32 | TargetScalarKind::U16)
            || !valid_long_double(long_double)
        {
            return None;
        }

        Some(Self {
            mappings: [
                Some(char),
                Some(TargetScalarKind::I8),
                Some(TargetScalarKind::U8),
                Some(TargetScalarKind::I16),
                Some(TargetScalarKind::U16),
                Some(TargetScalarKind::I32),
                Some(TargetScalarKind::U32),
                Some(long),
                Some(unsigned_long),
                Some(TargetScalarKind::I64),
                Some(TargetScalarKind::U64),
                Some(TargetScalarKind::Usize),
                Some(TargetScalarKind::Isize),
                Some(wide_char),
                Some(TargetScalarKind::Bool),
                Some(TargetScalarKind::R32),
                Some(TargetScalarKind::R64),
                long_double,
            ],
        })
    }

    /// Returns the exact Bray scalar for one C type, or absence when unsupported.
    pub const fn mapping(self, kind: TargetCScalarKind) -> Option<TargetScalarKind> {
        self.mappings[kind.index()]
    }

    pub(crate) fn is_supported_by(self, scalars: TargetScalarSupport) -> bool {
        self.mappings
            .into_iter()
            .flatten()
            .all(|scalar| scalars.supports(scalar))
    }
}

const fn matching_long_pair(long: TargetScalarKind, unsigned_long: TargetScalarKind) -> bool {
    matches!(
        (long, unsigned_long),
        (TargetScalarKind::I32, TargetScalarKind::U32)
            | (TargetScalarKind::I64, TargetScalarKind::U64)
    )
}

const fn valid_long_double(long_double: Option<TargetScalarKind>) -> bool {
    matches!(
        long_double,
        None | Some(TargetScalarKind::R64 | TargetScalarKind::R128)
    )
}

#[cfg(test)]
mod tests {
    use super::{TargetCDataModel, TargetCScalarKind};
    use crate::TargetScalarKind;

    #[test]
    fn c_models_reject_mismatched_signed_and_unsigned_long_widths() {
        assert_eq!(
            TargetCDataModel::try_new(
                TargetScalarKind::I8,
                TargetScalarKind::I32,
                TargetScalarKind::U64,
                TargetScalarKind::U16,
                Some(TargetScalarKind::R64),
            ),
            None
        );
    }

    #[test]
    fn c_models_keep_unsupported_long_double_explicit() {
        let properties = TargetCDataModel::try_new(
            TargetScalarKind::I8,
            TargetScalarKind::I64,
            TargetScalarKind::U64,
            TargetScalarKind::I32,
            None,
        )
        .unwrap_or_else(|| panic!("test C ABI properties must be valid"));

        assert_eq!(
            properties.mapping(TargetCScalarKind::Long),
            Some(TargetScalarKind::I64)
        );

        assert_eq!(properties.mapping(TargetCScalarKind::LongDouble), None);
    }
}
