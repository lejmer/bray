use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::Cancellation;
use bray_linker::{LinkedArtifactKind, StagingPathKey};
use tempfile::{Builder, TempDir, TempPath};

use crate::artifact::content::{open_content, validate_staged_content};
use crate::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, EmissionPlan, OutputSink,
    PlannedArtifact, PlannedArtifactDestination,
};

const COPY_BUFFER_LEN: usize = 64 * 1024;
const LINK_INPUT_PREFIX: &str = ".bray-link-input-";
const LINK_OUTPUT_PREFIX: &str = ".bray-link-output-";

/// Transactional emitter-owned storage for one native link operation.
pub struct LinkStaging {
    _paths: Vec<TempPath>,
    _output_directories: Vec<TempDir>,
    inputs: Arc<[StagedArtifact]>,
    outputs: Arc<[LinkOutputStaging]>,
}

impl LinkStaging {
    /// Stages planned native inputs and reserves linked outputs without publishing either.
    pub fn prepare(
        plan: &EmissionPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, LinkStagingError> {
        let contributions = contributions_by_id(contributions)?;
        let mut paths = Vec::new();
        let mut inputs = Vec::new();

        for planned in plan.staged_artifacts() {
            if cancellation.is_cancelled() {
                return Err(LinkStagingError::Cancelled);
            }

            let Some(contribution) = contributions.get(planned.id()) else {
                return Err(LinkStagingError::MissingContribution(planned.id().clone()));
            };

            validate_contribution(planned, contribution)?;

            let path = stage_input(contribution, cancellation)?;

            // The typed staging record outlives this borrow from the immutable plan.
            let staged = StagedArtifact::try_new(planned.id().clone(), path.to_path_buf())
                .map_err(|_| LinkStagingError::InvalidStagingPath(planned.id().clone()))?;

            paths.push(path);
            inputs.push(staged);
        }

        if let Some((artifact, _)) = contributions.into_iter().find(|(artifact, _)| {
            plan.artifact(artifact)
                .is_none_or(|planned| planned.destination() != &PlannedArtifactDestination::Stage)
        }) {
            return Err(LinkStagingError::UnexpectedContribution(artifact));
        }

        let mut outputs = Vec::new();
        let mut output_directories = Vec::new();

        for planned in plan
            .artifacts()
            .iter()
            .filter(|artifact| matches!(artifact.producer(), ArtifactProducer::Linker(_)))
        {
            if cancellation.is_cancelled() {
                return Err(LinkStagingError::Cancelled);
            }

            let (directory, path) = reserve_output(planned, cancellation)?;

            let kind = linked_kind(planned.id().kind())
                .ok_or_else(|| LinkStagingError::UnsupportedOutput(planned.id().clone()))?;

            let Some(path_key) = StagingPathKey::try_new(staging_key(planned.id())) else {
                return Err(LinkStagingError::InvalidStagingPath(planned.id().clone()));
            };

            // The typed output record outlives this borrow from the immutable plan.
            let output = LinkOutputStaging::try_new(
                planned.id().clone(),
                kind,
                path.to_path_buf(),
                path_key,
            )
            .map_err(|_| LinkStagingError::InvalidStagingPath(planned.id().clone()))?;

            output_directories.push(directory);
            outputs.push(output);
        }

        Ok(Self {
            _paths: paths,
            _output_directories: output_directories,
            inputs: inputs.into(),
            outputs: outputs.into(),
        })
    }

    /// Returns completed staged inputs in deterministic plan order.
    pub fn inputs(&self) -> &[StagedArtifact] {
        &self.inputs
    }

    /// Returns reserved linked outputs in deterministic plan order.
    pub fn outputs(&self) -> &[LinkOutputStaging] {
        &self.outputs
    }
}

impl std::fmt::Debug for LinkStaging {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LinkStaging")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

/// Failure to prepare private native-link staging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkStagingError {
    /// Cancellation was observed before staging completed.
    Cancelled,
    /// More than one contribution names the same planned artifact.
    DuplicateContribution(ArtifactId),
    /// A required staged artifact has no complete contribution.
    MissingContribution(ArtifactId),
    /// A contribution does not match its planned producer.
    InvalidContribution(ArtifactId),
    /// A contribution does not belong to a planned staged artifact.
    UnexpectedContribution(ArtifactId),
    /// The planned linker output has no native linked-artifact category.
    UnsupportedOutput(ArtifactId),
    /// A private staging path could not be represented by the typed link contract.
    InvalidStagingPath(ArtifactId),
    /// Private staging storage could not be created.
    Create(io::ErrorKind),
    /// Immutable contribution content could not be read.
    Read(io::ErrorKind),
    /// Private staging content could not be written.
    Write(io::ErrorKind),
    /// Private staging content could not be flushed.
    Flush(io::ErrorKind),
    /// Completed staged bytes did not satisfy the immutable contribution contract.
    InvalidContent(ArtifactId),
}

fn contributions_by_id(
    contributions: impl IntoIterator<Item = ArtifactContribution>,
) -> Result<BTreeMap<ArtifactId, ArtifactContribution>, LinkStagingError> {
    let mut entries = BTreeMap::new();

    for contribution in contributions {
        // The map owns its Arc-backed identity independently of the contribution value.
        let id = contribution.id().clone();

        if entries.insert(id.clone(), contribution).is_some() {
            return Err(LinkStagingError::DuplicateContribution(id));
        }
    }

    Ok(entries)
}

fn validate_contribution(
    planned: &PlannedArtifact,
    contribution: &ArtifactContribution,
) -> Result<(), LinkStagingError> {
    if contribution.producer() != planned.producer() {
        return Err(LinkStagingError::InvalidContribution(planned.id().clone()));
    }

    Ok(())
}

fn stage_input(
    contribution: &ArtifactContribution,
    cancellation: &dyn Cancellation,
) -> Result<TempPath, LinkStagingError> {
    if cancellation.is_cancelled() {
        return Err(LinkStagingError::Cancelled);
    }

    let mut staging = Builder::new()
        .prefix(LINK_INPUT_PREFIX)
        .tempfile()
        .map_err(|error| LinkStagingError::Create(error.kind()))?;

    if cancellation.is_cancelled() {
        return Err(LinkStagingError::Cancelled);
    }

    let mut reader = open_content(contribution.content()).map_err(LinkStagingError::Read)?;
    let mut buffer = [0_u8; COPY_BUFFER_LEN];

    loop {
        if cancellation.is_cancelled() {
            return Err(LinkStagingError::Cancelled);
        }

        let read = reader
            .read(&mut buffer)
            .map_err(|error| LinkStagingError::Read(error.kind()))?;

        if read == 0 {
            break;
        }

        if cancellation.is_cancelled() {
            return Err(LinkStagingError::Cancelled);
        }

        staging
            .write_all(&buffer[..read])
            .map_err(|error| LinkStagingError::Write(error.kind()))?;
    }

    if cancellation.is_cancelled() {
        return Err(LinkStagingError::Cancelled);
    }

    staging
        .flush()
        .map_err(|error| LinkStagingError::Flush(error.kind()))?;

    let path = staging.into_temp_path();

    validate_staged_content(
        &path,
        contribution.content().byte_len(),
        contribution.digest(),
        cancellation,
    )
    .map_err(|_| LinkStagingError::InvalidContent(contribution.id().clone()))?;

    Ok(path)
}

fn reserve_output(
    planned: &PlannedArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(TempDir, PathBuf), LinkStagingError> {
    if cancellation.is_cancelled() {
        return Err(LinkStagingError::Cancelled);
    }

    let mut builder = Builder::new();

    builder.prefix(LINK_OUTPUT_PREFIX);

    let directory = match planned.destination() {
        PlannedArtifactDestination::Publish(OutputSink::Filesystem(destination)) => {
            let directory = destination
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));

            builder.tempdir_in(directory)
        }
        PlannedArtifactDestination::Publish(OutputSink::Memory { .. } | OutputSink::Stream(_)) => {
            builder.tempdir()
        }
        PlannedArtifactDestination::Stage => {
            return Err(LinkStagingError::UnsupportedOutput(planned.id().clone()));
        }
    }
    .map_err(|error| LinkStagingError::Create(error.kind()))?;

