mod boundary;
mod foreign;

pub(super) use foreign::{prepare, translate};

pub(in crate::translation::unit) use boundary::{
    resolve_callback_outcome, resolve_terminal_status,
};
