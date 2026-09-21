use std::fmt;
use std::sync::Arc;

use bray_symbols::{NativeLinkRequirement, NativeSymbolContract, NativeSymbolIdentity};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

/// SHA-256 digest of exact artifact bytes or serialized producer configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeContentDigest([u8; 32]);

impl NativeContentDigest {
    /// Creates a digest from its exact bytes.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns its exact bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }

    /// Parses one lowercase hexadecimal digest.
    pub fn from_hex(value: &str) -> Option<Self> {
        bray_base::decode_lowercase_hex(value).map(Self)
    }

    /// Returns the lowercase hexadecimal digest used by artifact metadata.
    pub fn to_hex(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for NativeContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }

        Ok(())
    }
}

/// Physical native payload representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeUnitKind {
    /// A target object file.
    Object,
    /// Target-specific LLVM bitcode.
    Bitcode,
    /// An opaque external library, retained as one indivisible input.
    OpaqueArchive,
}

impl NativeUnitKind {
    /// Content-addressed file name relative to an artifact's native payload directory.
    pub fn file_name(self, digest: NativeContentDigest, target: NativeTarget) -> String {
        let kind = match self {
            Self::Object => TargetOutputKind::RelocatableObject,
            Self::Bitcode => TargetOutputKind::BackendBitcode,
            Self::OpaqueArchive => TargetOutputKind::StaticLibrary,
        };

        TargetOutputName::for_native(target.object_format(), kind)
            .file_name(&digest.to_string())
            .expect("a lowercase content digest is a valid output name stem")
    }
}

/// Why an exact native unit is mandatory without a symbol reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeRoot {
    /// Native initialization runs before the product entry.
    Initialization,
    /// Native finalization runs after the product entry.
    Finalization,
}

/// Additional native definition selection beyond the symbol's strong or weak binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeComdatSelection {
    /// An arbitrary provider may win.
    Any,
    /// Providers must have equal size.
    SameSize,
    /// Providers must have identical contents.
    ExactMatch,
    /// The largest provider wins.
    Largest,
    /// More than one provider is an error.
    NoDuplicates,
}

/// Additional native definition selection beyond the symbol's strong or weak binding.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeDefinitionSelection {
    /// Ordinary strong or weak definition.
    Ordinary,
    /// Used only after ordinary providers fail to resolve the symbol.
    Fallback,
    /// A COMDAT definition, optionally retained with its parent definition.
    Comdat {
        /// Exact target COMDAT group identity.
        group: NativeSymbolIdentity,
        /// Native duplicate-provider selection rule.
        rule: NativeComdatSelection,
        /// An associative parent's exact symbol identity, when present.
        associative_with: Option<NativeSymbolIdentity>,
    },
}

/// Native definition with target name or ordinal, version, binding, and selection semantics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeDefinition {
    symbol: NativeSymbolContract,
    selection: NativeDefinitionSelection,
}

impl NativeDefinition {
    /// Creates a definition from a validated native symbol contract.
    pub const fn new(symbol: NativeSymbolContract, selection: NativeDefinitionSelection) -> Self {
        Self { symbol, selection }
    }

    /// Returns its native symbol contract.
    pub const fn symbol(&self) -> &NativeSymbolContract {
        &self.symbol
    }

    /// Returns its target-specific selection behavior.
    pub const fn selection(&self) -> &NativeDefinitionSelection {
        &self.selection
    }
}

/// The known native symbol and retention behavior of an indivisible unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NativeUnitSummary {
    /// All definitions, references, and native roots are represented.
    Exact {
        /// Symbols provided by this unit. Different units may provide the same symbol.
        definitions: Arc<[NativeDefinition]>,
        /// Native references with required or optional presence and strong or weak binding.
        references: Arc<[NativeSymbolContract]>,
        /// Mandatory native lifecycle roots.
        roots: Arc<[NativeRoot]>,
    },
    /// No safe exact summary exists. The entire payload remains a conservative leaf.
    Opaque,
}

