mod callbacks;
pub(crate) mod native;
mod ownership;

pub(crate) use callbacks::EventWake;
pub(crate) use ownership::{Event, EventRegistration};