    let name = match planned.destination() {
        PlannedArtifactDestination::Publish(OutputSink::Filesystem(destination)) => destination
            .file_name()
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| LinkStagingError::InvalidStagingPath(planned.id().clone()))?,
        PlannedArtifactDestination::Publish(OutputSink::Memory { .. } | OutputSink::Stream(_)) => {
            format!(
                "{}-{}",
                planned.id().kind().machine_key(),
                planned.id().ordinal()
            )
            .into()
        }
        PlannedArtifactDestination::Stage => unreachable!("staged outputs were rejected above"),
    };

    let path = directory.path().join(name);

    Ok((directory, path))
}

const fn linked_kind(kind: ArtifactKind) -> Option<LinkedArtifactKind> {
    match kind {
        ArtifactKind::Executable => Some(LinkedArtifactKind::Executable),
        ArtifactKind::StaticLibrary => Some(LinkedArtifactKind::StaticLibrary),
        ArtifactKind::SharedLibrary => Some(LinkedArtifactKind::SharedLibrary),
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::ExecutableModule
        | ArtifactKind::DebugCompanion
        | ArtifactKind::PackageInterface
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::LinkedCompanion => None,
    }
}

fn staging_key(artifact: &ArtifactId) -> Arc<str> {
    format!(
        "{}:{}:{}:{}",
        artifact.product().package().as_str(),
        artifact.product().name(),
        artifact.kind().machine_key(),
        artifact.ordinal(),
    )
    .into()
}

