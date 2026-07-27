use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::ConstantValueKind;

use super::floating::{convert_real, integer_to_real};
use super::operation::ConstantOperationError;

pub(super) fn convert_scalar(
    value: &ConstantValueKind,
    target: RepresentationRole,
    target_width: impl FnOnce() -> std::num::NonZeroU16,
) -> Result<ConstantValueKind, ConstantOperationError> {
    match value {
        ConstantValueKind::Integer(value) => {
            if let Some(representation) = target.integer_representation() {
                if !super::integer::fits_integer_representation(value, representation, target_width)
                {
                    return Err(ConstantOperationError::NotRepresentable);
                }

                // Integer constants are immutable, Arc-backed canonical payloads.
                return Ok(ConstantValueKind::Integer(value.clone()));
            }

            integer_to_real(value, target).map(ConstantValueKind::Real)
        }
        ConstantValueKind::Real(value)
            if target.numeric_kind() == Some(NumericRepresentationKind::Real) =>
        {
            convert_real(*value, target).map(ConstantValueKind::Real)
        }
        ConstantValueKind::Complex { real, imaginary }
            if target.numeric_kind() == Some(NumericRepresentationKind::Complex) =>
        {
            let Some(component) = target.complex_component() else {
                return Err(ConstantOperationError::Invalid);
            };

            let real = convert_real(*real, component)?;
            let imaginary = convert_real(*imaginary, component)?;

            Ok(ConstantValueKind::Complex { real, imaginary })
        }
        _ => Err(ConstantOperationError::Invalid),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::num::NonZeroU16;

    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign, RealConstantBits};

    use super::convert_scalar;
    use crate::constant::operation::ConstantOperationError;

    #[test]
    fn conversion_observes_the_target_only_for_target_sized_integers() {
        let observations = Cell::new(0);

        let value =
            ConstantValueKind::Integer(IntegerConstant::new(IntegerSign::NonNegative, [0xff]));

        let fixed = convert_scalar(&value, RepresentationRole::ScalarU16, || {
            observations.set(observations.get() + 1);

            NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN)
        });

        assert!(fixed.is_ok());
        assert_eq!(observations.get(), 0);

        let too_wide = ConstantValueKind::Integer(IntegerConstant::new(
            IntegerSign::NonNegative,
            [1, 0, 0, 0, 0],
        ));

        let target_sized = convert_scalar(&too_wide, RepresentationRole::ScalarUsize, || {
            observations.set(observations.get() + 1);

            NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN)
        });

        assert_eq!(target_sized, Err(ConstantOperationError::NotRepresentable));
        assert_eq!(observations.get(), 1);
    }

    #[test]
    fn complex_widening_converts_both_components() {
        let value = ConstantValueKind::Complex {
            real: RealConstantBits::Binary32(0x3fc0_0000),
            imaginary: RealConstantBits::Binary32(0x4000_0000),
        };

        let result = convert_scalar(&value, RepresentationRole::ScalarC128, || {
            panic!("complex widening must not request target integer width")
        });

        assert_eq!(
            result,
            Ok(ConstantValueKind::Complex {
                real: RealConstantBits::Binary64(0x3ff8_0000_0000_0000),
                imaginary: RealConstantBits::Binary64(0x4000_0000_0000_0000),
            })
        );
    }
}
