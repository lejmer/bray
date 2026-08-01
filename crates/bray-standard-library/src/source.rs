use bray_symbols::PackageIdentity;

use crate::is_reserved_standard_library_package;

/// Host authority governing whether source may claim the reserved standard library namespace.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PackageSourceAuthority {
    /// Ordinary user or vendored package source.
    #[default]
    Ordinary,
    /// Toolchain-owned standard library source.
    StandardLibrary,
}

impl PackageSourceAuthority {
    /// Returns whether this authority accepts the package identity.
    pub fn accepts(self, package: &PackageIdentity) -> bool {
        self.is_standard_library() == is_reserved_standard_library_package(package)
    }

    /// Returns whether this authority denotes toolchain-owned standard library source.
    pub const fn is_standard_library(self) -> bool {
        matches!(self, Self::StandardLibrary)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::PackageIdentity;

    use super::PackageSourceAuthority;

    #[test]
    fn source_authority_partitions_ordinary_and_reserved_packages() {
        let ordinary = package("example.application");
        let public = package("std");
        let support = package("std.runtime");

        assert!(PackageSourceAuthority::Ordinary.accepts(&ordinary));
        assert!(!PackageSourceAuthority::Ordinary.accepts(&public));
        assert!(!PackageSourceAuthority::Ordinary.accepts(&support));
        assert!(!PackageSourceAuthority::StandardLibrary.accepts(&ordinary));
        assert!(PackageSourceAuthority::StandardLibrary.accepts(&public));
        assert!(PackageSourceAuthority::StandardLibrary.accepts(&support));
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid: {value}"))
    }
}
