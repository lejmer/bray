mod check;
mod compatibility;
mod coverage;
mod input;
mod source;

pub(crate) use check::check_patterns;
pub use input::{
    GuardConstantEvidence, PatternCheckInput, PatternConstantEvidence, PatternSubjectType,
};
pub(crate) use source::{binding_subjects, immutable_initializers};
