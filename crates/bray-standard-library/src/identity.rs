use bray_symbols::PackageIdentity;

/// Canonical public standard library package identity.
pub const PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY: &str = "std";

/// Canonical public standard library product identity.
pub const PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY: &str = "library";

/// Canonical public standard library surface identity.
pub const PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY: &str = "public";

/// Returns whether a package is the public standard library package.
pub fn is_public_standard_library_package(package: &PackageIdentity) -> bool {
    package.as_str() == PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
}

/// Returns whether a package belongs to the toolchain-reserved standard library namespace.
pub fn is_reserved_standard_library_package(package: &PackageIdentity) -> bool {
    is_public_standard_library_package(package)
        || package
            .as_str()
            .strip_prefix(PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use bray_symbols::PackageIdentity;

    use super::{is_public_standard_library_package, is_reserved_standard_library_package};

    #[test]
    fn standard_library_namespace_has_one_public_root_and_reserved_support_packages() {
        let public = package("std");
        let support = package("std.runtime");
        let lookalike = package("stdish.runtime");

        assert!(is_public_standard_library_package(&public));
        assert!(is_reserved_standard_library_package(&public));
        assert!(!is_public_standard_library_package(&support));
        assert!(is_reserved_standard_library_package(&support));
        assert!(!is_reserved_standard_library_package(&lookalike));
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid: {value}"))
    }
}
