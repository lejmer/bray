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
mod type_expression;

#[cfg(test)]
mod test_support;

pub use candidate::bind_expression_candidates;
pub(crate) use contract::{callable_normal_completion_has_value, push_contract_scope};
pub use directive::{bind_callable_type_directives, bind_directive_template};
pub(crate) use error::{BindingError, BindingResult};
pub(crate) use expression::ExpressionBinder;
pub use type_expression::{
    CallableTypeQualifiers, TypeExpressionBinder, TypeExpressionScope, TypeParameterBinding,
    bind_callable_abi,
};

#[cfg(test)]
pub(crate) use test_support::binder_and_block;
