mod anonymous;
mod binder;
mod block;
mod candidate;
mod contract;
mod directive;
mod error;
mod expression;
mod name;
mod pattern;
mod transaction;
mod type_expression;

#[cfg(test)]
mod test_support;

pub use candidate::{
    bind_expression_candidates, bind_member_callable_template, qualified_union_variant,
};
pub(crate) use contract::{callable_normal_completion_has_value, push_contract_scope};
pub use directive::{
    bind_callable_type_directives, bind_directive_template, malformed_directive_argument_diagnostic,
};
pub use error::BindingError;
pub(crate) use error::BindingResult;
pub(crate) use expression::ExpressionBinder;
pub use type_expression::{
    CallableTypeQualifiers, TypeExpressionBinder, TypeExpressionImports, TypeExpressionScope,
    TypeParameterBinding, bind_callable_abi, callable_execution_properties,
};

#[cfg(test)]
pub(crate) use test_support::binder_and_block;
