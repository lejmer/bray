use std::hash::Hasher as _;

use bray_base::{StableDigestHasher, lowercase_hex};

pub(crate) fn string_constant_name(text: &str) -> String {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.string.utf8.without-terminator\0");
    hasher.write_u128(text.len() as u128);
    hasher.write(text.as_bytes());

    let digest = hasher.finalize();

    format!("bray.constant.string.{}", lowercase_hex(&digest))
}
