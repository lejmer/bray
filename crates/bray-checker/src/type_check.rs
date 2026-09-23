mod array;
mod cardinality;
mod check;
mod constraints;
mod dependencies;
mod inference;
mod input;
mod literal;
mod propagation;
mod region;
mod session;

pub(crate) use check::diagnostic_type;
pub(crate) use check::{check_expression_types, finish_expression_types_with_deferred};
pub(crate) use constraints::intrinsic_representation_role;
pub use input::{ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput};
pub(crate) use literal::numeric_literal_accepts_type;
pub(crate) use session::{ExpressionTypeSession, SessionProgress};
