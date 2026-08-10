use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_symbols::NativeLinkRequirement;

use super::archive::{ArchiveValidationError, authenticate_archive};
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
