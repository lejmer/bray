pub(crate) use bray_testing::{
    TemporaryFile, unique_temporary_directory,
};

pub(crate) fn package_identity() -> bray_symbols::PackageIdentity {
    match bray_symbols::PackageIdentity::try_new("test.package") {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}
