//! Bray Tack, Bray's build tool and toolchain frontend.

#![forbid(unsafe_code)]

mod driver;
mod tack;
#[cfg(test)]
mod test_support;

pub use driver::run;
pub use tack::{
    TackCliError, TackCommandKind, TackInvocation, TackRunResult, run_tack, run_tack_result,
};
