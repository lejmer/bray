mod array;
mod call;
mod conversion;
mod equality;
mod evaluation;
mod floating;
mod input;
mod integer;
mod limits;
mod literal;
mod operation;
mod template;
mod template_evaluation;

pub use array::{ArrayLengthError, check_array_length};
pub use call::{
    ConstantCallRequest, ConstantCallResolution, ConstantCallResolver, ConstantTemplateResolver,
    EvaluatedConstantCall,
};
pub(crate) use equality::constant_values_equal;
pub use evaluation::EvaluatedConstant;
pub(crate) use evaluation::check_constant_term;
pub(crate) use evaluation::{evaluate_constant, evaluate_constant_with_references};
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub(crate) use integer::{fits_integer_representation, integer_to_usize, significant_bits};
pub use limits::{ConstantEvaluationLimits, ConstantEvaluationUsage};
pub(crate) use literal::literal_diagnostic_kind;
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
pub use template::{
    CheckedConstantTerms, CheckedConstantTermsBuildError, resolve_callable_signature_template,
    resolve_type_expression_template,
};
pub use template_evaluation::evaluate_constant_callable_template;
