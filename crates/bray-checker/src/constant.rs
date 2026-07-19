mod evaluation;
mod input;
mod limits;
mod literal;

pub(crate) use evaluation::evaluate_constant;
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub use limits::ConstantEvaluationLimits;
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
