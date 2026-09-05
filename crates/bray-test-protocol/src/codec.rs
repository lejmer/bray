//! Canonical bounded encodings for catalog artifacts and native host messages.

mod catalog;
mod host;
mod support;

pub use catalog::{TestCatalogDigest, decode_test_catalog, encode_test_catalog};
pub use host::{
    read_host_command, read_host_control, read_host_result, write_host_command, write_host_control,
    write_host_result,
};
pub use support::{TestProtocolError, protocol_version};
