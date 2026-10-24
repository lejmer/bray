use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;
use bray_diagnostics::{
    DiagnosticIoErrorKind, DiagnosticProjectCommandFailure, DiagnosticProjectOperation,
};
use bray_test_protocol::{
    TestCatalog, TestCatalogDecodeError, TestCatalogDigest, TestCatalogEntryId, TestProtocolError,
    decode_test_catalog, protocol_version,
};

#[derive(Clone, Copy)]
struct TestHostPublicationIdentity {
    catalog: TestCatalogDigest,
    executable: [u8; 32],
}

pub(crate) struct BuiltTestHost {
    executable: PathBuf,
    catalog_path: PathBuf,
    publication: TestHostPublicationIdentity,
}

impl BuiltTestHost {
    pub(crate) fn try_new(
        executable: PathBuf,
        catalog_path: PathBuf,
    ) -> Result<Self, DiagnosticProjectCommandFailure> {
        let (_, publication) = read_host(&executable, &catalog_path)?;

        Ok(Self {
            executable,
            catalog_path,
            publication,
        })
    }

    pub(super) fn load(self) -> Result<LoadedTestHost, DiagnosticProjectCommandFailure> {
        let (catalog, publication) = read_host(&self.executable, &self.catalog_path)?;

        if publication.catalog != self.publication.catalog {
            return Err(DiagnosticProjectCommandFailure::TestHostChanged {
                path: self.catalog_path,
            });
        }

        if publication.executable != self.publication.executable {
            return Err(DiagnosticProjectCommandFailure::TestHostChanged {
                path: self.executable,
            });
        }

        Ok(LoadedTestHost {
            executable: self.executable,
            catalog,
            digest: publication.catalog,
        })
    }
}

pub(super) struct LoadedTestHost {
    pub(super) executable: PathBuf,
    pub(super) catalog: TestCatalog,
    pub(super) digest: TestCatalogDigest,
}

#[derive(Clone)]
pub(super) struct HostLocation {
    pub(super) executable: PathBuf,
    pub(super) entry: TestCatalogEntryId,
    pub(super) catalog_digest: TestCatalogDigest,
}

