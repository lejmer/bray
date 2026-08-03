use std::fmt;
use std::sync::Arc;

/// A validated semantic version identifying one package release.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageVersion(Arc<semver::Version>);

impl PackageVersion {
    /// Parses a package version using the Semantic Versioning specification.
    pub fn try_new(value: &str) -> Option<Self> {
        semver::Version::parse(value)
            .ok()
            .map(Arc::new)
            .map(Self)
    }

    /// Returns the major version component.
    pub fn major(&self) -> u64 {
        self.0.major
    }

    /// Returns the minor version component.
    pub fn minor(&self) -> u64 {
        self.0.minor
    }

    /// Returns the patch version component.
    pub fn patch(&self) -> u64 {
        self.0.patch
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::PackageVersion;

    #[test]
    fn package_versions_follow_semantic_versioning() {
        let Some(version) = PackageVersion::try_new("1.2.3-alpha.1+build.7") else {
            panic!("valid semantic version must parse");
        };

        assert_eq!(version.major(), 1);
        assert_eq!(version.minor(), 2);
        assert_eq!(version.patch(), 3);
        assert_eq!(version.to_string(), "1.2.3-alpha.1+build.7");
        assert_eq!(PackageVersion::try_new("1.2"), None);
        assert_eq!(PackageVersion::try_new("1.02.3"), None);
    }

    #[test]
    fn package_versions_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<PackageVersion>();
    }
}
