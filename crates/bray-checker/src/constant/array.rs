use bray_symbols::IntegerConstant;

/// A semantic failure in a fixed-array length value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrayLengthError {
    /// The length is zero or negative.
    NotPositive,
}

/// Checks a closed integer value against the fixed-array length contract.
pub fn check_array_length(value: &IntegerConstant) -> Result<(), ArrayLengthError> {
    if !value.is_positive() {
        return Err(ArrayLengthError::NotPositive);
    }

    Ok(())
}

pub(super) fn slice_elements<T>(
    elements: &[T],
    lower: Option<usize>,
    upper: Option<usize>,
) -> Option<&[T]> {
    elements.get(lower.unwrap_or(0)..upper.unwrap_or(elements.len()))
}

#[cfg(test)]
mod tests {
    use bray_symbols::{IntegerConstant, IntegerSign};

    use super::{ArrayLengthError, check_array_length};

    #[test]
    fn array_lengths_must_be_positive() {
        let positive = IntegerConstant::new(IntegerSign::NonNegative, [1]);
        let zero = IntegerConstant::new(IntegerSign::NonNegative, []);
        let negative = IntegerConstant::new(IntegerSign::Negative, [1]);

        assert_eq!(check_array_length(&positive), Ok(()));

        assert_eq!(
            check_array_length(&zero),
            Err(ArrayLengthError::NotPositive)
        );

        assert_eq!(
            check_array_length(&negative),
            Err(ArrayLengthError::NotPositive)
        );
    }
}
