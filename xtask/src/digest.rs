use std::fs;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest as _, Sha256};

pub(crate) fn sha256(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];

    loop {
        let length = file.read(&mut buffer)?;

        if length == 0 {
            break;
        }

        hasher.update(&buffer[..length]);
    }

    Ok(hasher.finalize().into())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{hex, sha256};

    #[test]
    fn file_digest_uses_lowercase_sha256() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("temporary directory must be available");
        };

        let path = directory.path().join("input");

        if let Err(error) = fs::write(&path, b"bray") {
            panic!("test input must be written: {error}");
        }

        let Ok(digest) = sha256(&path) else {
            panic!("test input must be readable");
        };

        assert_eq!(
            hex(&digest),
            "877fba9141ff2930db14e97816a1f9ac5e5788b77a13daf330a456f8681279b9"
        );
    }
}
