use super::support::compilation::test_product_identity;
use super::support::compilation::{codegen_compilation, source_request_with_runtime};
use super::support::dependencies::{
    GenericDependencyFixture, dependency_from_fixture_with_native, native_fixture_compilation,
};
use super::support::linker::test_linker;
use super::support::runtime::runtime_artifact;
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::{
    CancellationToken, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_base::NonEmptySharedStr;
use bray_codegen::CodegenOptions;
use bray_native_artifact::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
    NativeUnit, NativeUnitKind, NativeUnitSummary,
};
use bray_runtime_interface::{
    RuntimeAbiRole, RuntimeArtifact, RuntimeArtifactMetadata, RuntimeArtifactPurpose,
};
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, NativeSymbolContract, ProductKind, StaticStorageDuration,
};
use std::fs;
use std::sync::Arc;

#[test]
#[ignore = "tier:native"]
fn static_cleanup_retains_runtime_providers_without_ordering_co_emitted_trivial_statics() {
    let (_, compilation) = codegen_compilation(source_request_with_runtime(
        &[r#"
            module application;

            struct Resource {
                state: i32;
                destruct() { panic("cleanup incident"); }
            }

            static VALUE: Resource = { state = 42 };
            func main() -> i32 { return VALUE.state; }
        "#],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    let archive = bray_testing::TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let original = runtime_artifact(&compilation, archive.path());

    let provider = bray_native_artifact::NativeStatic::new(
        NonEmptySharedStr::try_new("first_provider_host").expect("provider name must be valid"),
        [77; 32],
        Arc::from([0].as_slice()),
        StaticStorageDuration::Product,
        [],
        None,
        false,
    );

    let companion = bray_native_artifact::NativeStatic::new(
        NonEmptySharedStr::try_new("second_provider_host")
            .expect("fixture non empty shared str must be valid"),
        [78; 32],
        Arc::from([1].as_slice()),
        StaticStorageDuration::Product,
        [],
        None,
        false,
    );

    let indexes = original
        .native_indexes()
        .iter()
        .zip(RuntimeArtifactPurpose::ALL)
        .map(|(artifact, purpose)| {
            let original_index = artifact.index();

            let units = original_index.units().iter().map(|unit| {
                let NativeUnitSummary::Exact {
                    definitions,
                    references,
                    roots,
                } = unit.summary()
                else {
                    return unit.clone();
                };

                let definitions = definitions
                    .iter()
                    .cloned()
                    .chain([&provider, &companion].map(|provider| {
                        NativeDefinition::new(
                            NativeSymbolContract::required_name(
                                NonEmptySharedStr::try_new(provider.symbol())
                                    .expect("host symbol must be valid"),
                            ),
                            NativeDefinitionSelection::Ordinary,
                        )
                    }))
                    .collect::<Vec<_>>();

                NativeUnit::new(
                    unit.digest(),
                    unit.kind(),
                    NativeUnitSummary::Exact {
                        definitions: definitions.into(),
                        references: Arc::clone(references),
                        roots: Arc::clone(roots),
                    },
                    unit.native_links().iter().cloned(),
                )
                .with_statics([provider.clone(), companion.clone()])
            });

            let index = NativeArtifactIndex::try_new(
                original_index.target(),
                original_index.producer(),
                units,
                [],
            )
            .expect("role provider index must validate");

            let bytes = index.encode().expect("role provider index must encode");

            let digest = NativeContentDigest::new(
                bray_base::sha256_reader(bytes.as_slice()).expect("index must hash"),
            );

            let imported = NativeArtifactIndex::import(
                &bytes,
                digest,
                index.target(),
                index.producer(),
                &original.directory().join("native"),
            )
            .expect("role provider payload must authenticate");

            let reference = bray_runtime_interface::RuntimeNativeIndexMetadata::try_new(
                purpose,
                format!("{}.json", purpose.as_str()),
                digest,
            )
            .expect("provider index reference must validate");

            (reference, imported)
        })
        .collect::<Vec<_>>();

    let metadata = RuntimeArtifactMetadata::try_new(
        original.contract().clone(),
        original.metadata().components().iter().cloned(),
        indexes.iter().map(|(reference, _)| reference.clone()),
    )
    .expect("role provider metadata must validate");

    let runtime = RuntimeArtifact::try_new(
        metadata,
        original.directory().to_path_buf(),
        indexes
            .into_iter()
            .map(|(_, artifact)| artifact)
            .collect::<Vec<_>>()
            .try_into()
            .expect("product and test runtime indexes must be present"),
    )
    .expect("role provider runtime must validate");

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .expect("static cleanup must retain its runtime provider");

    let host = plan
        .product_host()
        .expect("nontrivial static cleanup needs a host");

    let provider_identity = bray_runtime_abi::NativeStaticIdentity::new(provider.identity());

    assert_eq!(host.statics().len(), 3);
    assert_eq!(host.statics()[1].identity(), provider_identity);
    assert_eq!(host.statics()[2].identity().bytes(), companion.identity());

    assert!(
        host.statics()[0].dependencies().is_empty(),
        "reusable records must not capture the final runtime binding"
    );
}

#[test]
#[ignore = "tier:native"]
fn direct_native_imports_close_runtime_and_lifecycle_before_host_mapping() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
        runtime_frames: &[],
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, false, None)),
    );

    let artifact = dependency
        .shared_implementation_artifact()
        .expect("fixture implementation artifact must have shared storage")
        .expect("successful fixture query must contain its expected result");

    let original = artifact
        .native_artifact()
        .expect("fixture `native_artifact` call must succeed")
        .expect("successful fixture query must contain its expected result");

    let unit = &original.units()[0];
    let role = RuntimeAbiRole::PanicReportSuppression;

    let static_entry = bray_native_artifact::NativeStatic::new(
        NonEmptySharedStr::try_new("bray_test_static_host")
            .expect("fixture non empty shared str must be valid"),
        [77; 32],
        Arc::from(b"native-static".as_slice()),
        StaticStorageDuration::Product,
        [],
        Some(bray_symbols::CallableExecution::Synchronous),
        true,
    );

    let NativeUnitSummary::Exact { definitions, .. } = unit.summary() else {
        panic!("native fixture must have an exact index");
    };

    let definitions = definitions
        .iter()
        .cloned()
        .chain([NativeDefinition::new(
            NativeSymbolContract::required_name(
                NonEmptySharedStr::try_new(static_entry.symbol())
                    .expect("fixture non empty shared str must be valid"),
            ),
            NativeDefinitionSelection::Ordinary,
        )])
        .collect::<Vec<_>>();

    let replacement = NativeUnit::new(
        unit.digest(),
        unit.kind(),
        NativeUnitSummary::Exact {
            definitions: definitions.into(),
            references: Arc::from([NativeSymbolContract::required_name(
                NonEmptySharedStr::try_new(
                    role.native_symbol()
                        .expect("fixture runtime role must have a native symbol"),
                )
                .expect("fixture non empty shared str must be valid"),
            )]),
            roots: Arc::from([]),
        },
        [],
    )
    .with_statics([static_entry.clone()]);

    let index =
        NativeArtifactIndex::try_new(original.target(), original.producer(), [replacement], [])
            .expect("fixture native artifact index must be valid");

    let encoded = index.encode().expect("fixture artifact must encode");

    let artifact = artifact
        .try_native_only_artifact(
            &[(NativeUnitKind::Object, &encoded)],
            &[(
                unit.digest().bytes(),
                artifact
                    .native_unit_bytes(unit.digest().bytes())
                    .expect("fixture `native_unit_bytes` call must succeed")
                    .expect("successful fixture query must contain its expected result"),
            )],
        )
        .expect("fixture native only artifact must succeed");

    let dependency = dependency.with_implementation_artifact("native.brayimpl", Arc::new(artifact));

    for (source, retained) in [
        (
            r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled")
                @abi(c)
                extern trusted func native_value() -> i32 uses(foreign_call);

                trusted func main() -> i32 uses(foreign_call) {
                    return trusted native_value();
                }
            "#,
            true,
        ),
        (
            r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled")
                extern trusted static NATIVE_VALUE: i32;

                trusted func main() {
                    let pointer: DataPointer<i32> = NATIVE_VALUE;
                }
            "#,
            true,
        ),
        (
            r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled", presence = optional)
                extern trusted static NATIVE_VALUE: i32;

                trusted func main() {
                    let pointer: DataPointer<i32> = NATIVE_VALUE;
                }
            "#,
            false,
        ),
    ] {
        let selected = SelectedTarget::baseline();

        let compilation = crate::Compilation::load(
            CompilationRequest::with_options(
                crate::test_support::package_identity(),
                vec![crate::test_support::source_input(source, 0)],
                CompilationOptions::new(
                    WorkerBudget::serial(),
                    ProductKind::Executable,
                    selected.clone(),
                )
                .with_native_link_inputs([NativeLinkRequirement::new(
                    NonEmptySharedStr::try_new("native")
                        .expect("fixture non empty shared str must be valid"),
                    NativeLinkKind::Dynamic,
                )]),
            )
            .with_dependency_interfaces([
                dependency.clone(),
                crate::test_support::runtime_standard_library_dependency(&selected),
            ]),
        )
        .expect("fixture input must load");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();

        let target = compilation
            .requested_target()
            .codegen_target()
            .expect("fixture target must support code generation");

        let semantic = compilation
            .product_semantics()
            .expect("fixture product semantics must resolve");

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .expect("fixture product roots must resolve");

        let reachability = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .expect("fixture product must resolve code-generation reachability");

        assert!(reachability.selected_native_units().next().is_none());

        let (demands, statics) = compilation
            .mapped_product_runtime_demands(
                &test_product_identity(),
                &reachability,
                CodegenOptions::default(),
                None,
                false,
                &target,
                &cancellation,
            )
            .expect("fixture `mapped_product_runtime_demands` call must succeed");

        assert_eq!(
            statics.statics(),
            if retained {
                vec![static_entry.clone()]
            } else {
                vec![]
            }
        );

        assert_eq!(
            demands.iter().any(|demand| demand.role() == Some(role)),
            retained
        );
    }
}

