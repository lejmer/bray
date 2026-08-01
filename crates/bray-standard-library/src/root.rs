use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Explicit immutable standard library bundle root selected by a compiler host.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardLibraryRoot(Arc<Path>);

impl StandardLibraryRoot {
    /// Creates a root only from an absolute host path.
    pub fn try_new(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();

        if !path.is_absolute() {
            return None;
        }

        Some(Self(Arc::from(path)))
    }

    /// Returns the exact configured bundle root.
    pub fn path(&self) -> &Path {
        &self.0
    }
}
