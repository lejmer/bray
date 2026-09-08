mod collect;
mod effects;
mod inherited;
mod observation;

pub(super) use collect::completion_candidates;
pub(super) use observation::{CompletionReceiver, completion_receiver, part_was_moved};
