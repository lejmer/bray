use std::fmt;
use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bray_base::StableDigestHasher;

static NEXT_SPOOL_ID: AtomicU64 = AtomicU64::new(0);
const SPOOL_CREATE_ATTEMPTS: usize = 128;

/// Compiler operation that failed while owning an immutable artifact spool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactSpoolOperation {
    /// Create compiler-private spool storage.
    Create,
    /// Flush pending writes.
    Flush,
    /// Read final spool metadata.
    Metadata,
    /// Reopen the finalized spool through read-only access.
    OpenReader,
    /// Finalize task-local writer ownership.
    Finalize,
}

/// I/O failure while constructing or reading compiler-owned artifact storage.
#[derive(Debug)]
pub struct ArtifactSpoolError {
    operation: ArtifactSpoolOperation,
    source: Option<io::Error>,
}

impl ArtifactSpoolError {
    fn from_io(operation: ArtifactSpoolOperation, source: io::Error) -> Self {
        Self {
            operation,
            source: Some(source),
        }
    }

    const fn invalid_state() -> Self {
        Self {
            operation: ArtifactSpoolOperation::Finalize,
            source: None,
        }
    }

    /// Returns the failed compiler-private spool operation.
    pub const fn operation(&self) -> ArtifactSpoolOperation {
        self.operation
    }

    /// Returns the underlying I/O failure when one occurred.
    pub const fn source(&self) -> Option<&io::Error> {
        self.source.as_ref()
    }
}

/// Writable compiler-owned artifact content.
pub struct ArtifactSpoolWriter {
    path: Option<PathBuf>,
    file: Option<File>,
    content_hasher: Option<StableDigestHasher>,
}

impl ArtifactSpoolWriter {
    /// Creates a compiler-owned spool in private temporary storage.
    pub fn create() -> Result<Self, ArtifactSpoolError> {
        let directory = std::env::temp_dir();
        let process = std::process::id();
        let mut last_collision = None;

        for _ in 0..SPOOL_CREATE_ATTEMPTS {
            let id = NEXT_SPOOL_ID.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!("bray-artifact-{process}-{id}.spool"));

            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path: Some(path),
                        file: Some(file),
                        content_hasher: Some(StableDigestHasher::new()),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    last_collision = Some(error);
                }
                Err(error) => {
                    return Err(ArtifactSpoolError::from_io(
                        ArtifactSpoolOperation::Create,
                        error,
                    ));
                }
            }
        }

        let Some(error) = last_collision else {
            return Err(ArtifactSpoolError::invalid_state());
        };

        Err(ArtifactSpoolError::from_io(
            ArtifactSpoolOperation::Create,
            error,
        ))
    }

    /// Flushes pending writes and returns completed artifact content.
    pub fn finish(mut self) -> Result<ArtifactSpool, ArtifactSpoolError> {
        let Some(mut file) = self.file.take() else {
            return Err(ArtifactSpoolError::invalid_state());
        };

        file.flush()
            .map_err(|error| ArtifactSpoolError::from_io(ArtifactSpoolOperation::Flush, error))?;

        let byte_len = file
            .metadata()
            .map_err(|error| ArtifactSpoolError::from_io(ArtifactSpoolOperation::Metadata, error))?
            .len();

        drop(file);

        let Some(path) = self.path.take() else {
            return Err(ArtifactSpoolError::invalid_state());
        };

        let Some(content_hasher) = self.content_hasher.take() else {
            return Err(ArtifactSpoolError::invalid_state());
        };

        let reader = match OpenOptions::new().read(true).open(&path) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = std::fs::remove_file(&path);

                return Err(ArtifactSpoolError::from_io(
                    ArtifactSpoolOperation::OpenReader,
                    error,
                ));
            }
        };

        Ok(ArtifactSpool(Arc::new(ArtifactSpoolStorage {
            path,
            reader: Some(reader),
            byte_len,
            content_identity: content_hasher.finalize(),
        })))
    }
}

impl Write for ArtifactSpoolWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(file) = &mut self.file else {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        };

        let written = file.write(bytes)?;

        let Some(hasher) = &mut self.content_hasher else {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        };

        hasher.write(&bytes[..written]);

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let Some(file) = &mut self.file else {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        };

        file.flush()
    }
}

