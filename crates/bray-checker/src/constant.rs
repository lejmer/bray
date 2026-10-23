mod array;
mod call;
mod conversion;
mod diagnostic;
pub use diagnostic::constant_eligibility_diagnostic;
mod equality;
mod evaluation;
pub(crate) mod floating;
mod input;
mod integer;
mod intrinsic;
mod limits;
mod literal;
mod machine;
mod materialization;
mod operation;
pub(crate) mod operator;
mod template;
mod template_evaluation;

pub use array::{ArrayLengthError, check_array_length};
pub use call::{
    ConstantCallContract, ConstantCallRequest, ConstantCallResolution, ConstantCallResolver,
    ConstantCallTarget, ConstantTemplateResolver, EvaluatedConstantCall,
    constant_invocation_targets,
};
pub(crate) use equality::constant_values_equal;
pub use evaluation::EvaluatedConstant;
pub(crate) use evaluation::check_constant_term;
pub(crate) use evaluation::{evaluate_constant, evaluate_constant_with_references};
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub(crate) use integer::{
    fits_integer_representation, from_big_integer, integer_to_usize, to_big_integer,
};
pub use limits::{ConstantEvaluationLimits, ConstantEvaluationUsage};
pub use literal::{ConstantLiteralError, check_constant_literal, normalize_integer_literal};
pub(crate) use literal::{
    check_byte_string_literal, check_negated_integer_operand_literal, literal_diagnostic_kind,
    parse_byte_string,
};
pub use machine::{
    fold_machine_integer_binary, fold_machine_integer_truncate, fold_machine_integer_unary,
};
pub(crate) use operation::fold_binary;
pub(crate) use template::checked_substituted_type;
pub use template::{
    CheckedConstantTerms, CheckedConstantTermsBuildError, resolve_callable_signature_template,
    resolve_trait_application_template, resolve_type_expression_template,
};
pub use template_evaluation::{
    evaluate_array_length, evaluate_constant_callable_template,
    evaluate_constant_definition_template, evaluate_generic_constraint_template,
    evaluate_static_initializer_template,
};

pub use intrinsic::evaluate_constant_intrinsic;
