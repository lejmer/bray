use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;

use crate::ArtifactId;

/// Host-supplied identity resolved to an in-memory collector or writable stream at publication.
///
/// The identity deliberately carries no open handle or mutable collector state, which keeps
/// requests and plans immutable and safe to share between workers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputSinkId(NonEmptySharedStr);

impl OutputSinkId {
    /// Creates a sink identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(value).map(Self)
    }

    /// Returns the canonical host-supplied sink identity.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Exact immutable publication destination of one planned external artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutputSink {
    /// Final filesystem artifact path.
    Filesystem(PathBuf),
    /// Host-owned in-memory collector and deterministic artifact key.
    Memory {
        /// Collector resolved by the host during publication.
        collector: OutputSinkId,
        /// Artifact key used within the collector.
        artifact: ArtifactId,
    },
    /// Host-owned writable stream resolved by identity during publication.
    Stream(OutputSinkId),
}

/// Borrowed indirect destination resolved to a writable host sink at publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndirectOutputSink<'sink> {
    /// Host-owned in-memory collector and deterministic artifact key.
    Memory {
        /// Collector selected by the emission request.
        collector: &'sink OutputSinkId,
        /// Artifact key within the collector.
        artifact: &'sink ArtifactId,
    },
    /// Host-owned writable stream.
    Stream(&'sink OutputSinkId),
}

/// Host boundary that resolves immutable sink identities to owned writable handles.
pub trait OutputSinkResolver: Send + Sync {
    /// Opens the selected indirect sink using the plan's replacement policy.
    fn open(
        &self,
        sink: IndirectOutputSink<'_>,
        replacement: ReplacementPolicy,
    ) -> io::Result<Box<dyn Write + Send>>;
}

impl OutputSink {
    pub(crate) fn collision_key(&self) -> OutputSinkCollisionKey {
        match self {
            Self::Filesystem(path) => {
                OutputSinkCollisionKey::Filesystem(FilesystemCollisionKey::new(path))
            }
            Self::Memory {
                collector,
                artifact,
            } => {
                // Collision validation owns keys independently of the planned sink collection.
                OutputSinkCollisionKey::Memory {
                    collector: collector.clone(),
                    artifact: artifact.clone(),
                }
            }
            Self::Stream(stream) => {
                // Collision validation owns keys independently of the planned sink collection.
                OutputSinkCollisionKey::Stream(stream.clone())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum OutputSinkCollisionKey {
    Filesystem(FilesystemCollisionKey),
    Memory {
        collector: OutputSinkId,
        artifact: ArtifactId,
    },
    Stream(OutputSinkId),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct FilesystemCollisionKey(Vec<FilesystemCollisionComponent>);

impl FilesystemCollisionKey {
    fn new(path: &Path) -> Self {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                Component::Prefix(prefix) => components.push(FilesystemCollisionComponent::Prefix(
                    normalize_component(prefix.as_os_str()),
                )),
                Component::RootDir => components.push(FilesystemCollisionComponent::Root),
                Component::CurDir => {}
                Component::ParentDir => match components.last() {
                    Some(FilesystemCollisionComponent::Normal(_)) => {
                        components.pop();
                    }
                    Some(FilesystemCollisionComponent::Root) => {}
                    _ => components.push(FilesystemCollisionComponent::Parent),
                },
                Component::Normal(component) => components.push(
                    FilesystemCollisionComponent::Normal(normalize_component(component)),
                ),
            }
        }

        Self(components)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FilesystemCollisionComponent {
    Prefix(OsString),
    Root,
    Parent,
    Normal(OsString),
}

#[cfg(windows)]
fn normalize_component(component: &OsStr) -> OsString {
    component
        .to_string_lossy()
        .trim_end_matches(['.', ' '])
        .to_lowercase()
        .into()
}

#[cfg(not(windows))]
fn normalize_component(component: &OsStr) -> OsString {
    component.to_owned()
}

#[cfg(windows)]
pub(crate) fn is_valid_host_file_name(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };

    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|character| character <= '\u{1f}' || "<>:\"/\\|?*".contains(character))
    {
        return false;
    }

    let base = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();

    !matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_numbered_reserved_name(&base, "COM")
        && !is_numbered_reserved_name(&base, "LPT")
}

#[cfg(windows)]
fn is_numbered_reserved_name(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|number| matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}

#[cfg(unix)]
pub(crate) fn is_valid_host_file_name(name: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    !name.is_empty() && !name.as_bytes().contains(&0)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn is_valid_host_file_name(name: &OsStr) -> bool {
    !name.is_empty() && !name.to_string_lossy().contains('\0')
}

/// Policy for a planned destination that already contains an artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReplacementPolicy {
    /// Publication fails when the destination already exists or contains an artifact.
    RequireAbsent,
    /// Publication may replace an existing artifact.
    ReplaceExisting,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{OutputSink, OutputSinkId, is_valid_host_file_name};

    #[test]
    fn indirect_sinks_are_immutable_identities_without_open_handles() {
        assert_eq!(OutputSinkId::try_new(""), None);

        let Some(id) = OutputSinkId::try_new("host.output") else {
            panic!("test sink identity must be valid");
        };

        assert_eq!(id.as_str(), "host.output");
        assert_send_sync::<OutputSink>();
    }

    #[cfg(windows)]
    #[test]
    fn host_file_names_reject_windows_reserved_forms() {
        for name in ["CON", "nul.log", "COM1.exe", "file:", "file.", "file "] {
            assert!(!is_valid_host_file_name(OsStr::new(name)), "{name}");
        }

        assert!(is_valid_host_file_name(OsStr::new("console.exe")));
    }

    #[cfg(windows)]
    #[test]
    fn filesystem_collision_keys_follow_windows_path_equivalence() {
        let first = OutputSink::Filesystem("OUT./temporary/../application".into());
        let second = OutputSink::Filesystem("out/application".into());

        assert_eq!(first.collision_key(), second.collision_key());
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
