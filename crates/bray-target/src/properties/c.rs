use super::{TargetForeignAbiContract, TargetScalarKind, TargetScalarSupport};

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
    /// Every C scalar kind in stable target-property order.
    pub const ALL: [Self; C_SCALAR_KIND_COUNT] = [
        Self::Char,
        Self::SignedChar,
        Self::UnsignedChar,
        Self::Short,
        Self::UnsignedShort,
        Self::Int,
        Self::UnsignedInt,
        Self::Long,
        Self::UnsignedLong,
        Self::LongLong,
        Self::UnsignedLongLong,
        Self::Size,
        Self::PointerDifference,
        Self::WideChar,
        Self::Bool,
        Self::Float,
        Self::Double,
        Self::LongDouble,
    ];

    /// Returns the stable target-property spelling for this C scalar kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Char => "char",
            Self::SignedChar => "signed_char",
            Self::UnsignedChar => "unsigned_char",
            Self::Short => "short",
            Self::UnsignedShort => "unsigned_short",
            Self::Int => "int",
            Self::UnsignedInt => "unsigned_int",
            Self::Long => "long",
            Self::UnsignedLong => "unsigned_long",
            Self::LongLong => "long_long",
            Self::UnsignedLongLong => "unsigned_long_long",
            Self::Size => "size",
            Self::PointerDifference => "pointer_difference",
            Self::WideChar => "wide_char",
            Self::Bool => "bool",
            Self::Float => "float",
            Self::Double => "double",
            Self::LongDouble => "long_double",
        }
    }

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
    /// Creates a C model from independently available exact scalar mappings.
    ///
    /// Omitted kinds are represented by the corresponding `"unavailable"` target property.
    /// Returns absence for a duplicate kind or a representation invalid for that C scalar.
    pub const fn try_new(mappings: &[(TargetCScalarKind, TargetScalarKind)]) -> Option<Self> {
        let mut model = Self {
            mappings: [None; C_SCALAR_KIND_COUNT],
        };

        let mut index = 0;

        while index < mappings.len() {
            let (kind, scalar) = mappings[index];

            let mapping_index = kind.index();

            if model.mappings[mapping_index].is_some() || !valid_mapping(kind, scalar) {
                return None;
            }

            model.mappings[mapping_index] = Some(scalar);
            index += 1;
        }

        let long = model.mapping(TargetCScalarKind::Long);
        let unsigned_long = model.mapping(TargetCScalarKind::UnsignedLong);

        if let (Some(long), Some(unsigned_long)) = (long, unsigned_long)
            && !matching_long_pair(long, unsigned_long)
        {
            return None;
        }

        Some(model)
    }

    /// Creates the complete C scalar registry shared by Bray's native target profiles.
    pub fn try_native(
        char: TargetScalarKind,
        long: TargetScalarKind,
        unsigned_long: TargetScalarKind,
        wide_char: TargetScalarKind,
        long_double: Option<TargetScalarKind>,
    ) -> Option<Self> {
        let mappings = [
            (TargetCScalarKind::Char, char),
            (TargetCScalarKind::SignedChar, TargetScalarKind::I8),
            (TargetCScalarKind::UnsignedChar, TargetScalarKind::U8),
            (TargetCScalarKind::Short, TargetScalarKind::I16),
            (TargetCScalarKind::UnsignedShort, TargetScalarKind::U16),
            (TargetCScalarKind::Int, TargetScalarKind::I32),
            (TargetCScalarKind::UnsignedInt, TargetScalarKind::U32),
            (TargetCScalarKind::Long, long),
            (TargetCScalarKind::UnsignedLong, unsigned_long),
            (TargetCScalarKind::LongLong, TargetScalarKind::I64),
            (TargetCScalarKind::UnsignedLongLong, TargetScalarKind::U64),
            (TargetCScalarKind::Size, TargetScalarKind::Usize),
            (
                TargetCScalarKind::PointerDifference,
                TargetScalarKind::Isize,
            ),
            (TargetCScalarKind::WideChar, wide_char),
            (TargetCScalarKind::Bool, TargetScalarKind::Bool),
            (TargetCScalarKind::Float, TargetScalarKind::R32),
            (TargetCScalarKind::Double, TargetScalarKind::R64),
        ];

        let mut model = Self::try_new(&mappings)?;

        if let Some(long_double) = long_double {
            if !valid_mapping(TargetCScalarKind::LongDouble, long_double) {
                return None;
            }

            model.mappings[TargetCScalarKind::LongDouble.index()] = Some(long_double);
        }

        Some(model)
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

    pub(crate) fn is_supported_by_c_abi(self, contract: TargetForeignAbiContract) -> bool {
        self.is_empty()
            || contract.transparent_layout()
                && self
                    .mappings
                    .into_iter()
                    .flatten()
                    .all(|scalar| contract.scalars().supports(scalar))
    }

    pub(crate) fn is_empty(self) -> bool {
        self.mappings.into_iter().all(|mapping| mapping.is_none())
    }
}

