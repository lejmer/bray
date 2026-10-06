use std::sync::Arc;

use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_package_interface::{
    InterfaceConstantValueKind, InterfaceSymbolReference, encode_package_interface,
};
use bray_symbols::{
    ExternalSymbolKey, IntegerConstant, ModulePathKey, PackageIdentity, ProductKind, SymbolKey,
    SymbolKind, SymbolName,
};

use crate::{Compilation, CompilationProfileConfiguration, CompilationProfileMode, WorkerBudget};

use super::fixtures::{
    compilation, compilation_from_sources, compilation_from_sources_for_product,
    compilation_from_sources_for_product_with_platform_services_and_worker_budget, export,
};

#[test]
fn export_reuses_resolved_contracts_after_the_query_cache_working_set() {
    use std::fmt::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_symbols::SymbolQueryKind;

    use crate::fact::{CompilationFactKey, FactEvaluationTestObserver};

    let mut source = String::from("module api;\n");
    let count = 4_100;

    for index in 0..count {
        writeln!(
            source,
            r#"
            func item{index}()
            {{
            }}
            "#,
        )
        .unwrap();
    }

    let unchecked = compilation(&source);
    let checked = compilation(&source);
    let evaluations = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&evaluations);

    unchecked
        .state
        .fact_runtime
        .set_test_observer(FactEvaluationTestObserver::new(move |key| {
            if matches!(key, CompilationFactKey::Symbol(query)
                if query.kind() == SymbolQueryKind::CallableContracts)
            {
                observed.fetch_add(1, Ordering::SeqCst);
            }
        }))
        .unwrap();

    let bundle = export(&unchecked);

    assert_eq!(bundle.semantics().callable_contracts().len(), count);
    assert_eq!(evaluations.load(Ordering::SeqCst), count);

    let dependencies = unchecked
        .state
        .fact_runtime
        .dependencies(&CompilationFactKey::PackageInterfaceExportBundle)
        .unwrap()
        .unwrap();

    assert_eq!(
        dependencies
            .iter()
            .filter(|key| matches!(
                key, CompilationFactKey::Symbol(query)
                    if query.kind() == SymbolQueryKind::CallableContracts
            ))
            .count(),
        count
    );

    let checked_evaluations = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&checked_evaluations);

    checked
        .state
        .fact_runtime
        .set_test_observer(FactEvaluationTestObserver::new(move |key| {
            if matches!(key, CompilationFactKey::Symbol(query)
            if query.kind() == SymbolQueryKind::CallableContracts)
            {
                observed.fetch_add(1, Ordering::SeqCst);
            }
        }))
        .unwrap();

    assert!(checked.check_diagnostics().is_empty());
    assert_eq!(checked_evaluations.load(Ordering::SeqCst), count);

    let checked_bundle = export(&checked);

    assert_eq!(checked_evaluations.load(Ordering::SeqCst), count);
    assert_eq!(bundle, checked_bundle);

    assert_eq!(
        encode_package_interface(bundle).unwrap().bytes(),
        encode_package_interface(checked_bundle).unwrap().bytes()
    );

    let dependencies = checked
        .state
        .fact_runtime
        .dependencies(&CompilationFactKey::PackageInterfaceExportBundle)
        .unwrap()
        .unwrap();

    assert_eq!(
        dependencies
            .iter()
            .filter(|key| matches!(
                key, CompilationFactKey::Symbol(query)
                    if query.kind() == SymbolQueryKind::CallableContracts
            ))
            .count(),
        count
    );
}

#[test]
fn module_only_library_exports_are_cached_on_demand() {
    let compilation = compilation(
        r#"
            module app;
        "#,
    );

    assert!(
        compilation
            .state
            .package_interface_export_bundle
            .get()
            .is_none()
    );

    let first = export(&compilation);
    let second = export(&compilation);

    assert!(std::ptr::eq(first, second));
    assert_eq!(first.surface().symbols().symbols().len(), 2);

    assert!(
        compilation
            .state
            .package_interface_export_bundle
            .get()
            .is_some()
    );
}

