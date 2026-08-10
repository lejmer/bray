use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::NativeLinkRequirement;
use serde::{Deserialize, Serialize};

use crate::{
    PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY, PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
    PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
};

use super::model::{
    StandardLibraryArtifact, StandardLibraryBundleDigest, StandardLibraryManifestError,
    StandardLibraryTargetArtifacts,
};

pub(super) const MANIFEST_FORMAT_REVISION: u32 = 2;
const DIGEST_ALGORITHM: &str = "blake3";

#[derive(Serialize)]
struct PayloadWire<'manifest> {
    format: u32,
    package: &'static str,
    product: &'static str,
    product_kind: &'static str,
    public_surface: &'static str,
    targets: Vec<TargetWire<'manifest>>,
}

#[derive(Serialize)]
struct PublishedWire<'manifest> {
    format: u32,
    package: &'static str,
    product: &'static str,
    product_kind: &'static str,
    public_surface: &'static str,
    targets: Vec<TargetWire<'manifest>>,
    bundle_digest: DigestWire,
}

#[derive(Serialize)]
struct ArtifactWire<'manifest> {
    kind: &'static str,
    path: &'manifest str,
    byte_len: u64,
    digest: DigestWire,
}

#[derive(Serialize)]
struct TargetWire<'manifest> {
    target: &'manifest str,
    runtime_abi: RuntimeAbiWire,
    artifacts: Vec<ArtifactWire<'manifest>>,
    native_links: Vec<NativeLinkWire<'manifest>>,
}

#[derive(Serialize)]
struct NativeLinkWire<'manifest> {
    name: &'manifest str,
    kind: &'static str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedPublishedWire {
    pub format: u32,
    pub package: String,
    pub product: String,
    pub product_kind: String,
    pub public_surface: String,
    pub targets: Vec<OwnedTargetWire>,
    pub bundle_digest: OwnedDigestWire,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedArtifactWire {
    pub kind: String,
    pub path: String,
    pub byte_len: u64,
    pub digest: OwnedDigestWire,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedTargetWire {
    pub target: String,
    pub runtime_abi: RuntimeAbiWire,
    pub artifacts: Vec<OwnedArtifactWire>,
    pub native_links: Vec<OwnedNativeLinkWire>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedNativeLinkWire {
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeAbiWire {
    pub major: u16,
    pub minor: u16,
}

#[derive(Serialize)]
struct DigestWire {
    algorithm: &'static str,
    bytes: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedDigestWire {
    pub algorithm: String,
    pub bytes: String,
}

pub(super) fn encode_payload(
    targets: &[StandardLibraryTargetArtifacts],
) -> Result<Vec<u8>, StandardLibraryManifestError> {
    serde_json::to_vec(&payload_wire(targets)).map_err(|_| StandardLibraryManifestError::Malformed)
}

pub(super) fn encode_published(
    targets: &[StandardLibraryTargetArtifacts],
    bundle_digest: StandardLibraryBundleDigest,
) -> Result<Vec<u8>, StandardLibraryManifestError> {
    let payload = payload_wire(targets);

    let published = PublishedWire {
        format: payload.format,
        package: payload.package,
        product: payload.product,
        product_kind: payload.product_kind,
        public_surface: payload.public_surface,
        targets: payload.targets,
        bundle_digest: digest_wire(bundle_digest.bytes()),
    };

    serde_json::to_vec(&published).map_err(|_| StandardLibraryManifestError::Malformed)
}

pub(super) fn decode_digest(
    digest: OwnedDigestWire,
) -> Result<[u8; 32], StandardLibraryManifestError> {
    if digest.algorithm != DIGEST_ALGORITHM || digest.bytes.len() != 64 {
        return Err(StandardLibraryManifestError::InvalidDigest);
    }

    let mut bytes = [0_u8; 32];

    for (destination, pair) in bytes
        .iter_mut()
        .zip(digest.bytes.as_bytes().chunks_exact(2))
    {
        *destination = decode_hex_byte(pair).ok_or(StandardLibraryManifestError::InvalidDigest)?;
    }

    Ok(bytes)
}

pub(super) const fn runtime_abi(wire: RuntimeAbiWire) -> RuntimeAbiVersion {
    RuntimeAbiVersion::new(wire.major, wire.minor)
}

fn payload_wire<'manifest>(
    targets: &'manifest [StandardLibraryTargetArtifacts],
) -> PayloadWire<'manifest> {
    PayloadWire {
        format: MANIFEST_FORMAT_REVISION,
        package: PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        product: PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY,
        product_kind: "library",
        public_surface: PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
        targets: targets.iter().map(target_wire).collect(),
    }
}

fn artifact_wire(artifact: &StandardLibraryArtifact) -> ArtifactWire<'_> {
    ArtifactWire {
        kind: artifact.kind().as_str(),
        path: artifact.path(),
        byte_len: artifact.byte_len(),
        digest: digest_wire(artifact.digest().bytes()),
    }
}

fn target_wire(target: &StandardLibraryTargetArtifacts) -> TargetWire<'_> {
    let runtime_abi = target.runtime_abi();

    TargetWire {
        target: target.target().as_str(),
        runtime_abi: RuntimeAbiWire {
            major: runtime_abi.major(),
            minor: runtime_abi.minor(),
        },
        artifacts: target.artifacts().iter().map(artifact_wire).collect(),
        native_links: target.native_links().iter().map(native_link_wire).collect(),
    }
}

fn native_link_wire(requirement: &NativeLinkRequirement) -> NativeLinkWire<'_> {
    NativeLinkWire {
        name: requirement.name(),
        kind: requirement.kind().as_str(),
    }
}

fn digest_wire(bytes: [u8; 32]) -> DigestWire {
    let mut encoded = String::with_capacity(64);

    for byte in bytes {
        use std::fmt::Write;

        let _ = write!(encoded, "{byte:02x}");
    }

    DigestWire {
        algorithm: DIGEST_ALGORITHM,
        bytes: encoded,
    }
}

fn decode_hex_byte(pair: &[u8]) -> Option<u8> {
    let [high, low] = pair else {
        return None;
    };

    Some(decode_hex_digit(*high)? << 4 | decode_hex_digit(*low)?)
}

const fn decode_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}
