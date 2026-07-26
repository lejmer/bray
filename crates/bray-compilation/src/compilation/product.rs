mod entry;
mod query;
mod visibility;

pub(in crate::compilation) use visibility::symbol_is_publicly_reachable;
