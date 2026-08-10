use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    /// Resolves every catalog component to an exact path and digest.
    pub fn try_new(
        metadata: RuntimeArtifactMetadata,
        components: impl IntoIterator<Item = (RuntimeArtifactId, PathBuf, RuntimeArtifactDigest)>,
    ) -> Result<Self, RuntimeArtifactBuildError> {
        let mut supplied: BTreeMap<_, _> = components
            .into_iter()
            .map(|(identity, path, digest)| (identity, (path, digest)))
            .collect();

        let mut resolved = Vec::with_capacity(metadata.components().len());

        for component in metadata.components() {
            let Some((archive, digest)) = supplied.remove(component.identity()) else {
                return Err(RuntimeArtifactBuildError::MissingComponent);
            };

            if archive.file_name().and_then(|name| name.to_str())
                != Some(component.archive_file_name())
            {
                return Err(RuntimeArtifactBuildError::ArchiveFileNameMismatch);
            }

            if digest != component.archive_digest() {
                return Err(RuntimeArtifactBuildError::ArchiveDigestMismatch);
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
    /// The selected archive content does not match the published digest.
    ArchiveDigestMismatch,
}

/// Failure to select a complete physical runtime surface for one product.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeArtifactSelectionError {
    /// The runtime contract is incompatible with reachable requirements.
    IncompatibleRuntime(crate::RuntimeCompatibilityError),
    /// No component for this product category owns a required role.
    MissingRoleOwner(RuntimeAbiRole),
    /// No component for this product category owns a required capability.
    MissingCapabilityOwner(RuntimeCapability),
}
