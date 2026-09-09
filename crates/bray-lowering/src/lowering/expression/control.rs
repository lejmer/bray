mod branch;
mod condition;
mod failure;
mod guard;
mod iteration;
mod join;
mod looping;
mod matching;
mod pattern;
mod propagation;
mod transfer;

pub(in crate::lowering) use propagation::PropagationSource;
