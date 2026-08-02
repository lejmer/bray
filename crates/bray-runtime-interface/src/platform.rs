use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

/// One closed platform-service role understood by the compiler and native provider.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformServiceRole {
    /// Measures the immutable process-context block.
    ContextMeasure,
    /// Copies the immutable process-context block into caller-owned storage.
    ContextCopy,
    /// Reads bytes from a borrowed stream handle.
    StreamRead,
    /// Writes bytes to a borrowed stream handle.
    StreamWrite,
    /// Flushes a borrowed stream handle.
    StreamFlush,
    /// Acquires product-wide serialization for a borrowed stream handle.
    StreamLock,
    /// Releases product-wide serialization for a borrowed stream handle.
    StreamUnlock,
}

impl PlatformServiceRole {
    /// Returns the stable numeric role identity.
    pub const fn id(self) -> u32 {
        match self {
            Self::ContextMeasure => 0x0001,
            Self::ContextCopy => 0x0002,
            Self::StreamRead => 0x0101,
            Self::StreamWrite => 0x0102,
            Self::StreamFlush => 0x0103,
            Self::StreamLock => 0x0106,
            Self::StreamUnlock => 0x0107,
        }
    }

    /// Returns the canonical role name used by product metadata.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextMeasure => "platform.context.measure",
            Self::ContextCopy => "platform.context.copy",
            Self::StreamRead => "platform.stream.read",
            Self::StreamWrite => "platform.stream.write",
            Self::StreamFlush => "platform.stream.flush",
            Self::StreamLock => "platform.stream.lock",
            Self::StreamUnlock => "platform.stream.unlock",
        }
    }

    /// Returns the role with an exact canonical metadata name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "platform.context.measure" => Some(Self::ContextMeasure),
            "platform.context.copy" => Some(Self::ContextCopy),
            "platform.stream.read" => Some(Self::StreamRead),
            "platform.stream.write" => Some(Self::StreamWrite),
            "platform.stream.flush" => Some(Self::StreamFlush),
            "platform.stream.lock" => Some(Self::StreamLock),
            "platform.stream.unlock" => Some(Self::StreamUnlock),
            _ => None,
        }
    }

    /// Returns the exact private callable shape required by this role.
    pub const fn signature(self) -> PlatformServiceSignature {
        use PlatformAbiType::{PointerU8, PointerU64, Status, U64};

        const CONTEXT_MEASURE: &[PlatformAbiType] = &[PointerU64];
        const CONTEXT_COPY: &[PlatformAbiType] = &[PointerU8, U64, PointerU64];
        const STREAM_TRANSFER: &[PlatformAbiType] = &[U64, PointerU8, U64, PointerU64];
        const STREAM_HANDLE: &[PlatformAbiType] = &[U64];

        let parameters = match self {
            Self::ContextMeasure => CONTEXT_MEASURE,
            Self::ContextCopy => CONTEXT_COPY,
            Self::StreamRead | Self::StreamWrite => STREAM_TRANSFER,
            Self::StreamFlush | Self::StreamLock | Self::StreamUnlock => STREAM_HANDLE,
        };

        PlatformServiceSignature::new(parameters, Status)
    }
}

/// One ABI value kind used by the closed platform-service callable schema.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformAbiType {
    /// Fixed-width unsigned 64-bit scalar.
    U64,
    /// Raw pointer to byte storage.
    PointerU8,
    /// Raw pointer to unsigned 64-bit storage.
    PointerU64,
    /// The fixed-layout platform status record.
    Status,
}

/// The exact parameter and result shape of one platform-service role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformServiceSignature {
    parameters: &'static [PlatformAbiType],
    result: PlatformAbiType,
}

impl PlatformServiceSignature {
    const fn new(parameters: &'static [PlatformAbiType], result: PlatformAbiType) -> Self {
        Self { parameters, result }
    }

    /// Returns parameter ABI kinds in calling order.
    pub const fn parameters(self) -> &'static [PlatformAbiType] {
        self.parameters
    }

    /// Returns the result ABI kind.
    pub const fn result(self) -> PlatformAbiType {
        self.result
    }
}

/// An explicit product association between a private declaration and platform role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlatformServiceBinding {
    role: PlatformServiceRole,
    module: Arc<[NonEmptySharedStr]>,
    declaration: NonEmptySharedStr,
}

impl PlatformServiceBinding {
    /// Creates a binding from a role and dotted declaration path.
    pub fn try_new(role: PlatformServiceRole, path: &str) -> Option<Self> {
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

    /// Returns the closed platform role selected by this binding.
    pub const fn role(&self) -> PlatformServiceRole {
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

/// Status returned by every native platform-service operation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformStatus {
    category: u32,
    reserved: u32,
    native_code: i64,
}

impl NativePlatformStatus {
    /// Successful platform operation.
    pub const SUCCESS: Self = Self::new(0, 0);
    /// Unsupported platform operation.
    pub const UNSUPPORTED: Self = Self::new(1, 0);
    /// Invalid platform-service input.
    pub const INVALID_INPUT: Self = Self::new(5, 0);
    /// Platform resource exhaustion.
    pub const EXHAUSTED: Self = Self::new(7, 0);
    /// Broken stream operation.
    pub const BROKEN_STREAM: Self = Self::new(8, 0);
    /// Caller-provided context buffer is too small.
    pub const INSUFFICIENT_BUFFER: Self = Self::new(10, 0);
    /// Other target failure.
    pub const OTHER: Self = Self::new(12, 0);

    /// Creates one stable category with optional target-native inspection data.
    pub const fn new(category: u32, native_code: i64) -> Self {
        Self {
            category,
            reserved: 0,
            native_code,
        }
    }

    /// Returns the stable status category.
    pub const fn category(self) -> u32 {
        self.category
    }

    /// Returns target-native inspection data, or zero when unavailable.
    pub const fn native_code(self) -> i64 {
        self.native_code
    }
}

#[cfg(test)]
mod tests {
    use super::{PlatformAbiType, PlatformServiceBinding, PlatformServiceRole};

    #[test]
    fn platform_roles_have_stable_names_ids_and_shapes() {
        let role = PlatformServiceRole::StreamWrite;

        assert_eq!(role.id(), 0x0102);
        assert_eq!(role.as_str(), "platform.stream.write");
        assert_eq!(PlatformServiceRole::from_name(role.as_str()), Some(role));
        assert_eq!(
            role.signature().parameters(),
            [
                PlatformAbiType::U64,
                PlatformAbiType::PointerU8,
                PlatformAbiType::U64,
                PlatformAbiType::PointerU64,
            ]
        );
        assert_eq!(role.signature().result(), PlatformAbiType::Status);
    }

    #[test]
    fn platform_bindings_require_module_qualified_declarations() {
        let Some(binding) = PlatformServiceBinding::try_new(
            PlatformServiceRole::StreamFlush,
            "std.io.platform_stream_flush",
        ) else {
            panic!("test binding path must be valid");
        };

        assert_eq!(binding.module().collect::<Vec<_>>(), ["std", "io"]);
        assert_eq!(binding.declaration(), "platform_stream_flush");
        assert_eq!(binding.dotted_path(), "std.io.platform_stream_flush");
        assert_eq!(
            PlatformServiceBinding::try_new(PlatformServiceRole::StreamFlush, "flush"),
            None
        );
    }
}
