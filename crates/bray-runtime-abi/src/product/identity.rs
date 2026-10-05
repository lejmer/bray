/// Stable compiled-product identity, independent of any particular image load.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeProductIdentity([u8; 32]);

impl NativeProductIdentity {
    /// Creates an identity from its exact compiler-generated digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity digest.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Stable identity of one closed static instance.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeStaticIdentity([u8; 32]);

impl NativeStaticIdentity {
    /// Creates an identity from its exact compiler-generated digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity digest.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Stable structural identity of one erased Bray type.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeTypeIdentity([u8; 32]);

impl NativeTypeIdentity {
    /// Creates an identity from its exact compiler-generated digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity digest.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}
