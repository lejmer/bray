use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::Sha256Reader;
use bray_symbols::NativeLinkRequirement;

use super::catalog::{
    RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactMetadata,
    RuntimeArtifactPurpose,
};
use crate::{RuntimeAbiRole, RuntimeArtifactId, RuntimeCapability, RuntimeContract};

/// One resolved physical runtime component.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactComponent {
    metadata: RuntimeArtifactComponentMetadata,
    archive: PathBuf,
}

impl RuntimeArtifactComponent {
    /// Returns immutable component metadata.
    pub const fn metadata(&self) -> &RuntimeArtifactComponentMetadata {
        &self.metadata
    }

    /// Returns the exact native archive path.
    pub fn archive(&self) -> &Path {
        &self.archive
    }
}

/// Resolved runtime catalog available to one compiler invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifact {
    metadata: RuntimeArtifactMetadata,
    components: Arc<[RuntimeArtifactComponent]>,
}

impl RuntimeArtifact {
    /// Resolves every catalog component to its published archive path.
    pub fn try_new(
        metadata: RuntimeArtifactMetadata,
        components: impl IntoIterator<Item = (RuntimeArtifactId, PathBuf)>,
    ) -> Result<Self, RuntimeArtifactBuildError> {
        let mut supplied: BTreeMap<_, _> = components.into_iter().collect();

        let mut resolved = Vec::with_capacity(metadata.components().len());

        for component in metadata.components() {
            let Some(archive) = supplied.remove(component.identity()) else {
                return Err(RuntimeArtifactBuildError::MissingComponent);
            };

            if archive.file_name().and_then(|name| name.to_str())
                != Some(component.archive_file_name())
            {
                return Err(RuntimeArtifactBuildError::ArchiveFileNameMismatch);
            }

            resolved.push(RuntimeArtifactComponent {
                metadata: component.clone(),
                archive,
            });
        }

        if !supplied.is_empty() {
            return Err(RuntimeArtifactBuildError::UnexpectedComponent);
        }

        Ok(Self {
            metadata,
            components: resolved.into(),
        })
    }

    /// Returns the immutable artifact metadata.
    pub const fn metadata(&self) -> &RuntimeArtifactMetadata {
        &self.metadata
    }

    /// Returns the selected runtime contract.
    pub const fn contract(&self) -> &RuntimeContract {
        self.metadata.contract()
    }

    /// Returns resolved physical components in metadata order.
    pub fn components(&self) -> &[RuntimeArtifactComponent] {
        &self.components
    }

    /// Selects the exact physical components required by one product.
    pub fn select(
        &self,
        purpose: RuntimeArtifactPurpose,
        requirements: &crate::RuntimeRequirements,
    ) -> Result<RuntimeArtifactSelection, RuntimeArtifactSelectionError> {
        self.contract()
            .validate(requirements)
            .map_err(RuntimeArtifactSelectionError::IncompatibleRuntime)?;

        let required_capabilities = requirements.capabilities().iter().copied().chain(
            requirements
                .lanes()
                .iter()
                .copied()
                .map(crate::runtime::lane_capability),
        );

        let mut selected = BTreeSet::new();

        for role in requirements.roles() {
            selected.insert(self.owner_of_role(purpose, *role)?);
        }

        for capability in required_capabilities {
            selected.insert(self.owner_of_capability(purpose, capability)?);
        }

        let mut pending = selected.iter().copied().collect::<Vec<_>>();

        while let Some(index) = pending.pop() {
            for dependency in self.components[index].metadata().dependencies() {
                let Some(dependency) = self
                    .components
                    .iter()
                    .position(|component| component.metadata().identity() == dependency)
                else {
                    continue;
                };

                if selected.insert(dependency) {
                    pending.push(dependency);
                }
            }
        }

        let mut authenticated = BTreeMap::new();

        for index in &selected {
            let component = &self.components[*index];
            let archive = component.archive();

            let digest = match authenticated.get(archive) {
                Some(digest) => *digest,
                None => {
                    let digest = authenticate_archive(archive).map_err(|error| match error {
                        ArchiveValidationError::Unreadable(kind) => {
                            RuntimeArtifactSelectionError::UnreadableArchive {
                                component: component.metadata().identity().clone(),
                                path: archive.to_path_buf(),
                                kind,
                            }
                        }
                        ArchiveValidationError::Invalid => {
                            RuntimeArtifactSelectionError::InvalidArchive {
                                component: component.metadata().identity().clone(),
                                path: archive.to_path_buf(),
                            }
                        }
                    })?;

                    authenticated.insert(archive.to_path_buf(), digest);

                    digest
                }
            };

            if digest != component.metadata().archive_digest() {
                return Err(RuntimeArtifactSelectionError::ArchiveDigestMismatch {
                    component: component.metadata().identity().clone(),
                    path: archive.to_path_buf(),
                    expected: component.metadata().archive_digest(),
                    actual: digest,
                });
            }
        }

        let components = selected
            .into_iter()
            .map(|index| self.components[index].clone())
            .collect::<Vec<_>>()
            .into();

        Ok(RuntimeArtifactSelection {
            contract: self.contract().clone(),
            components,
        })
    }

