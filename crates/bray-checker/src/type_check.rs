mod canonical;
mod check;
mod constraints;
mod facts;
mod inference;
mod input;

pub(crate) use check::check_expression_types;
pub use facts::{
    ExpressionTypeCheckResult, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
};
pub use input::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
