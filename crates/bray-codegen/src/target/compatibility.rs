use std::sync::Arc;

use bray_base::{shared_str, sorted_unique_shared_slice};

/// Versioned agreement between a target profile and backend-neutral code generation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetCompatibility {
    target_profile_revision: Arc<str>,
    codegen_contract_revision: u32,
    backend_requirements: Arc<[Arc<str>]>,
}

impl TargetCompatibility {
    /// Creates compatibility metadata from non-empty canonical revision and requirement names.
    pub fn try_new<Requirements, Requirement>(
        target_profile_revision: impl Into<Arc<str>>,
        codegen_contract_revision: u32,
        backend_requirements: Requirements,
    ) -> Option<Self>
    where
        Requirements: IntoIterator<Item = Requirement>,
        Requirement: Into<Arc<str>>,
    {
        let target_profile_revision = shared_str(target_profile_revision);
        let backend_requirements: Vec<_> =
            backend_requirements.into_iter().map(Into::into).collect();

        if target_profile_revision.is_empty()
            || backend_requirements
                .iter()
                .any(|requirement| requirement.is_empty())
        {
            return None;
        }

        Some(Self {
            target_profile_revision,
            codegen_contract_revision,
            backend_requirements: sorted_unique_shared_slice(backend_requirements),
        })
    }

    /// Returns the language target-profile revision validated by compilation.
    pub fn target_profile_revision(&self) -> &str {
        &self.target_profile_revision
    }

    /// Returns the backend-neutral codegen contract revision required by the target.
    pub const fn codegen_contract_revision(&self) -> u32 {
        self.codegen_contract_revision
    }

    /// Returns additional backend requirements in canonical order.
    pub fn backend_requirements(&self) -> impl ExactSizeIterator<Item = &str> {
        self.backend_requirements.iter().map(AsRef::as_ref)
    }
}
