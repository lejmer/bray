mod context;
mod host;
mod native;

pub use context::{CancellationContext, CancellationShield};
pub(crate) use host::{CancellationWakeRegistration, CancellationWakeRegistrationError};
