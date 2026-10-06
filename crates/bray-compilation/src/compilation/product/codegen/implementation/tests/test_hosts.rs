use super::support::artifacts::generated_artifacts;
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::linker::test_linker;
use super::support::runtime::runtime_artifact;

use bray_ir::{MirHostOperation, MirOperationKind, MirUnitKind};
use bray_runtime_interface::{RootExecution, RuntimeCapability};
use bray_symbols::ProductKind;
use bray_testing::TemporaryFile;

#[test]
fn native_test_products_emit_every_entry_in_catalog_order() {
    let source = concat!(
        "module app.tests;\n",
        "\n",
        "@test\n",
        "func alpha()\n",
        "{\n",
        "}\n",
        "\n",
        "@test\n",
        "async func gamma()\n",
        "{\n",
        "}\n",
        "\n",
        "@test(serial)\n",
        "func beta()\n",
        "{\n",
        "}\n",
    );

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Test);

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("native test plan must resolve: {error:?}"));

    let catalog = plan
        .test_catalog()
        .unwrap_or_else(|| panic!("native test plan must retain their catalog"));

    assert_eq!(
        catalog
            .entries()
            .iter()
            .map(|entry| entry.identity().declaration().name().as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta", "gamma"]
    );

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("native test plan must retain their host"));

    assert_eq!(host.entries().len(), catalog.entries().len());
    assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
    assert_eq!(host.entries()[1].root(), RootExecution::Synchronous);

    assert!(matches!(
        host.entries()[2].root(),
        RootExecution::Asynchronous { .. }
    ));

    let host_mir = plan
        .units()
        .iter()
        .flat_map(bray_codegen::CodegenUnit::mir_units)
        .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
        .unwrap_or_else(|| panic!("native test plan must retain generated host MIR"));

    let executed_entries = host_mir
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Host(MirHostOperation::ExecuteRoot { entry, .. }) => {
                Some(entry.slot())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(executed_entries, [0, 1, 2]);

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn empty_native_test_products_emit_a_successful_host() {
    let (backend, compilation) =
        codegen_compilation_for_product("module app.tests;\n", ProductKind::Test);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("empty native test plan must resolve: {error:?}"));

    let catalog = plan
        .test_catalog()
        .unwrap_or_else(|| panic!("empty native test plan must retain their catalog"));

    assert!(catalog.entries().is_empty());

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("empty native test plan must retain their host"));

    assert!(host.entries().is_empty());

    let host_mir = plan
        .units()
        .iter()
        .flat_map(bray_codegen::CodegenUnit::mir_units)
        .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
        .unwrap_or_else(|| panic!("empty native test plan must retain generated host MIR"));

    assert!(matches!(
        host_mir.operations(),
        [begin, report, shutdown]
            if matches!(
                begin.kind(),
                MirOperationKind::Host(MirHostOperation::BeginStaticCleanup)
            ) && matches!(
                report.kind(),
                MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents { .. })
            ) && matches!(
                shutdown.kind(),
                MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. })
            )
    ));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}
