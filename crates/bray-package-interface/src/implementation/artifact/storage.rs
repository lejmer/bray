use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

/// One open file or immutable memory buffer. File paths are provenance, never cache identity.
#[derive(Debug)]
enum StorageSource {
    Memory(Arc<[u8]>),
    File {
        path: PathBuf,
        file: Mutex<File>,
        length: usize,
    },
}

/// Counters cover artifact access, excluding fixed hash-domain prefixes.
#[derive(Debug, Default)]
struct AccessCounters {
    reads: AtomicU64,
    bytes_read: AtomicU64,
    payload_bytes_hashed: AtomicU64,
    bytes_decompressed: AtomicU64,
}

/// Storage access made by a packed implementation and all its clones.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImplementationAccessStatistics {
    /// Open file handles created for this artifact.
    pub opens: u64,
    /// Range-read operations, including memory-backed accesses.
    pub reads: u64,
    /// Encoded bytes requested by range reads, including metadata.
    pub bytes_read: u64,
    /// Encoded checksum, decoded content and native identity bytes hashed for selected payloads.
    pub payload_bytes_hashed: u64,
    /// Decoded bytes produced from compressed selected payloads.
    pub bytes_decompressed: u64,
}

#[derive(Debug)]
pub(super) struct ImplementationStorage {
    source: StorageSource,
    counters: AccessCounters,
}

impl ImplementationStorage {
    pub(super) fn memory(bytes: Arc<[u8]>) -> Self {
        Self {
            source: StorageSource::Memory(bytes),
            counters: AccessCounters::default(),
        }
    }

    pub(super) fn statistics(&self) -> ImplementationAccessStatistics {
        ImplementationAccessStatistics {
            opens: u64::from(matches!(self.source, StorageSource::File { .. })),
            reads: self.counters.reads.load(Ordering::Relaxed),
            bytes_read: self.counters.bytes_read.load(Ordering::Relaxed),
            payload_bytes_hashed: self.counters.payload_bytes_hashed.load(Ordering::Relaxed),
            bytes_decompressed: self.counters.bytes_decompressed.load(Ordering::Relaxed),
        }
    }

    pub(super) fn hashed(&self, bytes: usize) {
        self.counters
            .payload_bytes_hashed
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(super) fn authenticated(&self, encoded: usize, decoded: u64, compressed: bool) {
        self.counters
            .payload_bytes_hashed
            .fetch_add(encoded as u64 + decoded, Ordering::Relaxed);

        if compressed {
            self.counters
                .bytes_decompressed
                .fetch_add(decoded, Ordering::Relaxed);
        }
    }

    pub(super) fn open(
        path: &Path,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        let file = File::open(path).map_err(|error| read_error(path, error))?;

        let length = file
            .metadata()
            .map_err(|error| read_error(path, error))?
            .len();

        limits.check(InterfaceLimit::FileSize, length)?;

        let length = usize::try_from(length).map_err(|_| {
            crate::implementation::invalid_value(
                crate::InterfaceValidationField::DeclaredFileLength,
            )
        })?;

        Ok(Self {
            source: StorageSource::File {
                path: path.to_owned(),
                file: Mutex::new(file),
                length,
            },
            counters: AccessCounters::default(),
        })
    }

    pub(super) fn len(&self) -> usize {
        match &self.source {
            StorageSource::Memory(bytes) => bytes.len(),
            StorageSource::File { length, .. } => *length,
        }
    }

    pub(super) fn verify_length(&self) -> Result<(), InterfaceValidationError> {
        let StorageSource::File { path, file, length } = &self.source else {
            return Ok(());
        };

        let actual = file
            .lock()
            .expect("implementation read mutex poisoned")
            .metadata()
            .map_err(|error| read_error(path, error))?
            .len();

        if actual != *length as u64 {
            return Err(InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Header,
                cause: crate::InterfaceMalformedCause::LengthMismatch {
                    field: crate::InterfaceValidationField::DeclaredFileLength,
                    expected: *length as u64,
                    actual,
                },
            });
        }

        Ok(())
    }

    pub(super) fn read(&self, range: Range<usize>) -> Result<Arc<[u8]>, InterfaceValidationError> {
        assert!(
            range.start <= range.end && range.end <= self.len(),
            "validated implementation range must be within its open storage"
        );

        self.counters.reads.fetch_add(1, Ordering::Relaxed);

        self.counters
            .bytes_read
            .fetch_add(range.len() as u64, Ordering::Relaxed);

        match &self.source {
            StorageSource::Memory(bytes) => Ok(if range == (0..bytes.len()) {
                Arc::clone(bytes)
            } else {
                Arc::from(&bytes[range])
            }),
            StorageSource::File { path, file, .. } => {
                let mut bytes = Vec::new();

                bytes.try_reserve_exact(range.len()).map_err(|_| {
                    InterfaceValidationError::AllocationUnavailable {
                        context: crate::InterfaceValidationContext::Artifact,
                        field: crate::InterfaceValidationField::RecordPayload,
                        requested: range.len() as u64,
                    }
                })?;

                bytes.resize(range.len(), 0);

                let mut file = file.lock().expect("implementation read mutex poisoned");

                file.seek(SeekFrom::Start(range.start as u64))
                    .and_then(|_| file.read_exact(&mut bytes))
                    .map_err(|error| read_error(path, error))?;

                Ok(bytes.into())
            }
        }
    }
}

fn read_error(path: &Path, error: std::io::Error) -> InterfaceValidationError {
    InterfaceValidationError::Read {
        path: path.to_owned(),
        kind: error.kind(),
    }
}
