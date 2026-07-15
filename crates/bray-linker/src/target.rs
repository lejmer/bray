use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_target::{CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity};

/// Native linkage model selected before link-plan construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkModel {
    /// Target-defined default linkage behavior.
    Default,
    /// Resolve the product without dynamic runtime dependencies where supported.
    Static,
    /// Permit dynamic runtime and native-library dependencies.
    Dynamic,
}

/// Complete backend-neutral target facts needed by linker drivers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkTarget {
    identity: TargetIdentity,
    triple: NonEmptySharedStr,
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
    relocation_model: RelocationModel,
    code_model: CodeModel,
    link_model: LinkModel,
}

impl LinkTarget {
    /// Creates a link target when its canonical target triple is non-empty.
    pub fn try_new(
        identity: TargetIdentity,
        triple: impl Into<Arc<str>>,
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        relocation_model: RelocationModel,
        code_model: CodeModel,
        link_model: LinkModel,
    ) -> Result<Self, LinkTargetBuildError> {
        let Some(triple) = NonEmptySharedStr::try_new(triple) else {
            return Err(LinkTargetBuildError::EmptyTriple);
        };

        Ok(Self {
            identity,
            triple,
            architecture,
            object_format,
            relocation_model,
            code_model,
            link_model,
        })
    }

    /// Returns the stable target identity.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the canonical target triple.
    pub fn triple(&self) -> &str {
        self.triple.as_str()
    }

    /// Returns the target processor architecture.
    pub const fn architecture(&self) -> TargetArchitecture {
        self.architecture
    }

    /// Returns the native object format consumed and produced by the link operation.
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    /// Returns the selected relocation policy.
    pub const fn relocation_model(&self) -> RelocationModel {
        self.relocation_model
    }

    /// Returns the selected addressing-range policy.
    pub const fn code_model(&self) -> CodeModel {
        self.code_model
    }

    /// Returns the selected native linkage model.
    pub const fn link_model(&self) -> LinkModel {
        self.link_model
    }
}

/// A contract violation that prevents link-target publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkTargetBuildError {
    /// The canonical target triple is empty.
    EmptyTriple,
}

#[cfg(test)]
mod tests {
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };

    use super::{LinkModel, LinkTarget, LinkTargetBuildError};

    #[test]
    fn link_targets_require_canonical_target_triples() {
        let Some(identity) = TargetIdentity::try_new("linux-x86_64") else {
            panic!("test target identity must be valid");
        };

        assert_eq!(
            LinkTarget::try_new(
                identity,
                "",
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                RelocationModel::PositionIndependent,
                CodeModel::Small,
                LinkModel::Dynamic,
            ),
            Err(LinkTargetBuildError::EmptyTriple)
        );
    }
}