/// An unordered set of native units that must be retained together.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeCoRetentionGroup(Arc<[NativeContentDigest]>);

impl NativeCoRetentionGroup {
    /// Creates a group with at least two distinct content identities.
    pub fn try_new(members: impl IntoIterator<Item = NativeContentDigest>) -> Option<Self> {
        let mut members = members.into_iter().collect::<Vec<_>>();

        members.sort_unstable();
        members.dedup();

        (members.len() >= 2).then(|| Self(members.into()))
    }

    /// Returns the group's members in stable content-identity order.
    pub fn members(&self) -> &[NativeContentDigest] {
        &self.0
    }
}

/// One indivisible content-addressed native payload and its link requirements.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeUnit {
    digest: NativeContentDigest,
    kind: NativeUnitKind,
    summary: NativeUnitSummary,
    native_links: Arc<[NativeLinkRequirement]>,
    link_options: Arc<[Arc<str>]>,
}

impl NativeUnit {
    /// Creates one unit. The index validates all cross-unit references.
    pub fn new(
        digest: NativeContentDigest,
        kind: NativeUnitKind,
        summary: NativeUnitSummary,
        native_links: impl IntoIterator<Item = NativeLinkRequirement>,
        link_options: impl IntoIterator<Item = Arc<str>>,
    ) -> Self {
        let summary = match summary {
            NativeUnitSummary::Exact {
                definitions,
                references,
                roots,
            } => NativeUnitSummary::Exact {
                definitions: sorted(definitions),
                references: sorted(references),
                roots: sorted(roots),
            },
            NativeUnitSummary::Opaque => NativeUnitSummary::Opaque,
        };

        let mut native_links = native_links.into_iter().collect::<Vec<_>>();

        native_links.sort_unstable();
        native_links.dedup();

        Self {
            digest,
            kind,
            summary,
            native_links: native_links.into(),
            link_options: link_options.into_iter().collect(),
        }
    }

    /// Returns the payload's content identity.
    pub const fn digest(&self) -> NativeContentDigest {
        self.digest
    }

    /// Returns the physical payload kind.
    pub const fn kind(&self) -> NativeUnitKind {
        self.kind
    }

    /// Returns the exact summary or explicit opaque marker.
    pub const fn summary(&self) -> &NativeUnitSummary {
        &self.summary
    }

    /// Returns native library and framework requirements.
    pub fn native_links(&self) -> &[NativeLinkRequirement] {
        &self.native_links
    }

    /// Returns ordered target-linker options required by this unit.
    pub fn link_options(&self) -> &[Arc<str>] {
        &self.link_options
    }
}

fn sorted<T: Clone + Ord>(items: Arc<[T]>) -> Arc<[T]> {
    // Summary construction is a one-time publication boundary. Sorting a copy leaves shared
    // inputs immutable and makes the index independent of producer traversal order.
    let mut items = items.to_vec();

    items.sort_unstable();

    items.into()
}

#[cfg(test)]
mod tests {
    use super::{NativeContentDigest, NativeUnitKind};
    use bray_target::NativeTarget;

    #[test]
    fn payload_names_follow_target_object_and_archive_formats() {
        let id = NativeContentDigest::new([7; 32]);

        assert_eq!(
            NativeUnitKind::Object.file_name(id, NativeTarget::X86_64WindowsMsvc),
            format!("{id}.obj")
        );

        assert_eq!(
            NativeUnitKind::Object.file_name(id, NativeTarget::X86_64LinuxGnu),
            format!("{id}.o")
        );

        assert_eq!(
            NativeUnitKind::OpaqueArchive.file_name(id, NativeTarget::X86_64WindowsMsvc),
            format!("{id}.lib")
        );

        assert_eq!(
            NativeUnitKind::OpaqueArchive.file_name(id, NativeTarget::X86_64LinuxGnu),
            format!("lib{id}.a")
        );
    }
}
