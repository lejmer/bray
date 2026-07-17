use bray_runtime_interface::{ExecutableHostContract, ProtectedAsyncFrameId};

/// Representation category of one MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirUnitKind {
    /// Ordinary synchronous control flow with no protected async frame.
    Synchronous,
    /// Compiler-protected inactive and resumable async frame.
    ProtectedAsyncFrame(ProtectedAsyncFrameId),
    /// Compiler-generated native host stub for one executable or test root.
    ExecutableHost(ExecutableHostContract),
}

impl MirUnitKind {
    /// Returns the protected-frame identity when this unit represents one.
    pub const fn protected_frame(&self) -> Option<ProtectedAsyncFrameId> {
        match self {
            Self::ProtectedAsyncFrame(frame) => Some(*frame),
            Self::Synchronous | Self::ExecutableHost(_) => None,
        }
    }
}
