mod arrays;
mod asynchronous;
mod block;
mod cleanup;
mod error;
mod expression;
mod initialization;
mod inputs;
mod lowerer;
mod outgoing;
mod owned;
mod parts;
mod projection;
mod replacement;
mod representation;
mod scoped;

pub use error::LoweringError;
pub use lowerer::lower_unit;
