use super::support::compilation::test_product_identity;
use super::support::dependencies::{
    GenericDependencyFixture, dependency_from_fixture_with_native, native_fixture_compilation,
};
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
fn direct_native_imports_close_runtime_and_lifecycle_before_host_mapping() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
        runtime_frames: None,
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
        .unwrap()
        .unwrap();

    let original = artifact.native_artifact().unwrap().unwrap();
    let unit = &original.units()[0];
    let role = RuntimeAbiRole::PanicReportSuppression;

    let static_entry = bray_native_artifact::NativeStatic::new(
        NonEmptySharedStr::try_new("bray_test_static_host").unwrap(),
        [77; 32],
        Arc::from(b"native-static".as_slice()),
        StaticStorageDuration::Product,
        [],
        true,
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
                NonEmptySharedStr::try_new(static_entry.symbol()).unwrap(),
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
                NonEmptySharedStr::try_new(role.native_symbol().unwrap()).unwrap(),
            )]),
            roots: Arc::from([]),
        },
        [],
    )
    .with_statics([static_entry.clone()]);

    let index =
        NativeArtifactIndex::try_new(original.target(), original.producer(), [replacement], [])
            .unwrap();

    let encoded = index.encode().unwrap();

    let artifact = artifact
        .try_native_only_artifact(
            &[(NativeUnitKind::Object, &encoded)],
            &[(
                unit.digest().bytes(),
                artifact
                    .native_unit_bytes(unit.digest().bytes())
                    .unwrap()
                    .unwrap(),
            )],
        )
        .unwrap();

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
                    let pointer: RawPointer<i32> = NATIVE_VALUE;
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
                    let pointer: RawPointer<i32> = NATIVE_VALUE;
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
                    NonEmptySharedStr::try_new("native").unwrap(),
                    NativeLinkKind::Dynamic,
                )]),
            )
            .with_dependency_interfaces([
                dependency.clone(),
                crate::test_support::runtime_standard_library_dependency(&selected),
            ]),
        )
        .unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();
        let target = compilation.requested_target().codegen_target().unwrap();
        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let reachability = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

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
            .unwrap();

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
fn runtime_native_references_select_ordinary_library_dependencies() {
    let fixture = GenericDependencyFixture {
        source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
        runtime_frames: None,
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
        .unwrap();

    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("runtime.lib");

    fs::write(&archive, b"test runtime archive").unwrap();

    let original = runtime_artifact(&compilation, &archive);
    let original_index = original.native_indexes()[0].index();

    let original_unit = original_index
        .units()
        .iter()
        .find(|unit| unit.kind() == NativeUnitKind::Object)
        .unwrap();

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
        let symbol =
            NativeSymbolContract::required_name(NonEmptySharedStr::try_new(required).unwrap());

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
        .unwrap();

        let bytes = index.encode().unwrap();

        let digest = NativeContentDigest::new(bray_base::sha256_reader(bytes.as_slice()).unwrap());

        let imported = NativeArtifactIndex::import(
            &bytes,
            digest,
            index.target(),
            index.producer(),
            &directory.path().join("native"),
        )
        .unwrap();

        let metadata = RuntimeArtifactMetadata::try_new(
            original.contract().clone(),
            original.metadata().components().iter().cloned(),
            RuntimeArtifactPurpose::ALL.map(|purpose| {
                bray_runtime_interface::RuntimeNativeIndexMetadata::try_new(
                    purpose,
                    format!("{}.json", purpose.as_str()),
                    digest,
                )
                .unwrap()
            }),
        )
        .unwrap();

        let runtime = RuntimeArtifact::try_new(
            metadata,
            directory.path().to_owned(),
            [imported.clone(), imported],
        )
        .unwrap();

        let runtime = runtime
            .plan(RuntimeArtifactPurpose::Product, &requirements)
            .unwrap();

        let result = compilation.product_link_inputs(
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
