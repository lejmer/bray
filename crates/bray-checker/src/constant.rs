mod array;
mod call;
mod conversion;
mod diagnostic;
mod equality;
mod evaluation;
mod floating;
mod input;
mod integer;
mod limits;
mod literal;
mod operation;
pub(crate) mod pattern;
pub(crate) mod shape;
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
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
pub(crate) use literal::{check_negated_integer_operand_literal, literal_diagnostic_kind};
pub use template::{
    CheckedConstantTerms, CheckedConstantTermsBuildError, resolve_callable_signature_template,
    resolve_trait_application_template, resolve_type_expression_template,
};
pub use template_evaluation::{
    evaluate_constant_callable_template, evaluate_constant_definition_template,
    evaluate_generic_constraint_template, evaluate_static_initializer_template,
};