impl Drop for ArtifactSpoolWriter {
    fn drop(&mut self) {
        self.file.take();

        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct ArtifactSpoolStorage {
    path: PathBuf,
    reader: Option<File>,
    byte_len: u64,
    content_identity: [u8; 32],
}

impl Drop for ArtifactSpoolStorage {
    fn drop(&mut self) {
        self.reader.take();

        let _ = std::fs::remove_file(&self.path);
    }
}

/// Owning immutable handle to compiler-private spooled artifact content.
#[derive(Clone)]
pub struct ArtifactSpool(Arc<ArtifactSpoolStorage>);

impl ArtifactSpool {
    /// Returns the validated final spool length.
    pub fn byte_len(&self) -> u64 {
        self.0.byte_len
    }

    fn content_identity(&self) -> [u8; 32] {
        self.0.content_identity
    }

    /// Opens an independent read-only view without exposing the physical spool path.
    pub fn open_reader(&self) -> Result<File, ArtifactSpoolError> {
        OpenOptions::new()
            .read(true)
            .open(&self.0.path)
            .map_err(|error| ArtifactSpoolError::from_io(ArtifactSpoolOperation::OpenReader, error))
    }
}

impl fmt::Debug for ArtifactSpool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactSpool")
            .field("byte_len", &self.byte_len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for ArtifactSpool {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ArtifactSpool {}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ArtifactContentStorage {
    Memory(Arc<[u8]>),
    CompilerSpool(ArtifactSpool),
}

/// Borrowed view of immutable artifact content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactContentSource<'content> {
    /// Bytes retained in immutable shared memory.
    Memory(&'content [u8]),
    /// Bytes retained in a compiler-owned immutable spool.
    CompilerSpool(&'content ArtifactSpool),
}

/// Immutable artifact bytes or a compiler-owned immutable spool containing them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactContent {
    storage: ArtifactContentStorage,
    byte_len: u64,
    content_identity: [u8; 32],
}

impl ArtifactContent {
    /// Creates memory-backed content when its byte length is representable by the contract.
    pub fn try_memory(bytes: impl Into<Arc<[u8]>>) -> Result<Self, ArtifactContentBuildError> {
        let bytes = bytes.into();

        let Ok(byte_len) = u64::try_from(bytes.len()) else {
            return Err(ArtifactContentBuildError::LengthExceeded);
        };

        Ok(Self {
            content_identity: content_identity(&bytes),
            storage: ArtifactContentStorage::Memory(bytes),
            byte_len,
        })
    }

    /// Creates content from a completed compiler-owned spool.
    pub fn compiler_spool(spool: ArtifactSpool) -> Self {
        let byte_len = spool.byte_len();

        Self {
            content_identity: spool.content_identity(),
            storage: ArtifactContentStorage::CompilerSpool(spool),
            byte_len,
        }
    }

    /// Returns the exact artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the exact immutable content source.
    pub fn source(&self) -> ArtifactContentSource<'_> {
        match &self.storage {
            ArtifactContentStorage::Memory(bytes) => ArtifactContentSource::Memory(bytes),
            ArtifactContentStorage::CompilerSpool(spool) => {
                ArtifactContentSource::CompilerSpool(spool)
            }
        }
    }
}

impl fmt::Debug for ArtifactSpoolWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactSpoolWriter")
            .field("has_path", &self.path.is_some())
            .field("has_file", &self.file.is_some())
            .finish_non_exhaustive()
    }
}

impl Hash for ArtifactContent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.byte_len.hash(state);
        self.content_identity.hash(state);
    }
}

fn content_identity(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = StableDigestHasher::new();

    hasher.write(bytes);

    hasher.finalize()
}

/// A contract violation that prevents immutable artifact content construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactContentBuildError {
    /// The in-memory content length exceeds the contract's compact length field.
    LengthExceeded,
}

/// Hash algorithm used by an optional deterministic artifact digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactDigestAlgorithm {
    /// BLAKE3 with its standard 256-bit output.
    Blake3,
    /// SHA-256.
    Sha256,
}

impl ArtifactDigestAlgorithm {
    const fn byte_len(self) -> usize {
        match self {
            Self::Blake3 | Self::Sha256 => 32,
        }
    }
}

/// Optional deterministic digest of one complete artifact contribution.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactDigest {
    algorithm: ArtifactDigestAlgorithm,
    bytes: [u8; 32],
}

impl ArtifactDigest {
    /// Creates a digest when its byte length matches the selected algorithm.
    pub fn try_new(algorithm: ArtifactDigestAlgorithm, bytes: impl AsRef<[u8]>) -> Option<Self> {
        let bytes = bytes.as_ref();

        if bytes.len() != algorithm.byte_len() {
            return None;
        }

        let mut digest = [0_u8; 32];

        digest.copy_from_slice(bytes);

        Some(Self {
            algorithm,
            bytes: digest,
        })
    }

    /// Returns the digest algorithm.
    pub const fn algorithm(&self) -> ArtifactDigestAlgorithm {
        self.algorithm
    }

    /// Returns the exact digest bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact digest bytes by value.
    pub const fn into_bytes(self) -> [u8; 32] {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::io::{Read, Write};

    use super::{ArtifactContent, ArtifactContentSource, ArtifactSpoolWriter};

    #[test]
    fn finalized_spools_own_hidden_read_only_content() {
        let Ok(mut writer) = ArtifactSpoolWriter::create() else {
            panic!("test spool must be created");
        };

        if writer.write_all(b"object bytes").is_err() {
            panic!("test spool bytes must be written");
        }

        let Ok(spool) = writer.finish() else {
            panic!("test spool must be finalized");
        };

        let content = ArtifactContent::compiler_spool(spool);

        let ArtifactContentSource::CompilerSpool(spool) = content.source() else {
            panic!("test content must retain its spool");
        };

        let Ok(mut reader) = spool.open_reader() else {
            panic!("test spool must provide read-only access");
        };

        let mut bytes = Vec::new();

        if reader.read_to_end(&mut bytes).is_err() {
            panic!("test spool bytes must be readable");
        }

        assert_eq!(bytes, b"object bytes");
        assert_eq!(content.byte_len(), 12);
    }

    #[test]
    fn content_hashes_are_independent_of_storage() {
        let Ok(memory) = ArtifactContent::try_memory(&b"object bytes"[..]) else {
            panic!("test memory content must be valid");
        };

        let Ok(mut writer) = ArtifactSpoolWriter::create() else {
            panic!("test spool must be created");
        };

        if writer.write_all(b"object bytes").is_err() {
            panic!("test spool bytes must be written");
        }

        let Ok(spool) = writer.finish() else {
            panic!("test spool must be finalized");
        };

        let spooled = ArtifactContent::compiler_spool(spool);

        assert_eq!(hash(&memory), hash(&spooled));
    }

    fn hash(content: &ArtifactContent) -> u64 {
        let mut hasher = DefaultHasher::new();

        content.hash(&mut hasher);

        hasher.finish()
    }
}