/// Exact filesystem artifact retained for one native link operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StagedArtifact {
    pub(super) artifact: ArtifactId,
    pub(super) path: PathBuf,
}

impl StagedArtifact {
    /// Creates a completed staged artifact when its filesystem path is nonempty.
    pub fn try_new(
        artifact: ArtifactId,
        path: impl Into<PathBuf>,
    ) -> Result<Self, StagedArtifactBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(StagedArtifactBuildError::EmptyPath);
        }

        Ok(Self { artifact, path })
    }

    /// Returns the artifact identity selected by the emission plan.
    pub const fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    /// Returns the exact staged filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A contract violation that prevents staged-artifact construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedArtifactBuildError {
    /// The staged filesystem path is empty.
    EmptyPath,
}

/// Emitter-owned staging location for one planned linked output.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LinkOutputStaging {
    pub(super) artifact: ArtifactId,
    pub(super) kind: LinkedArtifactKind,
    pub(super) path: PathBuf,
    pub(super) path_key: StagingPathKey,
}

impl LinkOutputStaging {
    /// Creates one linked-output staging location when its filesystem path is nonempty.
    pub fn try_new(
        artifact: ArtifactId,
        kind: LinkedArtifactKind,
        path: impl Into<PathBuf>,
        path_key: StagingPathKey,
    ) -> Result<Self, LinkOutputStagingBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(LinkOutputStagingBuildError::EmptyPath);
        }

        Ok(Self {
            artifact,
            kind,
            path,
            path_key,
        })
    }

    /// Returns the planned artifact that will receive the linked bytes.
    pub const fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }

    /// Returns the native linked artifact category.
    pub const fn kind(&self) -> LinkedArtifactKind {
        self.kind
    }

    /// Returns the exact writable staging path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the normalized staging-path identity.
    pub const fn path_key(&self) -> &StagingPathKey {
        &self.path_key
    }
}

