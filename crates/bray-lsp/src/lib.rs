//! Bray language-server protocol and incremental editor services.

#![forbid(unsafe_code)]

mod model;
mod protocol;
mod query;
mod server;
#[cfg(test)]
mod test_support;
mod workspace;

pub use server::LanguageServer;
