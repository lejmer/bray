use bray_native_artifact::NativeContentDigest;
use bray_symbols::InterfaceSymbolId;

use crate::{
    InterfaceConstantCallableBody, InterfaceLanguageRevision, InterfaceValidationPolicy,
    ValidatedPackageInterface, encode_package_interface,
};

pub(super) struct ArtifactFixture {
    pub(super) interface: ValidatedPackageInterface,
    pub(super) bundle: crate::PackageInterfaceExportBundle,
    pub(super) body: InterfaceConstantCallableBody,
}

pub(super) fn artifact_fixture() -> ArtifactFixture {
    let bundle = crate::test_support::package_interface_export_bundle();

    let encoded = encode_package_interface(&bundle)
        .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

    let interface = ValidatedPackageInterface::try_new(
        encoded.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"));

    let body = crate::test_support::constant_callable_body(&bundle);

    ArtifactFixture {
        interface,
        bundle,
        body,
    }
}

pub(super) fn generic_callable_owner(
    bundle: &crate::PackageInterfaceExportBundle,
) -> InterfaceSymbolId {
    bundle
        .semantics()
        .generic_declarations()
        .iter()
        .find_map(|declaration| match declaration.owner() {
            crate::InterfaceSymbolReference::Local(owner)
                if bundle
                    .surface()
                    .symbols()
                    .symbol(*owner)
                    .is_some_and(|symbol| symbol.kind().is_callable()) =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("test interface must export a generic callable"))
}

pub(super) fn native_digest(bytes: &[u8]) -> NativeContentDigest {
    NativeContentDigest::new(
        bray_base::sha256_reader(bytes)
            .unwrap_or_else(|error| panic!("test bytes must hash: {error}")),
    )
}
