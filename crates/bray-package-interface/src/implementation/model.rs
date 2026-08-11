use std::sync::Arc;

use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{ForeignCallableDirection, InterfaceSymbolId};

use crate::{InterfaceCheckedTemplate, InterfaceValidationError};

/// One checked const-callable body addressed by its public interface identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceConstantCallableBody {
    owner: InterfaceSymbolId,
    template: InterfaceCheckedTemplate,
}

impl InterfaceConstantCallableBody {
    /// Creates one source-independent const-callable body payload.
    pub const fn new(owner: InterfaceSymbolId, template: InterfaceCheckedTemplate) -> Self {
        Self { owner, template }
    }

    /// Returns the callable declaration that owns this body.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the checked body template.
    pub const fn template(&self) -> &InterfaceCheckedTemplate {
        &self.template
    }
}

/// One source-independent executable body addressed by interface identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InterfaceExecutableTemplate {
    owner: InterfaceSymbolId,
    identity: bray_ir::MirExecutableTemplateId,
    family_size: u32,
    payload: Arc<[u8]>,
}

impl InterfaceExecutableTemplate {
    /// Creates one encoded executable template for a declaration.
    pub fn new(
        owner: InterfaceSymbolId,
        identity: bray_ir::MirExecutableTemplateId,
        family_size: u32,
        payload: impl Into<Arc<[u8]>>,
    ) -> Option<Self> {
        let payload = payload.into();

        (!payload.is_empty() && family_size > 0 && identity.raw() < family_size).then_some(Self {
            owner,
            identity,
            family_size,
            payload,
        })
    }

    /// Returns the declaration that owns this template.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the declaration-local executable-template identity.
    pub const fn identity(&self) -> bray_ir::MirExecutableTemplateId {
        self.identity
    }

    /// Returns the number of independently addressable templates in this declaration's family.
    pub const fn family_size(&self) -> u32 {
        self.family_size
    }

    /// Returns the canonical source-independent executable payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// One native symbol boundary retained by an executable package implementation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InterfaceNativeBoundary {
    owner: InterfaceSymbolId,
    direction: ForeignCallableDirection,
    symbol: BinarySymbolName,
}

impl InterfaceNativeBoundary {
    /// Creates the native boundary of one interface declaration.
    pub const fn new(
        owner: InterfaceSymbolId,
        direction: ForeignCallableDirection,
        symbol: BinarySymbolName,
    ) -> Self {
        Self {
            owner,
            direction,
            symbol,
        }
    }

    /// Returns the declaration that owns this boundary.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns whether the native symbol enters or leaves the package implementation.
    pub const fn direction(&self) -> ForeignCallableDirection {
        self.direction
    }

    /// Returns the exact native symbol spelling.
    pub const fn symbol(&self) -> &BinarySymbolName {
        &self.symbol
    }
}

/// Failure while assembling a package implementation artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageImplementationArtifactBuildError {
    /// Two payloads claim the same callable identity.
    DuplicateCallableBody(InterfaceSymbolId),
    /// Two executable templates claim the same declaration identity.
    DuplicateExecutableTemplate(InterfaceSymbolId),
    /// Executable templates for one declaration do not start at the root or contain a gap.
    InvalidExecutableTemplateFamily(InterfaceSymbolId),
    /// Two native boundaries claim the same declaration identity.
    DuplicateNativeBoundary(InterfaceSymbolId),
    /// Two pre-specialized MIR payloads claim the same complete specialization identity.
    DuplicateSpecialization,
    /// A pre-specialized payload does not belong to this bundle configuration and dependency graph.
    SpecializationIdentityMismatch,
    /// An executable template owner is missing or cannot own executable code.
    InvalidExecutableOwner(InterfaceSymbolId),
    /// A native boundary owner is missing or is not a function.
    InvalidNativeBoundaryOwner(InterfaceSymbolId),
    /// A payload owner is missing, is not callable, or disagrees with the body category.
    InvalidCallableOwner(InterfaceSymbolId),
    /// A checked body does not form a valid source-independent template graph.
    InvalidBody(InterfaceValidationError),
    /// The encoded artifact does not form a canonical demand-addressable container.
    InvalidArtifact(InterfaceValidationError),
}
