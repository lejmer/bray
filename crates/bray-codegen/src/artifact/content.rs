use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SPOOL_ID: AtomicU64 = AtomicU64::new(0);
const SPOOL_CREATE_ATTEMPTS: usize = 128;

/// Compiler operation that failed while owning an immutable artifact spool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactSpoolOperation {
    /// Create compiler-private spool storage.
    Create,
    /// Flush the writable spool before publication.
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

/// Task-local writable compiler spool before immutable publication.
#[derive(Debug)]
pub struct ArtifactSpoolWriter {
    path: Option<PathBuf>,
    file: Option<File>,
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

    /// Flushes and freezes the spool into an immutable owning handle.
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
        })))
    }
}

impl Write for ArtifactSpoolWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(file) = &mut self.file else {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        };

        file.write(bytes)
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

/// Borrowed view of immutable artifact content storage.
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
}

impl ArtifactContent {
    /// Creates memory-backed content when its byte length is representable by the contract.
    pub fn try_memory(bytes: impl Into<Arc<[u8]>>) -> Result<Self, ArtifactContentBuildError> {
        let bytes = bytes.into();
        let Ok(byte_len) = u64::try_from(bytes.len()) else {
            return Err(ArtifactContentBuildError::LengthExceeded);
        };

        Ok(Self {
            storage: ArtifactContentStorage::Memory(bytes),
            byte_len,
        })
    }

    /// Creates content backed by an owning compiler-private immutable spool.
    pub fn compiler_spool(spool: ArtifactSpool) -> Self {
        let byte_len = spool.byte_len();

        Self {
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
    bytes: Arc<[u8]>,
}

impl ArtifactDigest {
    /// Creates a digest when its byte length matches the selected algorithm.
    pub fn try_new(
        algorithm: ArtifactDigestAlgorithm,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Option<Self> {
        let bytes = bytes.into();

        if bytes.len() != algorithm.byte_len() {
            return None;
        }

        Some(Self { algorithm, bytes })
    }

    /// Returns the digest algorithm.
    pub const fn algorithm(&self) -> ArtifactDigestAlgorithm {
        self.algorithm
    }

    /// Returns the exact digest bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
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
}
