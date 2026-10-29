use std::sync::Arc;

use crate::RuntimeAbiRole;
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

/// The authority and execution boundary established by a generated native entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeEntryKind {
    /// Enter an ordinary foreign callback through the runtime attachment service.
    ForeignCallback,
    /// Finish an existing exact-thread owner without recursively attaching.
    ThreadStorageExit,
    /// Contain bootstrap failures and return the native runtime failure status.
    RuntimeStatus,
    /// Contain bootstrap identity lookup failures and return the unavailable identity zero.
    RuntimeIdentity,
    /// Preserve the body outcome and return entry cleanup status independently.
    EntryStatus,
}

impl NativeEntryKind {
    /// Selects the non-attaching entry boundary required by a bootstrap role.
    pub const fn for_runtime_role(role: RuntimeAbiRole) -> Option<Self> {
        match role {
            RuntimeAbiRole::ThreadAttachmentIdentity => Some(Self::RuntimeIdentity),
            RuntimeAbiRole::ThreadStorageExit => Some(Self::ThreadStorageExit),
            RuntimeAbiRole::SynchronousRootExecution
            | RuntimeAbiRole::ForeignCallbackExecution
            | RuntimeAbiRole::EntryContinuation => Some(Self::EntryStatus),
            RuntimeAbiRole::EntryContextRegistration
            | RuntimeAbiRole::RuntimeInitialization
            | RuntimeAbiRole::ThreadAttachmentEnter
            | RuntimeAbiRole::ThreadAttachmentLeave
            | RuntimeAbiRole::ThreadAttachmentMarkMain
            | RuntimeAbiRole::ThreadAttachmentCleanup
            | RuntimeAbiRole::ThreadStaticCleanupRegistration
            | RuntimeAbiRole::ProductHostControl
            | RuntimeAbiRole::ProductCleanupContinuation
            | RuntimeAbiRole::StructuredShutdown => Some(Self::RuntimeStatus),
            _ => None,
        }
    }
}
