use bray_compiler_known::RepresentationRole;
use bray_symbols::ConstantValueKind;

use super::operation::ConstantOperationError;

pub(super) fn convert_scalar(
    value: &ConstantValueKind,
    target: RepresentationRole,
    target_width: impl FnOnce() -> std::num::NonZeroU16,
) -> Result<ConstantValueKind, ConstantOperationError> {
    match value {
        ConstantValueKind::Integer(value) => {
            let Some(representation) = target.integer_representation() else {
                return Err(ConstantOperationError::Invalid);
            };

            if !super::integer::fits_integer_representation(value, representation, target_width) {
                return Err(ConstantOperationError::NotRepresentable);
            }

            // Integer constants are immutable, Arc-backed canonical payloads.
            Ok(ConstantValueKind::Integer(value.clone()))
        }
        _ => {
            // TODO(BRA-122): Add selected runtime-format real and complex widening.
            Err(ConstantOperationError::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::num::NonZeroU16;

    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{ConstantValueKind, IntegerConstant, IntegerSign};

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
}
