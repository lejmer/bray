use crate::{
    CompilationOptions, CompilationRequest, DependencyInterfaceInput,
    PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
};
use bray_base::NonEmptySharedStr;
use bray_codegen::{CodegenConfiguration, CodegenOptions};
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_package_interface::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    InterfaceExecutableTemplate, InterfaceLanguageRevision, InterfaceNativeBinding,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageImplementationArtifact,
    PackageImplementationSpecializationKey, PackageInterfaceIdentity, ValidatedPackageInterface,
    encode_package_interface,
};
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, NativeSymbolContract, PackageIdentity, ProductKind,
};
use bray_target::NativeTarget;
use std::sync::Arc;

pub(in super::super) fn native_fixture_compilation(
    dependency: DependencyInterfaceInput,
    product_kind: ProductKind,
    codegen: Option<CodegenConfiguration>,
) -> crate::Compilation {
    let source = match product_kind {
        ProductKind::Library => {
            "module application;
using example.dependency.templates.hot;
public func forwarded(pos value: i32) -> i32 {
    return example.dependency.templates.hot(value);
}
"
        }
        ProductKind::Executable => {
            "module application;
using example.dependency.templates.hot;
func main() { let value: i32 = example.dependency.templates.hot(1); }
"
        }
        ProductKind::Test => {
            unreachable!("native fixture only exercises libraries and executables")
        }
    };

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(source, 0)],
        CompilationOptions::new(
            WorkerBudget::serial(),
            product_kind,
            SelectedTarget::baseline(),
        ),
    )
    .with_dependency_interfaces([dependency]);

    let compilation = match codegen {
        Some(codegen) => crate::Compilation::load_with_codegen(request, codegen),
        None => crate::Compilation::load(request),
    }
    .expect("consumer compilation must load");

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    compilation
}

#[derive(Clone, Copy)]
pub(in super::super) struct GenericDependencyFixture {
    pub(in super::super) source: &'static str,
    pub(in super::super) runtime_frames: Option<usize>,
    pub(in super::super) executable_templates: usize,
    pub(in super::super) platform_service: Option<(PlatformServiceRole, &'static str)>,
}

pub(in super::super) fn generic_consumer_for_target_with_source(
    dependency: DependencyInterfaceInput,
    target: SelectedTarget,
    source: &str,
) -> crate::Compilation {
    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(source, 0)],
        CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, target),
    )
    .with_dependency_interfaces([dependency]);

    crate::Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"))
}

pub(in super::super) fn generic_dependency_from_fixture(
    include_implementation: bool,
    malformed_templates: bool,
    fixture: GenericDependencyFixture,
) -> DependencyInterfaceInput {
    dependency_from_fixture_with_native(include_implementation, malformed_templates, fixture, None)
}

