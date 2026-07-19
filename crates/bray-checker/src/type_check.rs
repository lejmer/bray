mod check;
mod constraints;
mod dependencies;
mod inference;
mod input;
mod literal;
mod propagation;
mod session;

pub(crate) use check::check_expression_types;
pub use input::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
