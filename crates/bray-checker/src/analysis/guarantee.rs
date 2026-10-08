mod check;
mod cleanup;
mod completion;
mod operation;
mod postcondition;
mod state;
mod trusted;

pub use check::check_execution_candidate;
pub(crate) use trusted::{check_trusted_completion, collect_trusted_memory_evidence};
mod flow;
