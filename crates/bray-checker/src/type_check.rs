mod check;
mod constraints;
mod dependencies;
mod inference;
mod input;
mod literal;
mod propagation;
mod session;

pub(crate) use check::diagnostic_type;
pub(crate) use check::{check_expression_types, finish_expression_types_with_deferred};
pub use input::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub(crate) use session::{ExpressionTypeSession, SessionProgress};