    fn owner_of_role(
        &self,
        purpose: RuntimeArtifactPurpose,
        role: RuntimeAbiRole,
    ) -> Result<usize, RuntimeArtifactSelectionError> {
        self.components
            .iter()
            .position(|component| {
                component.metadata().purpose() == purpose
                    && component.metadata().roles().binary_search(&role).is_ok()
            })
            .ok_or(RuntimeArtifactSelectionError::MissingRoleOwner(role))
    }

    fn owner_of_capability(
        &self,
        purpose: RuntimeArtifactPurpose,
        capability: RuntimeCapability,
    ) -> Result<usize, RuntimeArtifactSelectionError> {
        self.components
            .iter()
            .position(|component| {
                component.metadata().purpose() == purpose
                    && component
                        .metadata()
                        .capabilities()
                        .binary_search(&capability)
                        .is_ok()
            })
            .ok_or(RuntimeArtifactSelectionError::MissingCapabilityOwner(
                capability,
            ))
    }
}

enum ArchiveValidationError {
    Unreadable(io::ErrorKind),
    Invalid,
}

fn authenticate_archive(path: &Path) -> Result<RuntimeArtifactDigest, ArchiveValidationError> {
    let archive = std::fs::File::open(path)
        .map_err(|error| ArchiveValidationError::Unreadable(error.kind()))?;

    let metadata = archive
        .metadata()
        .map_err(|error| ArchiveValidationError::Unreadable(error.kind()))?;

    if !metadata.is_file() {
        return Err(ArchiveValidationError::Invalid);
    }

    let mut archive = Sha256Reader::new(archive);
    let mut magic = [0; 8];

    read_archive_exact(&mut archive, &mut magic)?;

    if magic != *b"!<arch>\n" {
        return Err(ArchiveValidationError::Invalid);
    }

    loop {
        let mut header = [0; 60];

        match archive.read(&mut header[..1]) {
            Ok(0) => break,
            Ok(1) => read_archive_exact(&mut archive, &mut header[1..])?,
            Ok(_) => unreachable!("a one-byte read cannot return more than one byte"),
            Err(error) => return Err(ArchiveValidationError::Unreadable(error.kind())),
        }

        let member_size = archive_member_size(&header)?;

        consume_archive_bytes(&mut archive, member_size)?;

        if member_size % 2 == 1 {
            let mut padding = [0];

            read_archive_exact(&mut archive, &mut padding)?;

            if padding != [b'\n'] {
                return Err(ArchiveValidationError::Invalid);
            }
        }
    }

    let digest = archive.finalize();

    Ok(RuntimeArtifactDigest::new(digest))
}

fn archive_member_size(header: &[u8; 60]) -> Result<u64, ArchiveValidationError> {
    if header[58..] != *b"`\n" {
        return Err(ArchiveValidationError::Invalid);
    }

    let size = std::str::from_utf8(&header[48..58])
        .map_err(|_| ArchiveValidationError::Invalid)?
        .trim_end_matches(' ');

    if size.is_empty() || !size.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ArchiveValidationError::Invalid);
    }

    size.parse()
        .map_err(|_| ArchiveValidationError::Invalid)
}

