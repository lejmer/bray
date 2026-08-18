mod api;
mod evaluator;
mod support;
mod term;

pub use api::{
    evaluate_constant_callable_template, evaluate_constant_definition_template,
    evaluate_generic_constraint_template, evaluate_static_initializer_template,
};
