use sha2::{Digest as _, Sha256};

use super::super::corpus::ExpectedSideEffects;

pub(in crate::standard_library::performance) fn validate(
    execution: &std::process::Output,
    expected_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
    working_directory: &std::path::Path,
) -> Result<(), String> {
    validate_parts(
        &execution.stdout,
        &execution.stderr,
        expected_sha256,
        expected_side_effects,
        working_directory,
    )
}

pub(in crate::standard_library::performance) fn validate_parts(
    stdout: &[u8],
    stderr: &[u8],
    expected_sha256: &str,
    expected_side_effects: ExpectedSideEffects,
    working_directory: &std::path::Path,
) -> Result<(), String> {
    if !stderr.is_empty() {
        return Err("performance artifact produced unexpected standard error output".to_owned());
    }

    if bray_base::lowercase_hex(&Sha256::digest(stdout)) != expected_sha256 {
        return Err("performance artifact did not produce the corpus-defined output".to_owned());
    }

    match expected_side_effects {
        ExpectedSideEffects::None => {}
        ExpectedSideEffects::AbsentPath(path) if !working_directory.join(path).exists() => {}
        ExpectedSideEffects::AbsentPath(_) => {
            return Err(
                "performance artifact did not clean up its declared file effect".to_owned(),
            );
        }
    }

    Ok(())
}
