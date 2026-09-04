use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;

/// Immutable identities needed to prove that a published native test product is reusable.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBuildIdentity {
    inputs: [u8; 32],
    compiler: [u8; 32],
    toolchain: [u8; 32],
    standard_library: [u8; 32],
    runtime: [u8; 32],
    catalog_protocol: u32,
    runner_protocol: u32,
}

impl ProductBuildIdentity {
    /// Creates the complete identity chain for one reusable product generation.
    #[expect(
        clippy::too_many_arguments,
        reason = "each independent reusable-build identity remains explicit"
    )]
    pub const fn new(
        inputs: [u8; 32],
        compiler: [u8; 32],
        toolchain: [u8; 32],
        standard_library: [u8; 32],
        runtime: [u8; 32],
        catalog_protocol: u32,
        runner_protocol: u32,
    ) -> Self {
        Self {
            inputs,
            compiler,
            toolchain,
            standard_library,
            runtime,
            catalog_protocol,
            runner_protocol,
        }
    }

    /// Returns the first exact part that differs from another identity chain.
    pub fn mismatch(&self, expected: &Self) -> Option<ProductBuildIdentityPart> {
        if self.inputs != expected.inputs {
            Some(ProductBuildIdentityPart::Inputs)
        } else if self.compiler != expected.compiler {
            Some(ProductBuildIdentityPart::Compiler)
        } else if self.toolchain != expected.toolchain {
            Some(ProductBuildIdentityPart::Toolchain)
        } else if self.standard_library != expected.standard_library {
            Some(ProductBuildIdentityPart::StandardLibrary)
        } else if self.runtime != expected.runtime {
            Some(ProductBuildIdentityPart::Runtime)
        } else if self.catalog_protocol != expected.catalog_protocol {
            Some(ProductBuildIdentityPart::CatalogProtocol)
        } else if self.runner_protocol != expected.runner_protocol {
            Some(ProductBuildIdentityPart::RunnerProtocol)
        } else {
            None
        }
    }

    /// Returns the exact compiler executable digest.
    pub const fn compiler(&self) -> [u8; 32] {
        self.compiler
    }

    /// Returns the digest of toolchain inputs outside the runtime and standard library.
    pub const fn toolchain(&self) -> [u8; 32] {
        self.toolchain
    }

    /// Returns the exact standard-library input digest.
    pub const fn standard_library(&self) -> [u8; 32] {
        self.standard_library
    }

    /// Returns the exact runtime input digest.
    pub const fn runtime(&self) -> [u8; 32] {
        self.runtime
    }

    /// Returns the test-catalog wire revision.
    pub const fn catalog_protocol(&self) -> u32 {
        self.catalog_protocol
    }

    /// Returns the native test-runner wire revision.
    pub const fn runner_protocol(&self) -> u32 {
        self.runner_protocol
    }
}

/// Failure while reading one exact path for a build-input digest.
#[derive(Debug)]
pub struct BuildInputDigestError {
    path: PathBuf,
    cause: std::io::Error,
}

impl BuildInputDigestError {
    /// Returns the exact path whose metadata or contents could not be read.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the underlying I/O error category.
    pub fn kind(&self) -> std::io::ErrorKind {
        self.cause.kind()
    }

    /// Separates the exact path and underlying I/O error.
    pub fn into_parts(self) -> (PathBuf, std::io::Error) {
        (self.path, self.cause)
    }
}

/// Returns a deterministic digest for a file or directory tree.
pub fn build_input_path_digest(path: &Path) -> Result<[u8; 32], BuildInputDigestError> {
    match path_digest(path) {
        Ok(digest) => Ok(digest),
        Err(error) if error.path() == path && error.kind() == std::io::ErrorKind::NotFound => {
            Ok(missing_path_digest(path))
        }
        Err(error) => Err(error),
    }
}

/// Returns a deterministic digest for toolchain inputs outside separately identified products.
pub fn toolchain_path_digest(path: &Path) -> Result<[u8; 32], BuildInputDigestError> {
    let mut files = Vec::new();

    match std::fs::read_dir(path) {
        Ok(entries) => {
            let mut entries = entries
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| digest_error(path, error))?;

            entries.sort_unstable_by_key(std::fs::DirEntry::file_name);

            for entry in entries {
                if matches!(
                    entry.file_name().to_str(),
                    Some("runtime" | "standard-library")
                ) {
                    continue;
                }

                collect_files(path, &entry.path(), &mut files)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(missing_path_digest(path));
        }
        Err(error) => return Err(digest_error(path, error)),
    }

    digest_files(files)
}

/// Returns a deterministic digest for an existing file or directory tree.
pub fn path_digest(path: &Path) -> Result<[u8; 32], BuildInputDigestError> {
    let mut files = Vec::new();

    collect_files(path, path, &mut files)?;

    digest_files(files)
}

fn digest_files(mut files: Vec<(PathBuf, PathBuf)>) -> Result<[u8; 32], BuildInputDigestError> {
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = StableDigestHasher::new();

    for (relative, file) in files {
        hash_field(&mut hasher, relative.to_string_lossy().as_bytes());
        hash_file_field(&mut hasher, &file)?;
    }

    Ok(hasher.finalize())
}

