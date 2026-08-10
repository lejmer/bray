use std::sync::Arc;

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

    /// Returns whether this provided version satisfies one required version.
    pub const fn supports(self, required: Self) -> bool {
        self.major == required.major && self.minor >= required.minor
    }
}

/// Canonical binary symbol name selected before backend translation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BinarySymbolName(Arc<str>);

impl BinarySymbolName {
    /// Creates a symbol name unless its canonical spelling is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        nonempty_shared_str(name).map(Self)
    }

    /// Returns the exact binary symbol name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity of the selected separately linked async-runtime artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeArtifactId(Arc<str>);

impl RuntimeArtifactId {
    /// Creates an artifact identity unless its canonical value is empty.
    pub fn try_new(identity: impl Into<Arc<str>>) -> Option<Self> {
        nonempty_shared_str(identity).map(Self)
    }

    /// Returns the canonical artifact identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity of one execution-runtime implementation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeIdentity(Arc<str>);

impl RuntimeIdentity {
    /// Creates a runtime identity unless its canonical value is empty.
    pub fn try_new(identity: impl Into<Arc<str>>) -> Option<Self> {
        nonempty_shared_str(identity).map(Self)
    }

    /// Returns the canonical runtime identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity of one target panic ABI.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PanicAbiIdentity(Arc<str>);

impl PanicAbiIdentity {
    /// Creates a panic ABI identity unless its canonical value is empty.
    pub fn try_new(identity: impl Into<Arc<str>>) -> Option<Self> {
        nonempty_shared_str(identity).map(Self)
    }

    /// Returns the canonical panic ABI identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn nonempty_shared_str(value: impl Into<Arc<str>>) -> Option<Arc<str>> {
    let value = value.into();

    (!value.is_empty()).then_some(value)
}
