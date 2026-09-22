use std::ops::Range;
use std::sync::Arc;
use std::sync::OnceLock;

use bray_symbols::InterfaceSymbolId;

use crate::implementation::PackageImplementationIdentity;
use crate::{InterfaceValidationError, InterfaceValidationLimits};

pub(in crate::implementation) const ARTIFACT_HASH_OFFSET: usize = 80;
pub(in crate::implementation) const BYTE_ORDER_MARKER: u32 = 0x0102_0304;
pub(in crate::implementation) const CONTENT_HASH_OFFSET: usize = 48;
pub(in crate::implementation) const DIRECTORY_ENTRY_LENGTH: usize = 148;
pub(in crate::implementation) const HEADER_LENGTH: usize = 112;
pub(in crate::implementation) const MAGIC: [u8; 8] = *b"BRAYM\0\r\n";
pub(in crate::implementation) const REQUIRED_FLAGS: u64 = 0;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(in crate::implementation) enum ImplementationPayloadKind {
    Identity = 0,
    ConstantCallableBody = 1,
    ExecutableTemplate = 2,
    NativeBoundary = 3,
    PreSpecializedMir = 4,
    NativeIndex = 5,
    NativeUnit = 6,
    NativeBinding = 7,
}

impl ImplementationPayloadKind {
    pub(in crate::implementation) const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Identity),
            1 => Some(Self::ConstantCallableBody),
            2 => Some(Self::ExecutableTemplate),
            3 => Some(Self::NativeBoundary),
            4 => Some(Self::PreSpecializedMir),
            5 => Some(Self::NativeIndex),
            6 => Some(Self::NativeUnit),
            7 => Some(Self::NativeBinding),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::implementation) struct ImplementationDirectoryEntry {
    pub(in crate::implementation) index: u64,
    pub(in crate::implementation) owner: InterfaceSymbolId,
    pub(in crate::implementation) raw_kind: u8,
    pub(in crate::implementation) kind: Option<ImplementationPayloadKind>,
    pub(in crate::implementation) compatibility: crate::InterfaceSectionCompatibility,
    pub(in crate::implementation) encoding: crate::InterfaceSectionEncoding,
    pub(in crate::implementation) discriminator: [u8; 32],
    pub(in crate::implementation) family_size: u32,
    pub(in crate::implementation) platform_service:
        Option<bray_runtime_interface::PlatformServiceRole>,
    pub(in crate::implementation) decoded_length: u64,
    pub(in crate::implementation) record_count: u64,
    pub(in crate::implementation) checksum: [u8; 32],
    pub(in crate::implementation) content_hash: [u8; 32],
    pub(in crate::implementation) payload: Range<usize>,
}

/// An immutable package implementation artifact associated with one semantic interface.
#[derive(Clone, Debug)]
pub struct PackageImplementationArtifact {
    pub(super) bytes: Arc<[u8]>,
    pub(super) identity: PackageImplementationIdentity,
    pub(super) content_hash: [u8; 32],
    pub(super) artifact_hash: [u8; 32],
    pub(super) directory: Arc<[ImplementationDirectoryEntry]>,
    pub(super) decoded: Arc<[OnceLock<Result<Arc<[u8]>, InterfaceValidationError>>]>,
    pub(super) limits: InterfaceValidationLimits,
}

impl PartialEq for PackageImplementationArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.identity == other.identity && self.limits == other.limits
    }
}

impl Eq for PackageImplementationArtifact {}

impl std::hash::Hash for PackageImplementationArtifact {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
        self.identity.hash(state);
        self.limits.hash(state);
    }
}

pub(in crate::implementation) const fn executable_discriminator(raw: u32) -> [u8; 32] {
    let bytes = raw.to_le_bytes();
    let mut discriminator = [0; 32];

    discriminator[0] = bytes[0];
    discriminator[1] = bytes[1];
    discriminator[2] = bytes[2];
    discriminator[3] = bytes[3];

    discriminator
}
