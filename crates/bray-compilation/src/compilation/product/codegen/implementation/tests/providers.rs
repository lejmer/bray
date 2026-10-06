use crate::{
    CompilationOptions, CompilationRequest, PackageInterfaceExportRequest, SelectedTarget,
    WorkerBudget,
};
use bray_base::NonEmptySharedStr;
use bray_linker::{LinkInputSource, LinkModel};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    InterfaceValidationLimits, PackageImplementationArtifact, PackageInterfaceIdentity,
    encode_package_interface,
};
use bray_runtime_interface::PlatformServiceRole;
use bray_standard_library::{
    StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
    StandardLibraryRoot, encode_standard_library_manifest, target_artifacts_for_test,
};
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, NativeSymbolContract, PackageIdentity, ProductKind,
};
use bray_target::NativeTarget;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::Arc;

#[test]
fn native_products_allow_platform_runtime_dependencies() {
    for target in NativeTarget::ALL {
        assert_eq!(
            crate::compilation::product::codegen::link::product_link_model(target.object_format()),
            LinkModel::Dynamic,
            "{target:?}"
        );
    }
}

#[test]
fn source_authority_selects_native_provider_units_and_flags() {
    let directory = tempfile::tempdir().unwrap();
    let selected = SelectedTarget::baseline();
    let target = selected.profile().identity().clone();
    let native_target = NativeTarget::for_identity(&target).unwrap();
    let runtime_abi = selected.runtime_abi();

    let prefix = format!(
        "targets/{}/{}.{}",
        target.as_str(),
        runtime_abi.major(),
        runtime_abi.minor()
    );

    let artifacts = [(
        StandardLibraryArtifactKind::PackageInterface,
        "std.brayi",
        b"interface".as_slice(),
    )]
    .into_iter()
    .map(|(kind, name, bytes)| {
        let artifact =
            StandardLibraryArtifact::try_for_bytes(kind, format!("{prefix}/{name}"), bytes)
                .unwrap();

        let path = artifact.beneath(directory.path());

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();

        artifact
    })
    .collect::<Vec<_>>();

    let mut payloads = BTreeMap::new();
    let mut unit_digests = BTreeMap::new();

    let units = [
        ("fallback", None, None),
        (
            "streams",
            Some(PlatformServiceRole::StandardOutputWrite),
            Some("c"),
        ),
        (
            "filesystem",
            Some(PlatformServiceRole::FileRead),
            Some("filesystem"),
        ),
        (
            "process",
            Some(PlatformServiceRole::ChildSpawn),
            Some("process"),
        ),
    ]
    .into_iter()
    .map(|(name, role, library)| {
        let bytes = name.as_bytes();
        let digest = NativeContentDigest::new(bray_base::sha256_reader(bytes).unwrap());

        let kind = if role.is_some() {
            NativeUnitKind::Object
        } else {
            NativeUnitKind::OpaqueArchive
        };

        let summary = role.map_or(NativeUnitSummary::opaque([]), |role| {
            NativeUnitSummary::Exact {
                definitions: Arc::from([NativeDefinition::new(
                    NativeSymbolContract::required_name(
                        NonEmptySharedStr::try_new(role.native_symbol()).unwrap(),
                    ),
                    NativeDefinitionSelection::Ordinary,
                )]),
                references: Arc::from([]),
                roots: Arc::from([]),
            }
        });

        let links = library.into_iter().map(|name| {
            NativeLinkRequirement::new(
                NonEmptySharedStr::try_new(name).unwrap(),
                NativeLinkKind::System,
            )
        });

        payloads.insert(digest, bytes.to_vec());

        unit_digests.insert(name, digest);

        NativeUnit::new(digest, kind, summary, links)
    })
    .collect::<Vec<_>>();

    let index =
        NativeArtifactIndex::try_new(native_target, NativeContentDigest::new([7; 32]), units, [])
            .unwrap();

    let native_index = index.encode().unwrap();
    let bitcode_bytes = b"optional bitcode";

    let bitcode_digest =
        NativeContentDigest::new(bray_base::sha256_reader(bitcode_bytes.as_slice()).unwrap());

    let bitcode_index = NativeArtifactIndex::try_new(
        native_target,
        NativeContentDigest::new([8; 32]),
        [NativeUnit::new(
            bitcode_digest,
            NativeUnitKind::Bitcode,
            NativeUnitSummary::opaque([]),
            [],
        )],
        [],
    )
    .unwrap()
    .encode()
    .unwrap();

    let package = PackageIdentity::try_new("example.nativefixture").unwrap();

    let identity = PackageInterfaceIdentity::try_new(
        package.clone(),
        crate::test_support::package_version(),
        InterfaceProductIdentity::try_new("library").unwrap(),
        InterfaceProductKind::Library,
        "public",
    )
    .unwrap();

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let producer = crate::Compilation::load(
        CompilationRequest::with_options(
            package,
            vec![crate::test_support::source_input(
                "module fixture;\n\npublic func fixture()\n{\n}\n",
                0,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                selected.clone(),
            ),
        )
        .with_package_interface_export(export),
    )
    .unwrap();

    assert!(
        producer.check_diagnostics().is_empty(),
        "{:#?}",
        producer.check_diagnostics()
    );

    let bundle = producer
        .package_interface_export_bundle()
        .unwrap()
        .as_ref()
        .unwrap();

    let interface = encode_package_interface(bundle).unwrap();

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let units = payloads
        .into_iter()
        .map(|(digest, bytes)| (digest.bytes(), Arc::from(bytes)))
        .collect::<Vec<_>>();

    let implementation = implementation
        .try_with_native_variants(&[(NativeUnitKind::Object, &native_index)], &units)
        .unwrap();

    let native_implementation = implementation
        .try_native_only_artifact(
            &[(NativeUnitKind::Bitcode, &bitcode_index)],
            &[(bitcode_digest.bytes(), Arc::from(bitcode_bytes.as_slice()))],
        )
        .unwrap();

    let implementation_artifact = StandardLibraryArtifact::try_for_bytes(
        StandardLibraryArtifactKind::PackageImplementation,
        format!("{prefix}/std.brayimpl"),
        &implementation.shared_bytes().unwrap(),
    )
    .unwrap();

    fs::write(
        implementation_artifact.beneath(directory.path()),
        &implementation.shared_bytes().unwrap(),
    )
    .unwrap();

    let native_artifact = StandardLibraryArtifact::try_for_bytes(
        StandardLibraryArtifactKind::NativeImplementation,
        format!("{prefix}/std-native.brayimpl"),
        &native_implementation.shared_bytes().unwrap(),
    )
    .unwrap();

    fs::write(
        native_artifact.beneath(directory.path()),
        &native_implementation.shared_bytes().unwrap(),
    )
    .unwrap();

    let manifest = StandardLibraryBundleManifest::try_new([target_artifacts_for_test(
        target.clone(),
        runtime_abi,
        artifacts
            .into_iter()
            .chain([implementation_artifact.clone(), native_artifact.clone()])
            .collect(),
    )
    .unwrap()])
    .unwrap();

    fs::write(
        directory.path().join("manifest.json"),
        encode_standard_library_manifest(&manifest).unwrap(),
    )
    .unwrap();

    let root = StandardLibraryRoot::try_new(directory.path()).unwrap();

    let native_input = bray_package_interface::PackageArtifactInput::file(
        native_artifact.beneath(root.path()),
        Some(native_artifact.digest().bytes()),
    );

    let published = native_input
        .load_implementation()
        .unwrap()
        .native_variant(NativeUnitKind::Bitcode)
        .unwrap()
        .unwrap();

    assert_eq!(published.producer(), NativeContentDigest::new([8; 32]));

    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std.tests.api").unwrap(),
        vec![crate::test_support::source_input(
            "module application;\n",
            0,
        )],
        CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, selected),
    )
    .with_native_implementations([
        bray_package_interface::PackageArtifactInput::file(
            implementation_artifact.beneath(root.path()),
            None,
        ),
        bray_package_interface::PackageArtifactInput::file(
            native_artifact.beneath(root.path()),
            None,
        ),
    ])
    .with_standard_library_source_authority();

    let compilation = crate::Compilation::load(request).unwrap();

    assert!(compilation.dependency_interfaces().is_empty());

    let select = |role: PlatformServiceRole| {
        compilation
            .native_library_link_inputs(
                ProductKind::Executable,
                &BTreeSet::from([role.native_symbol()]),
                &BTreeSet::new(),
            )
            .unwrap()
    };

    let (streams_links, streams) = select(PlatformServiceRole::StandardOutputWrite);

    assert_eq!(streams.len(), 2);

    assert!(
        streams
            .iter()
            .any(|unit| unit.digest == unit_digests["fallback"]
                && unit.bytes.as_ref() == b"fallback")
    );

    assert!(streams.iter().any(
            |unit| unit.digest == unit_digests["streams"] && unit.bytes.as_ref() == b"streams"
        ));

    assert!(
        streams_links
            .iter()
            .any(|input| input.source() == &LinkInputSource::try_native_library("c").unwrap())
    );

    assert!(
        streams
            .iter()
            .all(|unit| unit.digest != unit_digests["process"])
    );

    let (_, filesystem) = select(PlatformServiceRole::FileRead);

    assert!(
        filesystem
            .iter()
            .any(|unit| unit.digest == unit_digests["filesystem"])
    );

    assert!(
        filesystem
            .iter()
            .all(|unit| unit.digest != unit_digests["streams"])
    );

    let overridden = compilation
        .native_library_link_inputs(
            ProductKind::Test,
            &BTreeSet::from([PlatformServiceRole::StandardOutputWrite.native_symbol()]),
            &BTreeSet::from([PlatformServiceRole::StandardOutputWrite]),
        )
        .unwrap();

    assert!(overridden.0.is_empty());
    assert_eq!(overridden.1.len(), 1);
    assert_eq!(overridden.1[0].digest, unit_digests["fallback"]);

    assert!(
        compilation
            .native_library_link_inputs(ProductKind::Library, &BTreeSet::new(), &BTreeSet::new(),)
            .unwrap()
            .1
            .is_empty()
    );
}
