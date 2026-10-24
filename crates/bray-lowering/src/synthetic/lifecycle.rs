mod aggregate;
mod body;
mod calls;
mod destruction;
mod expansion;
mod frame;
mod outcome;
mod outgoing;
mod representation;
mod storage;
mod task;

pub use body::lower_lifecycle;
pub use expansion::expand_lifecycle_continuations;
