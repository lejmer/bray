mod array;
mod call;
mod conversion;
mod evaluation;
mod floating;
mod input;
mod integer;
mod limits;
mod literal;
mod operation;
mod template;

pub use array::{ArrayLengthError, check_array_length};
pub use call::{ConstantCallRequest, ConstantCallResolution, ConstantCallResolver};
pub(crate) use evaluation::check_constant_term;
pub(crate) use evaluation::evaluate_constant;
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub use limits::ConstantEvaluationLimits;
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
pub use template::{
    CheckedConstantTerms, CheckedConstantTermsBuildError, resolve_callable_signature_template,
    resolve_type_expression_template,
};