fn missing_path_digest(path: &Path) -> [u8; 32] {
    let mut hasher = StableDigestHasher::new();

    hash_field(&mut hasher, b"missing");
    hash_field(&mut hasher, path.to_string_lossy().as_bytes());

    hasher.finalize()
}

fn collect_files(
    root: &Path,
    path: &Path,
    files: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), BuildInputDigestError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| digest_error(path, error))?;

    if metadata.file_type().is_file() {
        files.push((
            path.strip_prefix(root).unwrap_or(path).to_path_buf(),
            path.to_path_buf(),
        ));

        return Ok(());
    }

    if !metadata.file_type().is_dir() {
        return Err(digest_error(path, std::io::ErrorKind::InvalidData.into()));
    }

    let entries = std::fs::read_dir(path).map_err(|error| digest_error(path, error))?;

    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| digest_error(path, error))?;

    entries.sort_unstable_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        collect_files(root, &entry.path(), files)?;
    }

    Ok(())
}

fn hash_field(hasher: &mut StableDigestHasher, bytes: &[u8]) {
    hasher.write_usize(bytes.len());
    hasher.write(bytes);
}

fn hash_file_field(
    hasher: &mut StableDigestHasher,
    path: &Path,
) -> Result<(), BuildInputDigestError> {
    let mut file = std::fs::File::open(path).map_err(|error| digest_error(path, error))?;

    let expected_length = file
        .metadata()
        .map_err(|error| digest_error(path, error))?
        .len();

    let mut observed_length = 0_u64;
    let mut buffer = [0; 64 * 1024];

    hasher.write_u64(expected_length);

    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|error| digest_error(path, error))?;

        if length == 0 {
            break;
        }

        let length_u64 = u64::try_from(length)
            .map_err(|_| digest_error(path, std::io::ErrorKind::InvalidData.into()))?;

        observed_length = observed_length
            .checked_add(length_u64)
            .ok_or_else(|| digest_error(path, std::io::ErrorKind::InvalidData.into()))?;

        hasher.write(&buffer[..length]);
    }

    if observed_length != expected_length {
        return Err(digest_error(path, std::io::ErrorKind::InvalidData.into()));
    }

    Ok(())
}

fn digest_error(path: &Path, cause: std::io::Error) -> BuildInputDigestError {
    BuildInputDigestError {
        path: path.to_path_buf(),
        cause,
    }
}

/// Exact part of a reusable product identity that failed validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductBuildIdentityPart {
    /// Project, product, target, profile, native-link, or source inputs.
    Inputs,
    /// Exact compiler executable.
    Compiler,
    /// Toolchain inputs outside the separately identified standard library and runtime.
    Toolchain,
    /// Selected standard-library bundle or source tree.
    StandardLibrary,
    /// Selected target runtime artifacts.
    Runtime,
    /// Test-catalog wire revision.
    CatalogProtocol,
    /// Native test-runner wire revision.
    RunnerProtocol,
}

#[cfg(test)]
mod tests {
    use super::{ProductBuildIdentity, ProductBuildIdentityPart};

    #[test]
    fn missing_build_input_identity_is_explicit_and_deterministic() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory should exist: {error:?}"));

        let missing = directory.path().join("missing");

        let error = super::path_digest(&missing)
            .expect_err("an existing-input digest must reject a missing root");

        let first = super::build_input_path_digest(&missing)
            .unwrap_or_else(|error| panic!("missing identity should hash: {error:?}"));

        let second = super::build_input_path_digest(&missing)
            .unwrap_or_else(|error| panic!("missing identity should be stable: {error:?}"));

        assert_eq!(error.path(), missing);
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(first, second);
    }

    #[test]
    fn identity_mismatches_preserve_the_first_exact_category() {
        let baseline = identity([[0; 32]; 5], [1, 1]);

        let cases = [
            (
                identity([[1; 32], [0; 32], [0; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Inputs,
            ),
            (
                identity([[0; 32], [1; 32], [0; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Compiler,
            ),
            (
                identity([[0; 32], [0; 32], [1; 32], [0; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::Toolchain,
            ),
            (
                identity([[0; 32], [0; 32], [0; 32], [1; 32], [0; 32]], [1, 1]),
                ProductBuildIdentityPart::StandardLibrary,
            ),
            (
                identity([[0; 32], [0; 32], [0; 32], [0; 32], [1; 32]], [1, 1]),
                ProductBuildIdentityPart::Runtime,
            ),
            (
                identity([[0; 32]; 5], [2, 1]),
                ProductBuildIdentityPart::CatalogProtocol,
            ),
            (
                identity([[0; 32]; 5], [1, 2]),
                ProductBuildIdentityPart::RunnerProtocol,
            ),
        ];

        for (actual, expected) in cases {
            assert_eq!(actual.mismatch(&baseline), Some(expected));
        }

        assert_eq!(baseline.mismatch(&baseline), None);
    }

    fn identity(digests: [[u8; 32]; 5], protocols: [u32; 2]) -> ProductBuildIdentity {
        ProductBuildIdentity::new(
            digests[0],
            digests[1],
            digests[2],
            digests[3],
            digests[4],
            protocols[0],
            protocols[1],
        )
    }
}
