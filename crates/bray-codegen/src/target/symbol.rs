use std::sync::Arc;

use bray_base::{shared_str, sorted_unique_shared_slice};

/// Linkage category already selected for one generated definition or reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenLinkage {
    /// Visible only within one codegen unit.
    Private,
    /// Visible within the current linked product.
    Internal,
    /// Ordinary externally visible definition or reference.
    External,
    /// Weak externally visible definition.
    Weak,
    /// Deduplicated definition emitted in multiple codegen units.
    LinkOnce,
    /// Common zero-initialized storage.
    Common,
    /// Imported definition supplied by another binary.
    Import,
    /// Exported definition visible outside the product.
    Export,
}

/// Target symbol spelling and linkage capabilities fixed before backend translation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetSymbolConvention {
    global_prefix: Arc<str>,
    private_prefix: Arc<str>,
    supported_linkages: Arc<[CodegenLinkage]>,
}

impl TargetSymbolConvention {
    /// Creates symbol rules with a non-empty private prefix and at least one linkage category.
    pub fn try_new(
        global_prefix: impl Into<Arc<str>>,
        private_prefix: impl Into<Arc<str>>,
        supported_linkages: impl IntoIterator<Item = CodegenLinkage>,
    ) -> Result<Self, TargetSymbolConventionBuildError> {
        let global_prefix = shared_str(global_prefix);
        let private_prefix = shared_str(private_prefix);
        let supported_linkages = sorted_unique_shared_slice(supported_linkages);

        if private_prefix.is_empty() {
            return Err(TargetSymbolConventionBuildError::EmptyPrivatePrefix);
        }

        if supported_linkages.is_empty() {
            return Err(TargetSymbolConventionBuildError::MissingLinkages);
        }

        Ok(Self {
            global_prefix,
            private_prefix,
            supported_linkages,
        })
    }

    /// Returns the prefix applied to external target symbols.
    pub fn global_prefix(&self) -> &str {
        &self.global_prefix
    }

    /// Returns the prefix reserved for private generated symbols.
    pub fn private_prefix(&self) -> &str {
        &self.private_prefix
    }

    /// Returns supported linkage categories in canonical order.
    pub fn supported_linkages(&self) -> &[CodegenLinkage] {
        &self.supported_linkages
    }

    /// Returns whether the target can represent one selected linkage category.
    pub fn supports(&self, linkage: CodegenLinkage) -> bool {
        self.supported_linkages.binary_search(&linkage).is_ok()
    }
}

/// A contract violation that prevents target symbol-convention publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetSymbolConventionBuildError {
    /// Private generated symbols have no reserved target prefix.
    EmptyPrivatePrefix,
    /// No linkage category can be represented.
    MissingLinkages,
}
