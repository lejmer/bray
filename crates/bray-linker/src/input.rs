use std::path::PathBuf;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_runtime_interface::{RuntimeArtifactId, RuntimeNativeUnit};
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
    /// Native library selected by canonical library name.
    NativeLibrary,
    /// Platform framework selected by canonical framework name.
    Framework,
}

impl From<bray_native_artifact::NativeUnitKind> for LinkInputKind {
    fn from(kind: bray_native_artifact::NativeUnitKind) -> Self {
        match kind {
            bray_native_artifact::NativeUnitKind::Object => Self::RelocatableObject,
            bray_native_artifact::NativeUnitKind::Bitcode => Self::Bitcode,
            bray_native_artifact::NativeUnitKind::OpaqueArchive => Self::Archive,
        }
    }
}

impl LinkInputKind {
    const fn accepts(self, source: &LinkInputSource) -> bool {
        match self {
            Self::RelocatableObject
            | Self::Bitcode
            | Self::Archive
            | Self::StartupObject
            | Self::TerminationObject => matches!(source, LinkInputSource::File(_)),
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
    /// Native platform provider supplied by a selected package dependency.
    PlatformProvider(PackageIdentity),
    /// Input selected by the canonical target profile.
    TargetProfile,
    /// Component of the selected separately linked Bray runtime artifact.
    Runtime(RuntimeArtifactId),
    /// Native dependency required by the selected Bray runtime artifact.
    RuntimeDependency(RuntimeArtifactId),
    /// Native dependency supplied by explicit compiler-host configuration.
    HostConfiguration,
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
    spec: LinkInputSpec,
}

/// Validated native linker input before one plan assigns its stable identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkInputSpec {
    kind: LinkInputKind,
    source: LinkInputSource,
    provenance: LinkInputProvenance,
    mode: LinkInputMode,
}

impl LinkInputSpec {
    /// Validates one native linker input specification.
    pub fn try_new(
        kind: LinkInputKind,
        source: LinkInputSource,
        provenance: LinkInputProvenance,
        mode: LinkInputMode,
    ) -> Result<Self, LinkInputBuildError> {
        validate_input(kind, &source, mode)?;

        Ok(Self {
            kind,
            source,
            provenance,
            mode,
        })
    }

    /// Creates an ordinary native-library input unless its canonical name is empty.
    pub fn try_native_library(
        name: impl Into<Arc<str>>,
        provenance: LinkInputProvenance,
    ) -> Option<Self> {
        Some(Self {
            kind: LinkInputKind::NativeLibrary,
            source: LinkInputSource::try_native_library(name)?,
            provenance,
            mode: LinkInputMode::Ordinary,
        })
    }

    /// Creates an ordinary platform-framework input unless its canonical name is empty.
    pub fn try_framework(
        name: impl Into<Arc<str>>,
        provenance: LinkInputProvenance,
    ) -> Option<Self> {
        Some(Self {
            kind: LinkInputKind::Framework,
            source: LinkInputSource::try_framework(name)?,
            provenance,
            mode: LinkInputMode::Ordinary,
        })
    }

    /// Creates one authenticated runtime input without changing its physical category.
    pub fn runtime_unit(runtime: &RuntimeArtifactId, unit: &RuntimeNativeUnit) -> Self {
        Self {
            kind: unit.kind().into(),
            source: LinkInputSource::file(unit.path()),
            provenance: LinkInputProvenance::Runtime(runtime.clone()),
            mode: LinkInputMode::Ordinary,
        }
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

    /// Assigns this specification its stable identity in one link plan.
    pub fn with_id(self, id: LinkInputId) -> LinkInput {
        LinkInput { id, spec: self }
    }
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
        LinkInputSpec::try_new(kind, source, provenance, mode).map(|spec| spec.with_id(id))
    }

    /// Creates an ordinary native-library input unless its canonical name is empty.
    pub fn try_native_library(
        id: LinkInputId,
        name: impl Into<Arc<str>>,
        provenance: LinkInputProvenance,
    ) -> Option<Self> {
        LinkInputSpec::try_native_library(name, provenance).map(|spec| spec.with_id(id))
    }

    /// Returns the stable input identity.
    pub const fn id(&self) -> LinkInputId {
        self.id
    }

    /// Returns the native input category.
    pub const fn kind(&self) -> LinkInputKind {
        self.spec.kind()
    }

    /// Returns the exact driver-facing input source.
    pub const fn source(&self) -> &LinkInputSource {
        self.spec.source()
    }

    /// Returns the input origin retained for diagnostics and metadata.
    pub const fn provenance(&self) -> &LinkInputProvenance {
        self.spec.provenance()
    }

    /// Returns the selected archive treatment.
    pub const fn mode(&self) -> LinkInputMode {
        self.spec.mode()
    }
}

fn validate_input(
    kind: LinkInputKind,
    source: &LinkInputSource,
    mode: LinkInputMode,
) -> Result<(), LinkInputBuildError> {
    if matches!(source, LinkInputSource::File(path) if path.as_os_str().is_empty()) {
        return Err(LinkInputBuildError::EmptyFilePath);
    }

    if !kind.accepts(source) {
        return Err(LinkInputBuildError::SourceKindMismatch);
    }

    if mode == LinkInputMode::WholeArchive && kind != LinkInputKind::Archive {
        return Err(LinkInputBuildError::WholeArchiveRequiresArchive);
    }

    Ok(())
}

/// A contract violation that prevents link-input construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
        LinkInputProvenance, LinkInputSource, LinkInputSpec,
    };

    #[test]
    fn named_inputs_select_their_source_kinds_and_reject_empty_names() {
        let library = LinkInput::try_native_library(
            LinkInputId::new(4),
            "pthread",
            LinkInputProvenance::HostConfiguration,
        )
        .unwrap_or_else(|| panic!("test native library name must be valid"));

        let framework =
            LinkInputSpec::try_framework("Foundation", LinkInputProvenance::TargetProfile)
                .unwrap_or_else(|| panic!("test framework name must be valid"));

        assert_eq!(library.kind(), LinkInputKind::NativeLibrary);
        assert_eq!(framework.kind(), LinkInputKind::Framework);

        assert!(
            LinkInput::try_native_library(
                LinkInputId::new(5),
                "",
                LinkInputProvenance::HostConfiguration,
            )
            .is_none()
        );

        assert!(LinkInputSpec::try_framework("", LinkInputProvenance::TargetProfile).is_none());
    }

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
