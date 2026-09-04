use std::collections::BTreeMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;
use bray_diagnostics::{
    DiagnosticBag, DiagnosticIoErrorKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};
use bray_emitter::ProductBuildIdentity;
use bray_project::{ProjectGraph, ProjectProduct};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::error::operation_diagnostics;
use super::model::TackBuildConfiguration;
use super::tool::{Tool, ToolExecutor};
use super::toolchain::Toolchain;

pub(super) struct TestProductBuildEvidence {
    identity: ProductBuildIdentity,
    source_inputs: BTreeMap<ProductIdentity, [u8; 32]>,
}

impl TestProductBuildEvidence {
    pub(super) const fn identity(&self) -> &ProductBuildIdentity {
        &self.identity
    }

    pub(super) fn source_inputs(&self, product: &ProductIdentity) -> Option<[u8; 32]> {
        self.source_inputs.get(product).copied()
    }
}

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
    Ok(test_product_build_evidence(
        workspace_root,
        graph,
        products,
        target,
        configuration,
        native_link_inputs,
        toolchain,
        executor,
    )?
    .identity)
}

pub(super) fn test_product_build_evidence(
    workspace_root: &Path,
    graph: &ProjectGraph,
    products: &[ProjectProduct],
    target: &TargetIdentity,
    configuration: TackBuildConfiguration,
    native_link_inputs: &[String],
    toolchain: &Toolchain,
    executor: &dyn ToolExecutor,
) -> Result<TestProductBuildEvidence, DiagnosticBag> {
    let (inputs, source_inputs) = input_digest(
        workspace_root,
        graph,
        products,
        target,
        configuration,
        native_link_inputs,
    )?;

    let compiler = executor
        .identity(Tool::Compiler)
        .map_err(|error| identity_io_diagnostics(error.path, error.error))?;

    let standard_library_root = toolchain.standard_library_root();

    let standard_library = bray_emitter::build_input_path_digest(&standard_library_root)
        .map_err(identity_digest_diagnostics)?;

    let runtime_metadata = toolchain.runtime_metadata(target);

    let runtime_root = runtime_metadata
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| runtime_metadata.clone());

    let runtime = bray_emitter::build_input_path_digest(&runtime_root)
        .map_err(identity_digest_diagnostics)?;

    let toolchain_root = toolchain.library_root();

    let toolchain_identity = bray_emitter::toolchain_path_digest(&toolchain_root)
        .map_err(identity_digest_diagnostics)?;

    let protocol = bray_test_protocol::protocol_version();

    Ok(TestProductBuildEvidence {
        identity: ProductBuildIdentity::new(
            inputs,
            compiler,
            toolchain_identity,
            standard_library,
            runtime,
            protocol,
            protocol,
        ),
        source_inputs,
    })
}

fn input_digest(
    workspace_root: &Path,
    graph: &ProjectGraph,
    products: &[ProjectProduct],
    target: &TargetIdentity,
    configuration: TackBuildConfiguration,
    native_link_inputs: &[String],
) -> Result<([u8; 32], BTreeMap<ProductIdentity, [u8; 32]>), DiagnosticBag> {
    let mut hasher = StableDigestHasher::new();
    let mut source_inputs = BTreeMap::new();

    graph.source_authority().hash(&mut hasher);
    target.hash(&mut hasher);
    configuration.directory_name().hash(&mut hasher);

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

        let digest = product_source_input_digest(workspace_root, product)?;

        digest.hash(&mut hasher);
        source_inputs.insert(product.identity().clone(), digest);
    }

    Ok((hasher.finalize(), source_inputs))
}

fn product_source_input_digest(
    workspace_root: &Path,
    product: &ProjectProduct,
) -> Result<[u8; 32], DiagnosticBag> {
    let paths = product
        .sources()
        .iter()
        .map(|source| source.beneath(workspace_root));

    let sources = bray_tooling::source_inputs_from_file_arguments(paths)
        .map_err(bray_tooling::SourceInputError::into_diagnostic_bag)?;

    Ok(bray_tooling::source_input_digest(&sources))
}

fn identity_missing_product_diagnostics() -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
        DiagnosticProjectOperation::ReusableBuildIdentity,
    ))
}

fn identity_io_diagnostics(path: PathBuf, error: std::io::Error) -> DiagnosticBag {
    operation_diagnostics(DiagnosticProjectCommandFailure::Io {
        operation: DiagnosticProjectOperation::ReusableBuildIdentity,
        path,
        error: DiagnosticIoErrorKind::from(error.kind()),
    })
}

fn identity_digest_diagnostics(error: bray_emitter::BuildInputDigestError) -> DiagnosticBag {
    let (path, cause) = error.into_parts();

    identity_io_diagnostics(path, cause)
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

        let baseline = bray_emitter::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        fs::write(runtime.join("runtime.bin"), b"second")
            .unwrap_or_else(|error| panic!("runtime fixture should update: {error}"));

        fs::write(standard_library.join("std.bin"), b"second")
            .unwrap_or_else(|error| panic!("standard library fixture should update: {error}"));

        let separated_inputs = bray_emitter::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        fs::write(directory.path().join("toolchain.bin"), b"toolchain")
            .unwrap_or_else(|error| panic!("toolchain fixture should exist: {error}"));

        let changed = bray_emitter::toolchain_path_digest(directory.path())
            .unwrap_or_else(|error| panic!("toolchain should hash: {error}"));

        assert_eq!(baseline, separated_inputs);
        assert_ne!(baseline, changed);
    }
}
