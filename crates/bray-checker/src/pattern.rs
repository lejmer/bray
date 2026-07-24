mod check;
mod compatibility;
mod coverage;
mod input;

pub(crate) use check::check_patterns;
pub use input::{
    GuardConstantEvidence, IterationPatternType, PatternCheckInput, PatternConstantEvidence,
};