#[test]
#[ignore = "tier:native"]
fn runtime_native_references_select_ordinary_library_dependencies() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
        runtime_frames: &[],
        executable_templates: 1,
        platform_service: None,
    };

    let dependency = dependency_from_fixture_with_native(
        true,
        false,
        fixture,
        Some((CodegenOptions::default(), false, true, None)),
    );

    let compilation = native_fixture_compilation(dependency, ProductKind::Executable, None);

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("fixture target must support code generation");

    let directory = tempfile::tempdir().expect("temporary fixture directory must be created");
    let archive = directory.path().join("runtime.lib");

    fs::write(&archive, b"test runtime archive").expect("fixture contents must be written");

    let original = runtime_artifact(&compilation, &archive);
    let original_index = original.native_indexes()[0].index();

    let original_unit = original_index
        .units()
        .iter()
        .find(|unit| unit.kind() == NativeUnitKind::Object)
        .expect("fixture lookup must find its expected value");

    let NativeUnitSummary::Exact { definitions, .. } = original_unit.summary() else {
        panic!("runtime fixture must have exact role definitions");
    };

    let requirements = bray_runtime_interface::RuntimeRequirements::new(
        None,
        compilation.selected_target().target().runtime_abi(),
        None,
        target.identity().clone(),
        target.panic_abi().clone(),
        [RuntimeAbiRole::PanicPropagation],
        [],
        [],
    );

    for required in ["bray_test_precompiled", "missing_library_symbol"] {
        let symbol = NativeSymbolContract::required_name(
            NonEmptySharedStr::try_new(required)
                .expect("fixture non empty shared str must be valid"),
        );

        let unit = NativeUnit::new(
            original_unit.digest(),
            NativeUnitKind::Object,
            NativeUnitSummary::Exact {
                definitions: Arc::clone(definitions),
                references: Arc::from([symbol.clone()]),
                roots: Arc::from([]),
            },
            [],
        );

        let index = NativeArtifactIndex::try_new(
            original_index.target(),
            original_index.producer(),
            [unit],
            [],
        )
        .expect("fixture native artifact index must be valid");

        let bytes = index.encode().expect("fixture artifact must encode");

        let digest = NativeContentDigest::new(
            bray_base::sha256_reader(bytes.as_slice())
                .expect("fixture contents must have a complete SHA-256 digest"),
        );

        let imported = NativeArtifactIndex::import(
            &bytes,
            digest,
            index.target(),
            index.producer(),
            &directory.path().join("native"),
        )
        .expect("fixture `NativeArtifactIndex::import` call must succeed");

        let metadata = RuntimeArtifactMetadata::try_new(
            original.contract().clone(),
            original.metadata().components().iter().cloned(),
            RuntimeArtifactPurpose::ALL.map(|purpose| {
                bray_runtime_interface::RuntimeNativeIndexMetadata::try_new(
                    purpose,
                    format!("{}.json", purpose.as_str()),
                    digest,
                )
                .expect("fixture runtime native index metadata must be valid")
            }),
        )
        .expect("fixture runtime artifact metadata must be valid");

        let runtime = RuntimeArtifact::try_new(
            metadata,
            directory.path().to_owned(),
            [imported.clone(), imported],
        )
        .expect("fixture runtime artifact must be valid");

        let runtime = runtime
            .plan(RuntimeArtifactPurpose::Product, &requirements)
            .expect("fixture `plan` call must succeed");

        let result = compilation.product_link_inputs(
            &[],
            bray_linker::LinkedProductKind::Executable,
            None,
            Some(runtime),
            &[],
            None,
            &target,
            crate::BuildConfiguration::Development,
        );

        if required == "missing_library_symbol" {
            assert!(
                matches!(result, Err(NativeProductPlanningError::NativeResolution(
                    bray_native_artifact::NativeResolutionError::Unresolved(actual)
                )) if actual == symbol)
            );
        } else {
            let (_, payloads) =
                result.expect("runtime references must close through ordinary libraries");

            assert_eq!(payloads.len(), 2);

            assert!(
                payloads
                    .iter()
                    .any(|unit| unit.bytes.as_ref() == b"test object bytes")
            );

            assert!(
                payloads
                    .iter()
                    .any(|unit| unit.bytes.as_ref() == b"test dependency object")
            );

            assert!(
                payloads
                    .iter()
                    .all(|unit| unit.bytes.as_ref() != b"disconnected object")
            );
        }
    }
}
