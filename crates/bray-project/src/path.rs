use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::{NonEmptySharedStr, is_canonical_relative_path};

/// A canonical portable path relative to a Bray workspace or package.
///
/// Project paths use `/`, contain no empty, current-directory, or parent-directory
/// components, and never depend on the host's path normalization rules.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectPath(NonEmptySharedStr);

impl ProjectPath {
    /// Creates a non-root canonical portable project path.
    pub fn try_relative(value: impl Into<Arc<str>>) -> Option<Self> {
        Self::try_new(value, false)
    }

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

        if !is_canonical_relative_path(&value) {
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

        assert_eq!(
            ProjectPath::try_relative("vendor/math").map(|path| path.as_str().to_owned()),
            Some(String::from("vendor/math"))
        );

        assert_eq!(path.as_str(), "vendor/math");

        assert_eq!(
            path.beneath(Path::new("workspace")),
            Path::new("workspace").join("vendor").join("math")
        );
    }
}