fn consume_archive_bytes(
    archive: &mut impl Read,
    mut remaining: u64,
) -> Result<(), ArchiveValidationError> {
    const BUFFER_LENGTH: usize = 64 * 1024;

    let mut buffer = [0; BUFFER_LENGTH];

    let buffer_length = u64::try_from(BUFFER_LENGTH)
        .unwrap_or_else(|_| unreachable!("archive buffer length must fit u64"));

    while remaining != 0 {
        let requested = usize::try_from(remaining.min(buffer_length))
            .unwrap_or_else(|_| unreachable!("bounded archive read length must fit usize"));

        let length = archive
            .read(&mut buffer[..requested])
            .map_err(|error| ArchiveValidationError::Unreadable(error.kind()))?;

        if length == 0 {
            return Err(ArchiveValidationError::Invalid);
        }

        remaining -= u64::try_from(length)
            .unwrap_or_else(|_| unreachable!("archive read length must fit u64"));
    }

    Ok(())
}

fn read_archive_exact(
    archive: &mut impl Read,
    bytes: &mut [u8],
) -> Result<(), ArchiveValidationError> {
    match archive.read_exact(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(ArchiveValidationError::Invalid)
        }
        Err(error) => Err(ArchiveValidationError::Unreadable(error.kind())),
    }
}

/// Exact runtime components selected for one product link.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactSelection {
    contract: RuntimeContract,
    components: Arc<[RuntimeArtifactComponent]>,
}

impl RuntimeArtifactSelection {
    /// Returns the selected runtime contract.
    pub const fn contract(&self) -> &RuntimeContract {
        &self.contract
    }

    /// Returns selected components in canonical catalog order.
    pub fn components(&self) -> &[RuntimeArtifactComponent] {
        &self.components
    }

    /// Returns whether any selected component embeds platform services.
    pub fn embeds_platform_services(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.metadata().embeds_platform_services())
    }

    /// Returns native dependencies across selected components in link order.
    pub fn native_links(&self) -> impl Iterator<Item = &NativeLinkRequirement> {
        self.components
            .iter()
            .flat_map(|component| component.metadata().native_links())
    }
}

/// A contract violation that prevents resolving metadata to an archive path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactBuildError {
    /// Metadata names a component for which no archive was supplied.
    MissingComponent,
    /// An archive was supplied for a component absent from metadata.
    UnexpectedComponent,
    /// The selected path does not end with the published archive file name.
    ArchiveFileNameMismatch,
}

/// Failure to select a complete physical runtime surface for one product.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeArtifactSelectionError {
    /// The runtime contract is incompatible with reachable requirements.
    IncompatibleRuntime(crate::RuntimeCompatibilityError),
    /// No component for this product category owns a required role.
    MissingRoleOwner(RuntimeAbiRole),
    /// No component for this product category owns a required capability.
    MissingCapabilityOwner(RuntimeCapability),
    /// A selected archive could not be read for authentication.
    UnreadableArchive {
        /// Selected component whose archive could not be read.
        component: RuntimeArtifactId,
        /// Exact selected archive path.
        path: PathBuf,
        /// Stable host I/O failure category.
        kind: io::ErrorKind,
    },
    /// A selected archive is not a regular archive file.
    InvalidArchive {
        /// Selected component whose archive is invalid.
        component: RuntimeArtifactId,
        /// Exact selected archive path.
        path: PathBuf,
    },
    /// A selected archive does not match its published content digest.
    ArchiveDigestMismatch {
        /// Selected component whose archive failed authentication.
        component: RuntimeArtifactId,
        /// Exact selected archive path.
        path: PathBuf,
        /// Digest published by the runtime metadata.
        expected: RuntimeArtifactDigest,
        /// Digest calculated from the selected archive.
        actual: RuntimeArtifactDigest,
    },
}