#[test]
fn library_interfaces_exclude_test_only_block_module_suffixes() {
    let compilation = compilation(
        r#"
            module net;

            func parse_packet()
            {
            }

            @test
            module net.tests
            {
                @test
                func parses_minimal_packet()
                {
                }
            }
        "#,
    );

    let bundle = export(&compilation);

    let package = ExternalSymbolKey::package(
        PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid")),
    );

    let production_module = ExternalSymbolKey::module(
        package.clone(),
        ModulePathKey::try_new(["net"])
            .unwrap_or_else(|| panic!("production module path must be valid")),
    )
    .unwrap_or_else(|| panic!("production module key must be valid"));

    let production_function = ExternalSymbolKey::named(
        production_module,
        SymbolKind::Function,
        SymbolName::try_new("parse_packet")
            .unwrap_or_else(|| panic!("production function name must be valid")),
    )
    .unwrap_or_else(|| panic!("production function key must be valid"));

    let test_module = ExternalSymbolKey::module(
        package,
        ModulePathKey::try_new(["net", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid")),
    )
    .unwrap_or_else(|| panic!("test module key must be valid"));

    assert!(
        bundle
            .surface()
            .symbol_by_external_key(&production_function)
            .is_some()
    );

    assert!(
        bundle
            .surface()
            .symbol_by_external_key(&test_module)
            .is_none()
    );
}

#[test]
fn internal_owner_chains_retain_identity_without_entering_exported_lookup() {
    let compilation = compilation(
        r#"
            module app;

            internal struct Hidden
            {
                func method()
                {
                }
            }
        "#,
    );

    let bundle = export(&compilation);

    assert_eq!(bundle.surface().symbols().symbols().len(), 5);
    assert!(bundle.surface().exports().is_empty());
}

#[test]
fn public_module_re_exports_enter_the_interface_lookup_surface() {
    let compilation = compilation_from_sources([
        r#"
            module a;
        "#,
        r#"
            module b;

            export a;
        "#,
    ]);

    let bundle = export(&compilation);

    let [edge] = bundle.surface().exports() else {
        panic!(
            "expected one module re-export: {:?}",
            bundle.surface().exports()
        );
    };

    assert_eq!(edge.name().as_str(), "a");

    assert_eq!(
        edge.kind(),
        bray_package_interface::ExportedLookupKind::ReExport
    );
}

#[test]
fn parallel_interface_discovery_preserves_encoded_identity() {
    let sources = [
        r#"
            module app.first;

            public struct Boxed<T>
            {
                value: T;
            }

            public func first(pos value: Boxed<i32>) -> Boxed<i32>
            {
                return value;
            }
        "#,
        r#"
            module app.second;

            public func second(pos value: app.first.Boxed<i32>) -> app.first.Boxed<i32>
            {
                return value;
            }
        "#,
    ];

    let serial = compilation_from_sources_with_worker_budget(sources, WorkerBudget::serial());

    let parallel_budget = WorkerBudget::new(4)
        .unwrap_or_else(|error| panic!("parallel worker budget must be valid: {error:?}"));

    let parallel = profiled_compilation_from_sources_with_worker_budget(sources, parallel_budget);

    let serial_artifact = encode_package_interface(export(&serial))
        .unwrap_or_else(|error| panic!("serial interface must encode: {error:?}"));

    let parallel_artifact = encode_package_interface(export(&parallel))
        .unwrap_or_else(|error| panic!("parallel interface must encode: {error:?}"));

    assert_eq!(
        serial_artifact.shared_bytes(),
        parallel_artifact.shared_bytes()
    );

    let profile = parallel
        .profile_report()
        .unwrap_or_else(|| panic!("parallel compilation must retain its profile"));

    let fragment = profile
        .descriptors
        .operations
        .iter()
        .find(|operation| operation.name == "compiler.interface.fragment")
        .and_then(|descriptor| {
            profile
                .operations
                .iter()
                .find(|operation| operation.id == descriptor.id)
        })
        .unwrap_or_else(|| panic!("parallel fragment discovery must be profiled"));

    assert!(fragment.maximum_active_workers > 1);
}

#[test]
fn non_library_products_cannot_export_package_interfaces() {
    for product_kind in [ProductKind::Executable, ProductKind::Test] {
        let compilation = compilation_from_sources_for_product(
            [r#"
                module app;
            "#],
            product_kind,
        );

        assert_eq!(
            compilation.package_interface_export_bundle(),
            Some(&Err(
                super::super::super::PackageInterfaceExportError::InvalidCompilationCause(
                    crate::PackageInterfaceInvalidCompilationCause::ExportContract(
                        crate::PackageInterfaceExportContract::RequestMismatch,
                    ),
                )
            ))
        );
    }
}

#[test]
fn target_gated_contributions_do_not_invalidate_package_interface_export() {
    let compilation = compilation_from_sources([
        r#"
            @target(false)
            module app;

            func disabled()
            {
            }
        "#,
        r#"
            @target(target.pointer.BITS == 64)
            module app;

            func enabled()
            {
            }
        "#,
    ]);

    let bundle = export(&compilation);

    assert_eq!(bundle.surface().symbols().symbols().len(), 3);

    let [dependency] = bundle.semantics().target_dependencies() else {
        panic!("selected contribution must retain one exact target dependency");
    };

    let package = ExternalSymbolKey::package(
        PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid")),
    );

    let module = ExternalSymbolKey::module(
        package,
        ModulePathKey::try_new(["app"]).unwrap_or_else(|| panic!("test module path must be valid")),
    )
    .unwrap_or_else(|| panic!("test module key must be valid"));

    let enabled = ExternalSymbolKey::named(
        module,
        SymbolKind::Function,
        SymbolName::try_new("enabled")
            .unwrap_or_else(|| panic!("test function name must be valid")),
    )
    .unwrap_or_else(|| panic!("test function key must be valid"));

    let owner = bundle
        .surface()
        .symbol_by_external_key(&enabled)
        .unwrap_or_else(|| panic!("enabled function must be exported"));

    assert_eq!(dependency.owner(), &InterfaceSymbolReference::Local(owner));

    let InterfaceSymbolReference::CompilerKnown(semantics) = dependency.property() else {
        panic!("target dependency must retain its compiler-known semantics");
    };

    let declaration = CompilerKnownDeclarationKey::try_new("TargetPointerBits")
        .unwrap_or_else(|| panic!("target pointer-bits key must be valid"));

    let semantic_key = SymbolKey::compiler_known_declaration(declaration, SymbolKind::Constant)
        .unwrap_or_else(|| panic!("target pointer-bits symbol key must be valid"));

    assert_eq!(semantics.key(), &semantic_key);

    let value = bundle
        .semantics()
        .constant_values()
        .get(
            usize::try_from(dependency.value().raw())
                .unwrap_or_else(|_| panic!("target dependency value ID must fit usize")),
        )
        .unwrap_or_else(|| panic!("target dependency value must be exported"));

    assert_eq!(
        value.kind(),
        &InterfaceConstantValueKind::Integer(IntegerConstant::from_u64(64))
    );
}

fn compilation_from_sources_with_worker_budget<const N: usize>(
    sources: [&str; N],
    worker_budget: WorkerBudget,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        sources,
        ProductKind::Library,
        std::iter::empty(),
        worker_budget,
        None,
        [],
    )
}

fn profiled_compilation_from_sources_with_worker_budget<const N: usize>(
    sources: [&str; N],
    worker_budget: WorkerBudget,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        sources,
        ProductKind::Library,
        std::iter::empty(),
        worker_budget,
        Some(CompilationProfileConfiguration::new(
            CompilationProfileMode::Summary,
        )),
        [],
    )
}
