use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_native_artifact::{NativeArtifactIndex, NativeContentDigest, ValidatedNativeArtifact};
use bray_symbols::NativeLinkRequirement;
use bray_symbols::NativeSymbolContract;
use bray_target::NativeTarget;

use super::catalog::{
    RuntimeArtifactComponentMetadata, RuntimeArtifactMetadata, RuntimeArtifactPurpose,
};
use crate::{RuntimeAbiRole, RuntimeCapability, RuntimeContract};

/// Resolved runtime catalog available to one compiler invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifact {
    metadata: RuntimeArtifactMetadata,
    directory: PathBuf,
    native_indexes: [ValidatedNativeArtifact; 2],
}

impl RuntimeArtifact {
    /// Binds a validated runtime catalog to its authenticated native indexes.
    pub fn try_new(
        metadata: RuntimeArtifactMetadata,
        directory: PathBuf,
        native_indexes: [ValidatedNativeArtifact; 2],
    ) -> Result<Self, RuntimeArtifactBuildError> {
        let target = NativeTarget::for_identity(metadata.contract().target())
            .ok_or(RuntimeArtifactBuildError::InvalidNativeTarget)?;

        if native_indexes
            .iter()
            .any(|artifact| artifact.index().target() != target)
        {
            return Err(RuntimeArtifactBuildError::IncompatibleIndexTarget);
        }

        Ok(Self {
            metadata,
            directory,
            native_indexes,
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

    /// Returns the root directory containing the selected native indexes.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Returns authenticated native indexes in product, then test-runner order.
    pub fn native_indexes(&self) -> &[ValidatedNativeArtifact; 2] {
        &self.native_indexes
    }

    /// Resolves semantic owners and their native demands for the product dependency set.
    pub fn plan(
        &self,
        purpose: RuntimeArtifactPurpose,
        requirements: &crate::RuntimeRequirements,
    ) -> Result<RuntimeArtifactPlan, RuntimeArtifactSelectionError> {
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

        let artifact = &self.native_indexes[match purpose {
            RuntimeArtifactPurpose::Product => 0,
            RuntimeArtifactPurpose::TestRunner => 1,
        }];

        let demands = requirements.roles().iter().map(|role| {
            let binding = self
                .contract()
                .role_binding(*role)
                .expect("validated runtime requirements must name a published role");

            NativeSymbolContract::required_name(
                NonEmptySharedStr::try_new(
                    artifact
                        .index()
                        .target()
                        .object_symbol_name(binding.symbol_name().as_str())
                        .as_ref(),
                )
                .expect("validated runtime symbol must be nonempty"),
            )
        });

        let demands = demands.collect::<Vec<_>>().into();

        let components = selected
            .into_iter()
            .map(|index| self.metadata.components()[index].clone())
            .collect::<Vec<_>>()
            .into();

        Ok(RuntimeArtifactPlan {
            contract: self.contract().clone(),
            components,
            // The plan retains authenticated paths after the loaded runtime is released.
            artifact: artifact.clone(),
            demands,
        })
    }

    fn owner_of_role(
        &self,
        purpose: RuntimeArtifactPurpose,
        role: RuntimeAbiRole,
    ) -> Result<usize, RuntimeArtifactSelectionError> {
        self.metadata
            .components()
            .iter()
            .position(|component| {
                component.purpose() == purpose && component.roles().binary_search(&role).is_ok()
            })
            .ok_or(RuntimeArtifactSelectionError::MissingRoleOwner(role))
    }

    fn owner_of_capability(
        &self,
        purpose: RuntimeArtifactPurpose,
        capability: RuntimeCapability,
    ) -> Result<usize, RuntimeArtifactSelectionError> {
        self.metadata
            .components()
            .iter()
            .position(|component| {
                component.purpose() == purpose
                    && component.capabilities().binary_search(&capability).is_ok()
            })
            .ok_or(RuntimeArtifactSelectionError::MissingCapabilityOwner(
                capability,
            ))
    }
}

/// Validated semantic runtime providers awaiting the complete product's native closure.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactPlan {
    contract: RuntimeContract,
    components: Arc<[RuntimeArtifactComponentMetadata]>,
    artifact: ValidatedNativeArtifact,
    demands: Arc<[NativeSymbolContract]>,
}

impl RuntimeArtifactPlan {
    /// Returns semantic providers selected for roles and capabilities.
    pub fn components(&self) -> &[RuntimeArtifactComponentMetadata] {
        &self.components
    }

    /// Returns native candidates to include with the product's other dependency indexes.
    pub fn native_index(&self) -> &NativeArtifactIndex {
        self.artifact.index()
    }

    /// Returns required native symbols for the selected semantic runtime roles.
    pub fn native_demands(&self) -> &[NativeSymbolContract] {
        &self.demands
    }

    /// Retains runtime units selected by the common product resolver, in its link order.
    /// Every digest must belong to this plan's authenticated index.
    pub fn finish(
        self,
        units: impl IntoIterator<Item = NativeContentDigest>,
    ) -> RuntimeArtifactSelection {
        let native_units = units
            .into_iter()
            .map(|digest| {
                let index = self.artifact.index();

                let unit = index
                    .units()
                    .binary_search_by_key(&digest, |unit| unit.digest())
                    .expect("selected runtime unit must belong to its authenticated index");

                RuntimeNativeUnit {
                    path: self
                        .artifact
                        .payload(digest)
                        .expect("authenticated native unit must retain its payload")
                        .to_path_buf(),
                    kind: index.units()[unit].kind(),
                    native_links: Arc::from(index.units()[unit].native_links()),
                }
            })
            .collect::<Vec<_>>()
            .into();

        RuntimeArtifactSelection {
            contract: self.contract,
            components: self.components,
            native_units,
        }
    }
}

/// Exact runtime components selected for one product link.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactSelection {
    contract: RuntimeContract,
    components: Arc<[RuntimeArtifactComponentMetadata]>,
    native_units: Arc<[RuntimeNativeUnit]>,
}

/// One authenticated runtime native unit selected by generic symbol demand.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeNativeUnit {
    path: PathBuf,
    kind: bray_native_artifact::NativeUnitKind,
    native_links: Arc<[NativeLinkRequirement]>,
}

impl RuntimeNativeUnit {
    /// Returns the authenticated physical representation.
    pub const fn kind(&self) -> bray_native_artifact::NativeUnitKind {
        self.kind
    }

    /// Returns the authenticated native file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RuntimeArtifactSelection {
    /// Returns the selected runtime contract.
    pub const fn contract(&self) -> &RuntimeContract {
        &self.contract
    }

    /// Returns semantic providers selected for roles and capabilities.
    pub fn components(&self) -> &[RuntimeArtifactComponentMetadata] {
        &self.components
    }

    /// Returns selected authenticated native units in link order.
    pub fn native_units(&self) -> &[RuntimeNativeUnit] {
        &self.native_units
    }

    /// Returns native dependencies across selected units in first-use link order.
    pub fn native_links(&self) -> impl Iterator<Item = &NativeLinkRequirement> {
        let mut seen = BTreeSet::new();

        self.native_units
            .iter()
            .flat_map(|unit| unit.native_links.iter())
            .filter(move |link| seen.insert((**link).clone()))
    }
}

/// A mismatch between runtime metadata and its imported native indexes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactBuildError {
    /// The runtime target has no supported native representation.
    InvalidNativeTarget,
    /// An imported index targets another native platform.
    IncompatibleIndexTarget,
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
}
