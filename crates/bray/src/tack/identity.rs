use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;
use bray_diagnostics::{
    DiagnosticBag, DiagnosticIoErrorKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};
use bray_emitter::ProductBuildIdentity;
use bray_project::{ProjectGraph, ProjectProduct};
use bray_target::TargetIdentity;

use super::error::operation_diagnostics;
use super::model::TackBuildConfiguration;
use super::tool::{Tool, ToolExecutor};
use super::toolchain::Toolchain;

pub(super) fn test_product_identity(
    workspace_root: &Path,
    graph: &ProjectGraph,
    products: &[ProjectProduct],
    target: &TargetIdentity,
    configuration: TackBuildConfiguration,
    native_link_inputs: &[String],
    toolchain: &Toolchain,
    executor: &dyn ToolExecutor,
) -> Result<ProductBuildIdentity, DiagnosticBag> {
    let inputs = input_digest(
        workspace_root,
        graph,
        products,
        target,
        configuration,
        native_link_inputs,
    )?;

    let compiler = executor
        .identity(Tool::Compiler)
        .map_err(|error| identity_io_diagnostics(PathBuf::from("brayc"), error))?;

    let standard_library_root = toolchain.standard_library_root();

    let standard_library = build_input_path_digest(&standard_library_root)
        .map_err(|error| identity_io_diagnostics(standard_library_root, error))?;

    let runtime_metadata = toolchain.runtime_metadata(target);

    let runtime_root = runtime_metadata
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| runtime_metadata.clone());

    let runtime = build_input_path_digest(&runtime_root)
        .map_err(|error| identity_io_diagnostics(runtime_root, error))?;

    let toolchain_root = toolchain.library_root();

    let toolchain_identity = toolchain_path_digest(&toolchain_root)
        .map_err(|error| identity_io_diagnostics(toolchain_root, error))?;

    let protocol = bray_test_protocol::protocol_version();

    Ok(ProductBuildIdentity::new(
        inputs,
        compiler,
        toolchain_identity,
        standard_library,
        runtime,
        protocol,
        protocol,
    ))
}

fn input_digest(
    workspace_root: &Path,
    graph: &ProjectGraph,
    products: &[ProjectProduct],
    target: &TargetIdentity,
    configuration: TackBuildConfiguration,
    native_link_inputs: &[String],
) -> Result<[u8; 32], DiagnosticBag> {
    let mut hasher = StableDigestHasher::new();

    graph.source_authority().hash(&mut hasher);
    target.hash(&mut hasher);
    hash_field(&mut hasher, configuration.directory_name().as_bytes());

    for input in native_link_inputs {
        input.hash(&mut hasher);
    }

    for product in products {
        let package = graph
            .package(product.identity().package())
            .ok_or_else(identity_missing_product_diagnostics)?;

        package.identity().hash(&mut hasher);
        package.version().hash(&mut hasher);
        package.role().hash(&mut hasher);
        package.path().hash(&mut hasher);
        package.enabled_features().hash(&mut hasher);
        product.hash(&mut hasher);

        for source in product.sources() {
            let path = source.beneath(workspace_root);

            source.hash(&mut hasher);

            hash_file_field(&mut hasher, &path)
                .map_err(|error| identity_io_diagnostics(path.clone(), error))?;
        }
    }

    Ok(hasher.finalize())
}

fn identity_missing_product_diagnostics() -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
        DiagnosticProjectOperation::ReusableBuildIdentity,
    ))
}

pub(super) fn path_digest(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut files = Vec::new();

    collect_files(path, path, &mut files)?;

    digest_files(files)
}

fn toolchain_path_digest(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut files = Vec::new();

    match std::fs::read_dir(path) {
        Ok(entries) => {
            let mut entries = entries.collect::<Result<Vec<_>, _>>()?;
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
        Err(error) => return Err(error),
    }

    digest_files(files)
}

fn digest_files(mut files: Vec<(PathBuf, PathBuf)>) -> std::io::Result<[u8; 32]> {
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = StableDigestHasher::new();

    for (relative, file) in files {
        hash_field(&mut hasher, relative.to_string_lossy().as_bytes());
        hash_file_field(&mut hasher, &file)?;
    }

    Ok(hasher.finalize())
}

fn build_input_path_digest(path: &Path) -> std::io::Result<[u8; 32]> {
    match path_digest(path) {
        Ok(digest) => Ok(digest),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(missing_path_digest(path)),
        Err(error) => Err(error),
    }
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
) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;

    if metadata.file_type().is_file() {
        files.push((
            path.strip_prefix(root).unwrap_or(path).to_path_buf(),
            path.to_path_buf(),
        ));

        return Ok(());
    }

    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
    }

    let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
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

fn hash_file_field(hasher: &mut StableDigestHasher, path: &Path) -> std::io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let expected_length = file.metadata()?.len();
    let mut observed_length = 0_u64;
    let mut buffer = [0; 64 * 1024];

    hasher.write_u64(expected_length);

    loop {
        let length = file.read(&mut buffer)?;

        if length == 0 {
            break;
        }

        let length_u64 = u64::try_from(length).map_err(|_| std::io::ErrorKind::InvalidData)?;

        observed_length = observed_length
            .checked_add(length_u64)
            .ok_or(std::io::ErrorKind::InvalidData)?;

        hasher.write(&buffer[..length]);
    }

    if observed_length != expected_length {
        return Err(std::io::ErrorKind::InvalidData.into());
    }

    Ok(())
}

fn identity_io_diagnostics(path: PathBuf, error: std::io::Error) -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::Io {
        operation: DiagnosticProjectOperation::ReusableBuildIdentity,
        path,
        error: DiagnosticIoErrorKind::from(error.kind()),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn toolchain_digest_excludes_separately_identified_runtime_and_standard_library() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary toolchain should exist: {error}"));

        let runtime = directory.path().join("runtime");
        let standard_library = directory.path().join("standard-library");

        fs::create_dir_all(&runtime)
            .unwrap_or_else(|error| panic!("runtime directory should exist: {error}"));

        fs::create_dir_all(&standard_library)
            .unwrap_or_else(|error| panic!("standard library directory should exist: {error}"));

        fs::write(runtime.join("runtime.bin"), b"first")
            .unwrap_or_else(|error| panic!("runtime fixture should exist: {error}"));

        fs::write(standard_library.join("std.bin"), b"first")
            .unwrap_or_else(|error| panic!("standard library fixture should exist: {error}"));

        let baseline = super::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        fs::write(runtime.join("runtime.bin"), b"second")
            .unwrap_or_else(|error| panic!("runtime fixture should update: {error}"));

        fs::write(standard_library.join("std.bin"), b"second")
            .unwrap_or_else(|error| panic!("standard library fixture should update: {error}"));

        let separated_inputs = super::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        fs::write(directory.path().join("toolchain.bin"), b"toolchain")
            .unwrap_or_else(|error| panic!("toolchain fixture should exist: {error}"));

        let changed = super::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        assert_eq!(baseline, separated_inputs);
        assert_ne!(baseline, changed);
    }
}