fn read_host(
    executable: &Path,
    catalog_path: &Path,
) -> Result<(TestCatalog, TestHostPublicationIdentity), DiagnosticProjectCommandFailure> {
    let read = |path: &Path| {
        fs::read(path).map_err(|error| DiagnosticProjectCommandFailure::Io {
            operation: DiagnosticProjectOperation::TestHostLoad,
            path: path.to_path_buf(),
            error: DiagnosticIoErrorKind::from(error.kind()),
        })
    };

    let executable = read(executable)?;
    let bytes = read(catalog_path)?;

    let (catalog, catalog_digest) = decode_test_catalog(&bytes).map_err(|error| match error {
        TestCatalogDecodeError::Protocol(TestProtocolError::Malformed) => {
            DiagnosticProjectCommandFailure::MalformedTestCatalog {
                path: catalog_path.to_path_buf(),
            }
        }
        TestCatalogDecodeError::Entries(cause) => match cause {
            bray_test_protocol::TestCatalogBuildError::DuplicateIdentity(identity) => {
                DiagnosticProjectCommandFailure::DuplicateTestCatalogEntry {
                    path: catalog_path.to_path_buf(),
                    identity: identity.to_string(),
                }
            }
            bray_test_protocol::TestCatalogBuildError::InvalidResultMetadata(identity) => {
                DiagnosticProjectCommandFailure::TestCatalogResultMismatch {
                    path: catalog_path.to_path_buf(),
                    identity: identity.to_string(),
                }
            }
            bray_test_protocol::TestCatalogBuildError::ProductMismatch(identity) => {
                panic!("catalog decoder must assign each entry its decoded product: {identity}")
            }
        },
        TestCatalogDecodeError::Protocol(TestProtocolError::UnsupportedVersion(actual)) => {
            DiagnosticProjectCommandFailure::UnsupportedTestCatalogVersion {
                path: catalog_path.to_path_buf(),
                actual,
                supported: protocol_version(),
            }
        }
        TestCatalogDecodeError::Protocol(TestProtocolError::ResourceLimit) => {
            DiagnosticProjectCommandFailure::TestCatalogCapacityExceeded {
                path: catalog_path.to_path_buf(),
            }
        }
        TestCatalogDecodeError::Protocol(TestProtocolError::Io) => {
            panic!("in-memory test catalog decoding cannot perform host I/O: {catalog_path:?}")
        }
    })?;

    let mut executable_digest = StableDigestHasher::new();

    executable_digest.write(&executable);

    Ok((
        catalog,
        TestHostPublicationIdentity {
            catalog: catalog_digest,
            executable: executable_digest.finalize(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::test_support::write_empty_test_host;
    use super::BuiltTestHost;
    use crate::tack::error::operation_diagnostics;
    use bray_diagnostics::{
        DiagnosticIoErrorKind, DiagnosticKind, DiagnosticProjectCommandFailure,
        DiagnosticProjectOperation,
    };

    #[test]
    fn publication_identity_rejects_replaced_host_artifacts() {
        let directory = tempfile::tempdir().expect("test directory");

        let (executable, catalog_path) = write_empty_test_host(directory.path());

        let host = BuiltTestHost::try_new(executable.clone(), catalog_path)
            .unwrap_or_else(|failure| panic!("valid publication: {failure:?}"));

        std::fs::write(&executable, b"host-two").expect("replacement executable");

        assert!(
            matches!(host.load(), Err(DiagnosticProjectCommandFailure::TestHostChanged { path }) if path == executable)
        );
    }

    #[test]
    fn initial_and_later_host_reads_preserve_the_exact_file_and_io_cause() {
        for after_publication in [false, true] {
            for catalog in [false, true] {
                let directory = tempfile::tempdir().expect("test directory");

                let (executable, catalog_path) = write_empty_test_host(directory.path());

                let host = after_publication.then(|| {
                    BuiltTestHost::try_new(executable.clone(), catalog_path.clone())
                        .unwrap_or_else(|failure| panic!("valid publication: {failure:?}"))
                });

                let missing = if catalog { &catalog_path } else { &executable };

                std::fs::rename(missing, missing.with_extension("saved"))
                    .expect("move the owned fixture file aside");

                let failure = match host {
                    Some(host) => host.load().err(),
                    None => BuiltTestHost::try_new(executable.clone(), catalog_path.clone()).err(),
                }
                .expect("missing host file must remain an operational failure");

                assert_io_failure(failure, missing);
            }
        }
    }

    fn assert_io_failure(failure: DiagnosticProjectCommandFailure, path: &Path) {
        assert_eq!(
            &failure,
            &DiagnosticProjectCommandFailure::Io {
                operation: DiagnosticProjectOperation::TestHostLoad,
                path: path.to_path_buf(),
                error: DiagnosticIoErrorKind::NotFound,
            }
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &operation_diagnostics(failure),
            DiagnosticKind::ProjectCommandFailed,
        );
    }

    #[test]
    fn changed_catalogs_preserve_malformed_and_version_causes() {
        const VERSION_OFFSET: usize = 8;

        for malformed in [false, true] {
            let directory = tempfile::tempdir().expect("test directory");

            let (executable, catalog_path) = write_empty_test_host(directory.path());

            let host = BuiltTestHost::try_new(executable, catalog_path.clone())
                .unwrap_or_else(|failure| panic!("valid publication: {failure:?}"));

            let actual = bray_test_protocol::protocol_version() + 1;
            let mut bytes = std::fs::read(&catalog_path).expect("catalog bytes");

            if malformed {
                bytes.clear();
            } else {
                bytes[VERSION_OFFSET..VERSION_OFFSET + size_of::<u32>()]
                    .copy_from_slice(&actual.to_le_bytes());
            }

            std::fs::write(&catalog_path, bytes).expect("replace owned catalog contents");

            let failure = host.load().err().expect("invalid catalog must be rejected");

            if malformed {
                assert_eq!(
                    failure,
                    DiagnosticProjectCommandFailure::MalformedTestCatalog { path: catalog_path }
                );
            } else {
                assert_eq!(
                    failure,
                    DiagnosticProjectCommandFailure::UnsupportedTestCatalogVersion {
                        path: catalog_path,
                        actual,
                        supported: bray_test_protocol::protocol_version()
                    }
                );
            }
        }
    }
}
