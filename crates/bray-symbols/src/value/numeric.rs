use bray_compiler_known::{
    IntegerRepresentation, NumericRepresentationKind, RepresentationRole, ScalarShape,
};

use super::RealConstantBits;
use crate::{AvailableCompilerKnownSymbols, SemanticValueStore, TypeId};

/// The supported truncation policy for a selected pair of scalar representations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumericTruncation {
    /// Integer truncation retains the low target bits with the established signedness policy.
    Integer {
        /// Source integer representation.
        source: IntegerRepresentation,
        /// Target integer representation.
        target: IntegerRepresentation,
    },
    /// Every source value converts without information loss.
    Lossless,
    /// Real inputs must lie in this ordered interval before rounding toward zero.
    RealToInteger {
        /// Target integer representation.
        target: IntegerRepresentation,
        /// Lower source-format bound.
        minimum: RealConstantBits,
        /// Whether the lower bound itself belongs to the domain.
        minimum_inclusive: bool,
        /// Exclusive upper source-format bound.
        maximum: RealConstantBits,
    },
}

impl NumericTruncation {
    /// Classifies a recognized numeric policy instance with its checked Target, Source parameters.
    pub fn for_instance(
        values: &SemanticValueStore,
        symbols: &AvailableCompilerKnownSymbols,
        callable: crate::CallableInstanceData,
        pointer_width: u16,
    ) -> Option<Self> {
        let substitution = values.generic_substitution_data(callable.substitution());

        let [target, source] = substitution.bindings() else {
            panic!("recognized numeric policy retains its Target and Source type arguments");
        };

        let (crate::GenericArgument::Type(target), crate::GenericArgument::Type(source)) =
            (target.argument(), source.argument())
        else {
            panic!("recognized numeric policy arguments are types");
        };

        Self::for_types(values, symbols, source, target, pointer_width)
    }

    /// Classifies checked source and target types for the selected pointer width.
    pub fn for_types(
        values: &SemanticValueStore,
        symbols: &AvailableCompilerKnownSymbols,
        source: TypeId,
        target: TypeId,
        pointer_width: u16,
    ) -> Option<Self> {
        Self::for_roles(
            symbols.type_representation(values, source)?,
            symbols.type_representation(values, target)?,
            pointer_width,
        )
    }

    /// Derives the supported conversion and its complete source-format input domain.
    pub fn for_roles(
        source: RepresentationRole,
        target: RepresentationRole,
        pointer_width: u16,
    ) -> Option<Self> {
        if let (Some(source), Some(target)) = (
            source.integer_representation(),
            target.integer_representation(),
        ) {
            return Some(Self::Integer { source, target });
        }

        if let (Some((width, precision)), Some(integer)) =
            (source.real_format(), target.integer_representation())
        {
            let (bits, signed) = match integer {
                IntegerRepresentation::Signed(bits) => (bits, true),
                IntegerRepresentation::Unsigned(bits) => (bits, false),
                IntegerRepresentation::TargetSigned => (pointer_width, true),
                IntegerRepresentation::TargetUnsigned => (pointer_width, false),
            };

            let exponent_bits = width - precision;
            let bias = (1_u16 << (exponent_bits - 1)) - 1;
            let infinity = ((1_u128 << exponent_bits) - 1) << (precision - 1);

            let power_bits = |power: u16| {
                if power > bias {
                    infinity
                } else {
                    u128::from(power + bias) << (precision - 1)
                }
            };

            let upper_power = bits - u16::from(signed);
            let maximum = power_bits(upper_power);

            let (minimum, minimum_inclusive) = if signed {
                let power = bits - 1;
                let minimum = power_bits(power);

                if minimum == infinity {
                    (minimum, false)
                } else if power < precision {
                    // -2^power-1 is exact only when the source significand can retain the unit.
                    (minimum | (1_u128 << (precision - 1 - power)), false)
                } else {
                    // No source value between this inclusive bound and its predecessor truncates to the minimum.
                    (minimum, true)
                }
            } else {
                (power_bits(0), false)
            };

            let minimum = minimum | (1_u128 << (width - 1));

            let encode = |bits| {
                RealConstantBits::from_bits(source, bits)
                    .expect("derived truncation bound fits its selected IEEE format")
            };

            return Some(Self::RealToInteger {
                target: integer,
                minimum: encode(minimum),
                minimum_inclusive,
                maximum: encode(maximum),
            });
        }

        if target.numeric_kind() != Some(NumericRepresentationKind::Real) {
            return None;
        }

        let source = ScalarShape::for_role(source, pointer_width)?;
        let target = ScalarShape::for_role(target, pointer_width)?;

        source
            .converts_losslessly_to(target)
            .then_some(Self::Lossless)
    }
}

