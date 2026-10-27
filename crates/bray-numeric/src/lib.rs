//! Deterministic complex arithmetic over the language's IEEE component formats.

mod complex;
mod format;
mod power;

pub use complex::{divide, multiply};
pub use format::BinaryFormat;
pub use power::power;
