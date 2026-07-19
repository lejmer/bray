mod array;
mod evaluation;
mod input;
mod limits;
mod literal;

pub use array::{ArrayLengthError, check_array_length};
pub(crate) use evaluation::evaluate_constant;
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub use limits::ConstantEvaluationLimits;
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
