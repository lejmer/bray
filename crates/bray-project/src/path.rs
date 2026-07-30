use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;

/// A canonical portable path relative to a Bray workspace or package.
///
/// Project paths use `/`, contain no empty, current-directory, or parent-directory
/// components, and never depend on the host's path normalization rules.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectPath(NonEmptySharedStr);

impl ProjectPath {
    /// Returns the portable serialized path.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Converts this portable path to a host path beneath `root`.
    pub fn beneath(&self, root: &Path) -> PathBuf {
        if self.as_str() == "." {
            return root.to_path_buf();
        }

        self.as_str()
            .split('/')
            .fold(root.to_path_buf(), |path, component| path.join(component))
    }

    pub(crate) fn try_new(value: impl Into<Arc<str>>, allow_workspace_root: bool) -> Option<Self> {
        let value = value.into();

        if value.as_ref() == "." {
            return allow_workspace_root
                .then(|| NonEmptySharedStr::try_new(value).map(Self))
                .flatten();
        }

        if value.is_empty()
            || value.starts_with('/')
            || value.ends_with('/')
            || value.contains('\\')
            || value.contains('\0')
            || value.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        {
            return None;
        }

        NonEmptySharedStr::try_new(value).map(Self)
    }

    pub(crate) fn joined(&self, suffix: &Self) -> Self {
        if self.as_str() == "." {
            // Project paths are immutable Arc-backed values; retaining one is constant time.
            return suffix.clone();
        }

        let joined: Arc<str> = format!("{}/{}", self.as_str(), suffix.as_str()).into();

        let Some(path) = Self::try_new(joined, false) else {
            // Two validated relative paths separated by `/` preserve every path invariant.
            panic!("joining validated project paths must remain valid");
        };

        path
    }
}

impl AsRef<str> for ProjectPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::ProjectPath;

    #[test]
    fn project_paths_are_portable_and_canonical() {
        assert_eq!(ProjectPath::try_new(".", false), None);
        assert_eq!(ProjectPath::try_new("../vendor", true), None);
        assert_eq!(ProjectPath::try_new("vendor\\math", true), None);
        assert_eq!(ProjectPath::try_new("vendor//math", true), None);

        let Some(path) = ProjectPath::try_new("vendor/math", false) else {
            panic!("test project path must be valid");
        };

        assert_eq!(path.as_str(), "vendor/math");

        assert_eq!(
            path.beneath(Path::new("workspace")),
            Path::new("workspace").join("vendor").join("math")
        );
    }
}
