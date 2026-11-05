mod callbacks;
pub(crate) mod native;
mod ownership;

pub(crate) use callbacks::{NativeCallback, Resources};
pub(crate) use ownership::{Event, EventRegistration};
