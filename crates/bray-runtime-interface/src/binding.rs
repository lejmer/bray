use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

/// One explicit association between a source declaration and a closed semantic role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceRoleBinding<Role> {
    role: Role,
    module: Arc<[NonEmptySharedStr]>,
    declaration: NonEmptySharedStr,
}

impl<Role: Copy> SourceRoleBinding<Role> {
    /// Creates a binding from a role and dotted declaration path.
    pub fn try_new(role: Role, path: &str) -> Option<Self> {
        let mut segments = path.split('.').map(NonEmptySharedStr::try_new);
        let mut present = segments.by_ref().collect::<Option<Vec<_>>>()?;

        let declaration = present.pop()?;

        if present.is_empty() {
            return None;
        }

        Some(Self {
            role,
            module: shared_slice(present),
            declaration,
        })
    }

    /// Returns the closed semantic role selected by this binding.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Returns the declaration's module path segments.
    pub fn module(&self) -> impl ExactSizeIterator<Item = &str> {
        self.module.iter().map(NonEmptySharedStr::as_str)
    }

    /// Returns the declaration name within its module.
    pub fn declaration(&self) -> &str {
        self.declaration.as_str()
    }

    /// Returns the canonical dotted declaration path.
    pub fn dotted_path(&self) -> String {
        self.module()
            .chain(std::iter::once(self.declaration()))
            .collect::<Vec<_>>()
            .join(".")
    }
}
