mod boundary;
mod foreign;

pub(super) use foreign::{prepare, translate};

pub(in crate::translation::unit) use boundary::{
    consume_owned_report, resolve_callback_outcome, resolve_terminal_status,
};