const fn valid_mapping(kind: TargetCScalarKind, scalar: TargetScalarKind) -> bool {
    match kind {
        TargetCScalarKind::Char => matches!(scalar, TargetScalarKind::I8 | TargetScalarKind::U8),
        TargetCScalarKind::SignedChar => matches!(scalar, TargetScalarKind::I8),
        TargetCScalarKind::UnsignedChar => matches!(scalar, TargetScalarKind::U8),
        TargetCScalarKind::Short => matches!(scalar, TargetScalarKind::I16),
        TargetCScalarKind::UnsignedShort => matches!(scalar, TargetScalarKind::U16),
        TargetCScalarKind::Int => matches!(scalar, TargetScalarKind::I32),
        TargetCScalarKind::UnsignedInt => matches!(scalar, TargetScalarKind::U32),
        TargetCScalarKind::Long => matches!(scalar, TargetScalarKind::I32 | TargetScalarKind::I64),
        TargetCScalarKind::UnsignedLong => {
            matches!(scalar, TargetScalarKind::U32 | TargetScalarKind::U64)
        }
        TargetCScalarKind::LongLong => matches!(scalar, TargetScalarKind::I64),
        TargetCScalarKind::UnsignedLongLong => matches!(scalar, TargetScalarKind::U64),
        TargetCScalarKind::Size => matches!(scalar, TargetScalarKind::Usize),
        TargetCScalarKind::PointerDifference => matches!(scalar, TargetScalarKind::Isize),
        TargetCScalarKind::WideChar => {
            matches!(scalar, TargetScalarKind::I32 | TargetScalarKind::U16)
        }
        TargetCScalarKind::Bool => matches!(scalar, TargetScalarKind::Bool),
        TargetCScalarKind::Float => matches!(scalar, TargetScalarKind::R32),
        TargetCScalarKind::Double => matches!(scalar, TargetScalarKind::R64),
        TargetCScalarKind::LongDouble => {
            matches!(scalar, TargetScalarKind::R64 | TargetScalarKind::R128)
        }
    }
}

const fn matching_long_pair(long: TargetScalarKind, unsigned_long: TargetScalarKind) -> bool {
    matches!(
        (long, unsigned_long),
        (TargetScalarKind::I32, TargetScalarKind::U32)
            | (TargetScalarKind::I64, TargetScalarKind::U64)
    )
}

#[cfg(test)]
mod tests {
    use super::{TargetCDataModel, TargetCScalarKind};
    use crate::TargetScalarKind;

    #[test]
    fn c_models_reject_mismatched_signed_and_unsigned_long_widths() {
        assert_eq!(
            TargetCDataModel::try_new(&[
                (TargetCScalarKind::Long, TargetScalarKind::I32),
                (TargetCScalarKind::UnsignedLong, TargetScalarKind::U64),
            ]),
            None
        );
    }

    #[test]
    fn c_models_keep_each_unavailable_mapping_independent() {
        let properties = TargetCDataModel::try_new(&[
            (TargetCScalarKind::SignedChar, TargetScalarKind::I8),
            (TargetCScalarKind::Long, TargetScalarKind::I64),
            (TargetCScalarKind::UnsignedLong, TargetScalarKind::U64),
        ])
        .unwrap_or_else(|| panic!("test C ABI properties must be valid"));

        assert_eq!(
            properties.mapping(TargetCScalarKind::Long),
            Some(TargetScalarKind::I64)
        );

        assert_eq!(properties.mapping(TargetCScalarKind::LongDouble), None);
        assert_eq!(properties.mapping(TargetCScalarKind::UnsignedChar), None);
    }

    #[test]
    fn c_models_reject_duplicate_and_inexact_mappings() {
        assert_eq!(
            TargetCDataModel::try_new(&[
                (TargetCScalarKind::Int, TargetScalarKind::I32),
                (TargetCScalarKind::Int, TargetScalarKind::I32),
            ]),
            None
        );

        assert_eq!(
            TargetCDataModel::try_new(&[(TargetCScalarKind::Int, TargetScalarKind::I64)]),
            None
        );
    }
}
