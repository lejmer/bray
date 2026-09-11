mod abandonment;
mod aggregate;
mod asynchronous;
mod body;
mod buffer;
mod calls;
mod expansion;
mod future;
mod incident;
mod outcome;
mod representation;
mod storage;
mod support;
mod task;

pub use body::lower_lifecycle;

pub(in crate::synthetic) use expansion::LifecycleExpansion;