pub(in super::super) fn dependency_from_fixture_with_native(
    include_implementation: bool,
    malformed_templates: bool,
    fixture: GenericDependencyFixture,
    native: Option<(CodegenOptions, bool, bool, Option<NativeUnitKind>)>,
) -> DependencyInterfaceInput {
    let package = PackageIdentity::try_new("example.dependency")
        .unwrap_or_else(|| panic!("dependency package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("dependency product identity must be valid"));

    let identity = PackageInterfaceIdentity::try_new(
        package.clone(),
        crate::test_support::package_version(),
        product.clone(),
        InterfaceProductKind::Library,
        "public",
    )
    .unwrap_or_else(|| panic!("dependency interface identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let request = CompilationRequest::with_options(
        package.clone(),
        vec![crate::test_support::source_input(fixture.source, 0)],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_package_interface_export(export)
    .with_platform_services(fixture.platform_service.map(|(role, declaration)| {
        PlatformServiceBinding::try_new(role, declaration)
            .unwrap_or_else(|| panic!("fixture platform binding must validate"))
    }));

    let compilation = crate::Compilation::load(request)
        .unwrap_or_else(|error| panic!("dependency compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = compilation
        .package_interface_export_bundle()
        .and_then(|result| result.as_ref().ok())
        .unwrap_or_else(|| panic!("dependency interface bundle must build"));

    match fixture.runtime_frames {
        Some(expected) => {
            let [runtime] = bundle.semantics().runtime_requirements() else {
                panic!("generic callable family must publish one runtime requirement");
            };

            assert_eq!(runtime.frames().len(), expected);
            assert!(runtime.requirements().capabilities().is_empty());
        }
        None => assert!(bundle.semantics().runtime_requirements().is_empty()),
    }

    let interface = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("dependency interface must encode: {error:?}"));

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated = ValidatedPackageInterface::try_new(interface.bytes(), policy)
        .unwrap_or_else(|error| panic!("dependency interface must validate: {error:?}"));

    let templates = bundle
        .executable_templates()
        .iter()
        .map(|template| {
            if malformed_templates {
                InterfaceExecutableTemplate::new(
                    template.owner(),
                    template.identity(),
                    template.family_size(),
                    [0_u8],
                )
                .unwrap_or_else(|| panic!("malformed test payload must remain nonempty"))
            } else {
                template.clone()
            }
        })
        .collect::<Vec<_>>();

    let implementation = if let Some((producer_options, corrupt, linked, opaque)) = native {
        let [template] = bundle.executable_templates() else {
            panic!("native fixture must publish exactly one executable template");
        };

        let owner = template.owner();

        let owner_symbol = bundle
            .surface()
            .symbols()
            .symbol(owner)
            .expect("native fixture owner must be exported");

        let symbol = NonEmptySharedStr::try_new("bray_test_precompiled")
            .expect("test native symbol must be valid");

        let bytes: Arc<[u8]> = Arc::from(b"test object bytes".as_slice());

        let digest = NativeContentDigest::new(
            bray_base::sha256_reader(bytes.as_ref()).expect("in-memory hash cannot fail"),
        );

        let unit = NativeUnit::new(
            digest,
            NativeUnitKind::Object,
            NativeUnitSummary::Exact {
                definitions: Arc::from([NativeDefinition::new(
                    NativeSymbolContract::required_name(symbol.clone()),
                    NativeDefinitionSelection::Ordinary,
                )]),
                references: if linked {
                    Arc::from([NativeSymbolContract::required_name(
                        NonEmptySharedStr::try_new("bray_test_dependency")
                            .expect("test dependency symbol must validate"),
                    )])
                } else {
                    Arc::from([])
                },
                roots: Arc::from([]),
            },
            [],
        );

        let target = NativeTarget::for_identity(bundle.implementation_configuration().target())
            .expect("test target must have a native artifact kind");

        let mut units = vec![unit];

        let mut payloads = vec![(
            digest.bytes(),
            if corrupt {
                Arc::from(b"corrupt object".as_slice())
            } else {
                bytes
            },
        )];

        if linked {
            let dependency_bytes: Arc<[u8]> = Arc::from(b"test dependency object".as_slice());

            let dependency_digest = NativeContentDigest::new(
                bray_base::sha256_reader(dependency_bytes.as_ref())
                    .expect("in-memory dependency hash cannot fail"),
            );

            units.push(NativeUnit::new(
                dependency_digest,
                NativeUnitKind::Object,
                NativeUnitSummary::Exact {
                    definitions: Arc::from([NativeDefinition::new(
                        NativeSymbolContract::required_name(
                            NonEmptySharedStr::try_new("bray_test_dependency")
                                .expect("test dependency symbol must validate"),
                        ),
                        NativeDefinitionSelection::Ordinary,
                    )]),
                    references: Arc::from([]),
                    roots: Arc::from([]),
                },
                [NativeLinkRequirement::new(
                    NonEmptySharedStr::try_new("native_support")
                        .expect("test native library name must validate"),
                    NativeLinkKind::System,
                )],
            ));

            payloads.push((dependency_digest.bytes(), dependency_bytes));

            let unused_bytes: Arc<[u8]> = Arc::from(b"disconnected object".as_slice());

            let unused_digest = NativeContentDigest::new(
                bray_base::sha256_reader(unused_bytes.as_ref())
                    .expect("in-memory unused hash cannot fail"),
            );

            units.push(NativeUnit::new(
                unused_digest,
                NativeUnitKind::Object,
                NativeUnitSummary::Exact {
                    definitions: Arc::from([NativeDefinition::new(
                        NativeSymbolContract::required_name(
                            NonEmptySharedStr::try_new("bray_test_disconnected")
                                .expect("test unused symbol must validate"),
                        ),
                        NativeDefinitionSelection::Ordinary,
                    )]),
                    references: Arc::from([]),
                    roots: Arc::from([]),
                },
                [],
            ));

            payloads.push((unused_digest.bytes(), unused_bytes));
        }

        if let Some(kind) = opaque {
            let opaque_bytes: Arc<[u8]> = Arc::from(b"opaque package bitcode".as_slice());

            let opaque_digest = NativeContentDigest::new(
                bray_base::sha256_reader(opaque_bytes.as_ref())
                    .expect("in-memory opaque hash cannot fail"),
            );

            units.push(NativeUnit::new(
                opaque_digest,
                kind,
                NativeUnitSummary::opaque([]),
                [],
            ));

            payloads.push((opaque_digest.bytes(), opaque_bytes));
        }

        let index =
            NativeArtifactIndex::try_new(target, NativeContentDigest::new([1; 32]), units, [])
                .expect("test native index must validate");

        let index_bytes = index.encode().expect("test native index must encode");

        let binding = InterfaceNativeBinding::new(
            owner,
            PackageImplementationSpecializationKey::new(
                ImplementationExternalSymbolIdentity::new(owner_symbol.key()),
                [],
                [],
                bundle.implementation_configuration().clone(),
                CURRENT_TEMPLATE_SCHEMA_REVISION,
                bundle.surface().dependencies().iter().cloned(),
            ),
            producer_options,
            digest.bytes(),
            symbol,
        );

        PackageImplementationArtifact::try_from_export_bundle_with_native(
            &interface,
            &(**bundle)
                .clone()
                .with_executable_templates(templates)
                .expect("fixture executable templates must form a complete family"),
            &index_bytes,
            &payloads,
            &[binding],
            InterfaceValidationLimits::default(),
        )
        .expect("native dependency implementation must encode")
    } else {
        PackageImplementationArtifact::try_new(
            &validated,
            bundle.surface(),
            bundle.semantics(),
            bundle.implementation_configuration().clone(),
            [],
            templates,
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .expect("dependency implementation must encode")
    };

    assert_eq!(
        bundle.executable_templates().len(),
        fixture.executable_templates
    );

    let dependency = DependencyInterfaceInput::new(
        package,
        product,
        "dependency.brayi",
        interface.shared_bytes(),
        policy,
    );

    if include_implementation {
        dependency.with_implementation_artifact("dependency.brayimpl", Arc::new(implementation))
    } else {
        dependency
    }
}
