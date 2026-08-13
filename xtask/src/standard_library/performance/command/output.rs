use sha2::{Digest as _, Sha256};

pub(in crate::standard_library::performance) fn validate(
    output: &[u8],
    expected_sha256: &str,
) -> Result<(), String> {
    if bray_base::lowercase_hex(&Sha256::digest(output)) != expected_sha256 {
        return Err("performance artifact did not produce the corpus-defined output".to_owned());
    }

    Ok(())
}