/// A contract violation that prevents linked-output staging construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkOutputStagingBuildError {
    /// The linked-output staging path is empty.
    EmptyPath,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_codegen::{
        ArtifactContent, AssemblySyntaxKind, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind,
    };

    use super::LinkStaging;
    use crate::test_support::{
        backend_capabilities, backend_identity, codegen_unit_key, executable_host_contract,
        product_identity, target_identity, target_output_description,
    };
    use crate::{
        ArtifactContribution, ArtifactKind, ArtifactRequirement, BackendEmissionPolicy,
        EmissionBackend, EmissionPlanner, EmissionRequest, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };

    #[test]
    fn link_staging_owns_validated_inputs_and_deterministic_output_keys() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test output directory must exist: {error:?}"));

        let destination = directory.path().join("application");
        let destination_name = destination.file_name().map(ToOwned::to_owned);
        let plan = linked_plan(destination);
        let contribution = staged_contribution(&plan, b"object bytes");

        let first = LinkStaging::prepare(&plan, [contribution.clone()], &never_cancelled)
            .unwrap_or_else(|error| panic!("first link staging must complete: {error:?}"));

        let input_path = first.inputs()[0].path().to_owned();
        let output_path = first.outputs()[0].path().to_owned();
        let output_key = first.outputs()[0].path_key().clone();

        assert_eq!(
            std::fs::read(&input_path)
                .unwrap_or_else(|error| panic!("staged input must be readable: {error:?}")),
            b"object bytes",
        );

        let output_directory = first.outputs()[0]
            .path()
            .parent()
            .unwrap_or_else(|| panic!("staged output must have a private directory"));

        assert_eq!(output_directory.parent(), Some(directory.path()));

        assert_eq!(
            first.outputs()[0].path().file_name(),
            destination_name.as_deref()
        );

        let second = LinkStaging::prepare(&plan, [contribution], &never_cancelled)
            .unwrap_or_else(|error| panic!("second link staging must complete: {error:?}"));

        assert_ne!(first.outputs()[0].path(), second.outputs()[0].path());

        assert_eq!(
            first.outputs()[0].path_key(),
            second.outputs()[0].path_key()
        );

        assert_eq!(first.outputs()[0].path_key(), &output_key);

        drop(first);

        assert!(!input_path.exists());
        assert!(!output_path.exists());
        assert!(second.inputs()[0].path().exists());
        assert!(!second.outputs()[0].path().exists());

        assert!(
            second.outputs()[0]
                .path()
                .parent()
                .is_some_and(std::path::Path::exists)
        );
    }

    #[test]
    fn cancelled_link_staging_creates_no_transaction() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test output directory must exist: {error:?}"));

        let plan = linked_plan(directory.path().join("application"));
        let contribution = staged_contribution(&plan, b"object bytes");

        assert!(matches!(
            LinkStaging::prepare(&plan, [contribution], &always_cancelled),
            Err(super::LinkStagingError::Cancelled)
        ));
    }

    #[test]
    fn cancellation_after_staging_creation_discards_private_state() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test output directory must exist: {error:?}"));

        let plan = linked_plan(directory.path().join("application"));
        let contribution = staged_contribution(&plan, b"object bytes");
        let observations = AtomicUsize::new(0);
        let cancellation = || observations.fetch_add(1, Ordering::AcqRel) >= 2;

        assert!(matches!(
            LinkStaging::prepare(&plan, [contribution], &cancellation),
            Err(super::LinkStagingError::Cancelled)
        ));

        assert_eq!(
            std::fs::read_dir(directory.path())
                .unwrap_or_else(|error| panic!("test output directory must be readable: {error:?}"))
                .count(),
            0
        );
    }

    fn linked_plan(destination: std::path::PathBuf) -> crate::EmissionPlan {
        let backend = EmissionBackend::try_new(
            backend_identity(),
            backend_capabilities(),
            [codegen_unit_key(1)],
            BackendEmissionPolicy::new(
                DebugInformationMode::None,
                DebugInformationOutputMode::Omit,
                Some(LinkableArtifactKind::RelocatableObject),
                BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
            ),
        )
        .unwrap_or_else(|error| panic!("test emission backend must be valid: {error:?}"));

        let request = EmissionRequest::try_new(
            product_identity(),
            crate::ProductKind::Executable,
            Some(executable_host_contract()),
            target_identity(),
            RequestedArtifactDestination::FilesystemFile(destination),
            [RequestedArtifact::new(
                ArtifactKind::Executable,
                ArtifactRequirement::Required,
            )],
            ReplacementPolicy::RequireAbsent,
        )
        .unwrap_or_else(|error| panic!("test emission request must be valid: {error:?}"));

        EmissionPlanner::new(target_output_description(), Some(backend), None)
            .plan(request)
            .unwrap_or_else(|error| panic!("test emission plan must be valid: {error:?}"))
    }

    fn staged_contribution(plan: &crate::EmissionPlan, bytes: &[u8]) -> ArtifactContribution {
        let planned = plan
            .staged_artifacts()
            .next()
            .unwrap_or_else(|| panic!("linked test plan must stage one input"));

        let content = ArtifactContent::try_memory(bytes)
            .unwrap_or_else(|error| panic!("test contribution must be valid: {error:?}"));

        ArtifactContribution::new(
            planned.id().clone(),
            planned.producer().clone(),
            content,
            None,
        )
    }

    fn never_cancelled() -> bool {
        false
    }

    fn always_cancelled() -> bool {
        true
    }
}
