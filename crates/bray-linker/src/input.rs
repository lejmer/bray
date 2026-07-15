use std::path::PathBuf;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::PackageIdentity;

/// Stable identity of one source-ordered link input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkInputId(u32);

impl LinkInputId {
    /// Creates an identity from its deterministic position in the link plan.
    pub const fn new(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// Returns the deterministic input ordinal.
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// Native linker input category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkInputKind {
    /// Relocatable native object file.
    RelocatableObject,
    /// Backend bitcode accepted by a compatible linker driver.
    Bitcode,
    /// Existing native archive.
    Archive,
    /// Target startup object placed before ordinary product inputs.
    StartupObject,
    /// Target termination object placed after ordinary product inputs.
    TerminationObject,
    /// File-backed Bray runtime component.
    RuntimeObject,
    /// Native library selected by canonical library name.
    NativeLibrary,
    /// Platform framework selected by canonical framework name.
    Framework,
}

impl LinkInputKind {
    const fn accepts(self, source: &LinkInputSource) -> bool {
        match self {
            Self::RelocatableObject
            | Self::Bitcode
            | Self::Archive
            | Self::StartupObject
            | Self::TerminationObject
            | Self::RuntimeObject => matches!(source, LinkInputSource::File(_)),
            Self::NativeLibrary => matches!(source, LinkInputSource::NativeLibrary(_)),
            Self::Framework => matches!(source, LinkInputSource::Framework(_)),
        }
    }
}

/// Exact input supplied to the selected linker driver.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkInputSource {
    /// Immutable staged or configured filesystem input.
    File(PathBuf),
    /// Canonical native-library name resolved by the driver.
    NativeLibrary(NonEmptySharedStr),
    /// Canonical platform-framework name resolved by the driver.
    Framework(NonEmptySharedStr),
}

impl LinkInputSource {
    /// Creates a file-backed input source.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    /// Creates a native-library source unless its canonical name is empty.
    pub fn try_native_library(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self::NativeLibrary)
    }

    /// Creates a framework source unless its canonical name is empty.
    pub fn try_framework(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self::Framework)
    }
}

/// Origin retained for diagnostics and reproducibility metadata.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkInputProvenance {
    /// Input emitted for the product represented by the enclosing plan.
    Product,
    /// Input supplied by a selected package dependency.
    Package(PackageIdentity),
    /// Input selected by the canonical target profile.
    TargetProfile,
    /// Named Bray runtime component.
    Runtime(NonEmptySharedStr),
    /// Native dependency supplied by explicit compiler-host configuration.
    HostConfiguration,
}

impl LinkInputProvenance {
    /// Creates runtime provenance unless the canonical component name is empty.
    pub fn try_runtime(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self::Runtime)
    }
}

/// Per-input archive treatment selected before driver translation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkInputMode {
    /// Ordinary platform linker treatment.
    Ordinary,
    /// Retain every member of an archive input.
    WholeArchive,
}

/// One validated source-ordered native linker input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkInput {
    id: LinkInputId,
    kind: LinkInputKind,
    source: LinkInputSource,
    provenance: LinkInputProvenance,
    mode: LinkInputMode,
}

impl LinkInput {
    /// Validates and creates one native linker input.
    pub fn try_new(
        id: LinkInputId,
        kind: LinkInputKind,
        source: LinkInputSource,
        provenance: LinkInputProvenance,
        mode: LinkInputMode,
    ) -> Result<Self, LinkInputBuildError> {
        if matches!(&source, LinkInputSource::File(path) if path.as_os_str().is_empty()) {
            return Err(LinkInputBuildError::EmptyFilePath);
        }

        if !kind.accepts(&source) {
            return Err(LinkInputBuildError::SourceKindMismatch);
        }

        if mode == LinkInputMode::WholeArchive && kind != LinkInputKind::Archive {
            return Err(LinkInputBuildError::WholeArchiveRequiresArchive);
        }

        Ok(Self {
            id,
            kind,
            source,
            provenance,
            mode,
        })
    }

    /// Returns the stable input identity.
    pub const fn id(&self) -> LinkInputId {
        self.id
    }

    /// Returns the native input category.
    pub const fn kind(&self) -> LinkInputKind {
        self.kind
    }

    /// Returns the exact driver-facing input source.
    pub const fn source(&self) -> &LinkInputSource {
        &self.source
    }

    /// Returns the input origin retained for diagnostics and metadata.
    pub const fn provenance(&self) -> &LinkInputProvenance {
        &self.provenance
    }

    /// Returns the selected archive treatment.
    pub const fn mode(&self) -> LinkInputMode {
        self.mode
    }
}

/// A contract violation that prevents link-input construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkInputBuildError {
    /// A file-backed input has an empty path.
    EmptyFilePath,
    /// The source representation does not match the selected input category.
    SourceKindMismatch,
    /// Whole-archive treatment was requested for a non-archive input.
    WholeArchiveRequiresArchive,
}

#[cfg(test)]
mod tests {
    use super::{
        LinkInput, LinkInputBuildError, LinkInputId, LinkInputKind, LinkInputMode,
        LinkInputProvenance, LinkInputSource,
    };

    #[test]
    fn inputs_reject_mismatched_sources_and_archive_modes() {
        let library = LinkInputSource::try_native_library("pthread");

        assert_eq!(
            library.map(|source| LinkInput::try_new(
                LinkInputId::new(0),
                LinkInputKind::RelocatableObject,
                source,
                LinkInputProvenance::HostConfiguration,
                LinkInputMode::Ordinary,
            )),
            Some(Err(LinkInputBuildError::SourceKindMismatch))
        );

        assert_eq!(
            LinkInput::try_new(
                LinkInputId::new(0),
                LinkInputKind::RelocatableObject,
                LinkInputSource::file("main.o"),
                LinkInputProvenance::Product,
                LinkInputMode::WholeArchive,
            ),
            Err(LinkInputBuildError::WholeArchiveRequiresArchive)
        );
    }

    #[test]
    fn file_inputs_require_non_empty_paths() {
        assert_eq!(
            LinkInput::try_new(
                LinkInputId::new(0),
                LinkInputKind::RelocatableObject,
                LinkInputSource::file(""),
                LinkInputProvenance::Product,
                LinkInputMode::Ordinary,
            ),
            Err(LinkInputBuildError::EmptyFilePath)
        );
    }
}
