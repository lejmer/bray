use std::sync::Arc;

use bray_base::NonEmptySharedStr;

/// Stable opaque identity of one protected async-frame representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectedAsyncFrameId([u8; 32]);

impl ProtectedAsyncFrameId {
    /// Creates an identity from a compiler-derived stable digest.
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the stable digest bytes.
    pub const fn digest(self) -> [u8; 32] {
        self.0
    }
}

/// Version of the private binary execution ABI selected for one product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeAbiVersion {
    major: u16,
    minor: u16,
}

impl RuntimeAbiVersion {
    /// Creates an ABI version.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Returns the compatibility-breaking version component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the backwards-compatible version component.
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

/// Canonical binary symbol name selected before backend translation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BinarySymbolName(NonEmptySharedStr);

impl BinarySymbolName {
    /// Creates a symbol name unless its canonical spelling is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self)
    }

    /// Returns the exact binary symbol name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Stable identity of the selected separately linked async-runtime artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeArtifactId(NonEmptySharedStr);

impl RuntimeArtifactId {
    /// Creates an artifact identity unless its canonical value is empty.
    pub fn try_new(identity: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(identity).map(Self)
    }

    /// Returns the canonical artifact identity.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
