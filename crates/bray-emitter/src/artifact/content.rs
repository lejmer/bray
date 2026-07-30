use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;

use bray_base::Cancellation;
use bray_codegen::{
    ArtifactContent, ArtifactContentSource, ArtifactDigest, ArtifactDigestAlgorithm,
};
use sha2::{Digest as _, Sha256};

const COPY_BUFFER_LEN: usize = 64 * 1024;

pub(crate) enum ContentReader<'content> {
    Memory(Cursor<&'content [u8]>),
    File(File),
}

impl Read for ContentReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Memory(reader) => reader.read(buffer),
            Self::File(reader) => reader.read(buffer),
        }
    }
}

pub(crate) fn open_content(
    content: &ArtifactContent,
) -> Result<ContentReader<'_>, io::ErrorKind> {
    match content.source() {
        ArtifactContentSource::Memory(bytes) => Ok(ContentReader::Memory(Cursor::new(bytes))),
        ArtifactContentSource::CompilerSpool(spool) => spool
            .open_reader()
            .map(ContentReader::File)
            .map_err(|error| error.source().map_or(io::ErrorKind::Other, io::Error::kind)),
    }
}

pub(crate) fn open_linked_staging(path: &Path) -> Result<ContentReader<'static>, io::ErrorKind> {
    File::open(path)
        .map(ContentReader::File)
        .map_err(|error| error.kind())
}

pub(crate) fn validate_content(
    content: &ArtifactContent,
    expected: Option<&ArtifactDigest>,
    cancellation: &dyn Cancellation,
) -> Result<ArtifactDigest, ContentValidationError> {
    if cancellation.is_cancelled() {
        return Err(ContentValidationError::Cancelled);
    }

    let reader = open_content(content).map_err(ContentValidationError::Read)?;

    validate_reader(reader, content.byte_len(), expected, cancellation)
}

pub(crate) fn validate_staged_content(
    path: &Path,
    expected_byte_len: u64,
    expected_digest: Option<&ArtifactDigest>,
    cancellation: &dyn Cancellation,
) -> Result<ArtifactDigest, ContentValidationError> {
    if cancellation.is_cancelled() {
        return Err(ContentValidationError::Cancelled);
    }

    let reader = File::open(path).map_err(|error| ContentValidationError::Read(error.kind()))?;

    validate_reader(reader, expected_byte_len, expected_digest, cancellation)
}

fn validate_reader(
    mut reader: impl Read,
    expected_byte_len: u64,
    expected_digest: Option<&ArtifactDigest>,
    cancellation: &dyn Cancellation,
) -> Result<ArtifactDigest, ContentValidationError> {
    let mut canonical = blake3::Hasher::new();

    let mut expected_hasher =
        expected_digest.map(|digest| ExpectedDigestHasher::new(digest.algorithm()));

    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; COPY_BUFFER_LEN];

    loop {
        if cancellation.is_cancelled() {
            return Err(ContentValidationError::Cancelled);
        }

        let read = reader
            .read(&mut buffer)
            .map_err(|error| ContentValidationError::Read(error.kind()))?;

        if read == 0 {
            break;
        }

        let bytes = &buffer[..read];

        canonical.update(bytes);

        if let Some(hasher) = &mut expected_hasher {
            hasher.update(bytes);
        }

        let Ok(read) = u64::try_from(read) else {
            return Err(ContentValidationError::LengthOverflow);
        };

        let Some(next_byte_len) = byte_len.checked_add(read) else {
            return Err(ContentValidationError::LengthOverflow);
        };

        byte_len = next_byte_len;
    }

    if byte_len != expected_byte_len {
        return Err(ContentValidationError::LengthMismatch {
            expected: expected_byte_len,
            actual: byte_len,
        });
    }

    if let (Some(expected), Some(hasher)) = (expected_digest, expected_hasher) {
        let actual_bytes = hasher.finish();

        if actual_bytes.as_slice() != expected.bytes() {
            let Some(actual) = ArtifactDigest::try_new(expected.algorithm(), actual_bytes) else {
                return Err(ContentValidationError::DigestConstruction);
            };

            return Err(ContentValidationError::DigestMismatch {
                // The error must own the declared digest after the contribution borrow ends.
                expected: expected.clone(),
                actual,
            });
        }
    }

    let bytes = *canonical.finalize().as_bytes();

    ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, bytes)
        .ok_or(ContentValidationError::DigestConstruction)
}

enum ExpectedDigestHasher {
    Blake3(Box<blake3::Hasher>),
    Sha256(Sha256),
}

impl ExpectedDigestHasher {
    fn new(algorithm: ArtifactDigestAlgorithm) -> Self {
        match algorithm {
            ArtifactDigestAlgorithm::Blake3 => Self::Blake3(Box::new(blake3::Hasher::new())),
            ArtifactDigestAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Blake3(hasher) => {
                hasher.update(bytes);
            }
            Self::Sha256(hasher) => hasher.update(bytes),
        }
    }

    fn finish(self) -> [u8; 32] {
        match self {
            Self::Blake3(hasher) => *hasher.finalize().as_bytes(),
            Self::Sha256(hasher) => hasher.finalize().into(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ContentValidationError {
    Cancelled,
    Read(io::ErrorKind),
    LengthMismatch {
        expected: u64,
        actual: u64,
    },
    DigestMismatch {
        expected: ArtifactDigest,
        actual: ArtifactDigest,
    },
    LengthOverflow,
    DigestConstruction,
}

#[cfg(test)]
mod tests {
    use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
    use sha2::{Digest as _, Sha256};

    use super::validate_content;

    #[test]
    fn validation_accepts_sha256_and_records_canonical_blake3() {
        let bytes = b"artifact content";

        let Ok(content) = ArtifactContent::try_memory(bytes.as_slice()) else {
            panic!("test artifact content must be valid");
        };

        let Some(expected) =
            ArtifactDigest::try_new(ArtifactDigestAlgorithm::Sha256, Sha256::digest(bytes))
        else {
            panic!("test SHA-256 digest must be valid");
        };

        let Ok(actual) = validate_content(&content, Some(&expected), &never_cancelled) else {
            panic!("matching content digest must validate");
        };

        assert_eq!(actual.algorithm(), ArtifactDigestAlgorithm::Blake3);
        assert_eq!(actual.bytes(), blake3::hash(bytes).as_bytes());
    }

    fn never_cancelled() -> bool {
        false
    }
}