#[cfg(test)]
mod tests {
    use super::NumericTruncation;
    use bray_compiler_known::{IntegerRepresentation, RepresentationRole};

    #[test]
    fn truncation_type_domains_share_lossless_conversion_rules() {
        for (source, target, expected) in [
            (
                RepresentationRole::ScalarU128,
                RepresentationRole::ScalarI8,
                true,
            ),
            (
                RepresentationRole::ScalarR64,
                RepresentationRole::ScalarU8,
                true,
            ),
            (
                RepresentationRole::ScalarR16,
                RepresentationRole::ScalarR128,
                true,
            ),
            (
                RepresentationRole::ScalarI32,
                RepresentationRole::ScalarR64,
                true,
            ),
            (
                RepresentationRole::ScalarR64,
                RepresentationRole::ScalarR32,
                false,
            ),
            (
                RepresentationRole::ScalarI64,
                RepresentationRole::ScalarR32,
                false,
            ),
            (
                RepresentationRole::ScalarU128,
                RepresentationRole::ScalarR128,
                false,
            ),
            (
                RepresentationRole::ScalarBool,
                RepresentationRole::ScalarBool,
                false,
            ),
            (
                RepresentationRole::ScalarChar,
                RepresentationRole::ScalarU32,
                false,
            ),
            (
                RepresentationRole::ScalarC64,
                RepresentationRole::ScalarR64,
                false,
            ),
        ] {
            assert_eq!(
                NumericTruncation::for_roles(source, target, 64).is_some(),
                expected
            );
        }
    }

    #[test]
    fn real_to_target_integer_bounds_follow_the_selected_pointer_width() {
        for (width, signed, unsigned) in [
            (
                32,
                RepresentationRole::ScalarI32,
                RepresentationRole::ScalarU32,
            ),
            (
                64,
                RepresentationRole::ScalarI64,
                RepresentationRole::ScalarU64,
            ),
            (
                128,
                RepresentationRole::ScalarI128,
                RepresentationRole::ScalarU128,
            ),
        ] {
            for source in [
                RepresentationRole::ScalarR16,
                RepresentationRole::ScalarR32,
                RepresentationRole::ScalarR64,
                RepresentationRole::ScalarR128,
            ] {
                for (target, fixed) in [
                    (RepresentationRole::ScalarIsize, signed),
                    (RepresentationRole::ScalarUsize, unsigned),
                ] {
                    let NumericTruncation::RealToInteger {
                        minimum,
                        minimum_inclusive,
                        maximum,
                        ..
                    } = NumericTruncation::for_roles(source, target, width).unwrap()
                    else {
                        unreachable!()
                    };

                    let NumericTruncation::RealToInteger {
                        minimum: fixed_minimum,
                        minimum_inclusive: fixed_inclusive,
                        maximum: fixed_maximum,
                        ..
                    } = NumericTruncation::for_roles(source, fixed, width).unwrap()
                    else {
                        unreachable!()
                    };

                    assert_eq!(
                        (minimum, minimum_inclusive, maximum),
                        (fixed_minimum, fixed_inclusive, fixed_maximum)
                    );
                }
            }
        }

        assert!(matches!(
            NumericTruncation::for_roles(
                RepresentationRole::ScalarUsize,
                RepresentationRole::ScalarI8,
                32
            ),
            Some(NumericTruncation::Integer {
                source: IntegerRepresentation::TargetUnsigned,
                target: IntegerRepresentation::Signed(8)
            })
        ));
    }
}
