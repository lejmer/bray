mod adaptation;
mod evaluation;
mod input;
mod limits;
mod literal;

pub use adaptation::LiteralAdaptationInput;
pub(crate) use adaptation::adapt_literals;
pub(crate) use evaluation::evaluate_constant;
pub use input::{ConstantEvaluationInput, ConstantReferenceResolution};
pub use limits::ConstantEvaluationLimits;
