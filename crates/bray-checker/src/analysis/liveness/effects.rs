mod core;
mod retention;
mod subjects;

pub(crate) use core::OperationEffects;
pub(in crate::analysis) use subjects::dependency_subjects;
