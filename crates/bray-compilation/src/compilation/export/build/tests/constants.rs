use std::sync::Arc;

use bray_bound_tree::CheckedTemplateKind;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageImplementationArtifact, encode_package_interface,
};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{ExternalSymbolKey, ModulePathKey, PackageIdentity, SymbolKind, SymbolName};

use crate::{Compilation, CompilationRequest, DependencyInterfaceInput};

use super::fixtures::{compilation, execution_consumer, export};

#[test]
fn imported_constant_templates_preserve_materialization_checks() {
    let provider = compilation(
        r#"
        module api;
        struct Guard
        {
            value: i32;
            destruct() { panic("cleanup"); }
        }
        const TEXT: string = "ready";
        const func same<T>(pos value: T) -> T { return value; }
    "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        const TEXT_COPY: string = example.package.api.TEXT;
        const BAD: example.package.api.Guard =
            example.package.api.same<example.package.api.Guard>(
                example.package.api.Guard { value = 1, }
            );
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        consumer.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant,
    );

    let rejected: Vec<_> = consumer
        .check_diagnostics()
        .by_kind(bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant)
        .collect();

    assert_eq!(rejected.len(), 1, "{:?}", consumer.check_diagnostics());
    assert!(rejected[0].primary_span().is_some());
}

#[test]
fn public_constant_callables_round_trip_as_implementation_bodies() {
    let compilation = compilation(
        r#"
            module math;

            const func selected(pos value: i32) -> i32
            {
                return value;
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let request = compilation
        .package_interface_export_request()
        .unwrap_or_else(|| panic!("constant export request must exist"));

    let bundle = compilation
        .build_package_interface_export_bundle(request)
        .unwrap_or_else(|error| panic!("constant export must build: {error:?}"));

    let bundle = bundle.as_ref();

    let interface = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("constant interface must encode: {error:?}"));

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("constant implementation must encode: {error:?}"));

    let package_identity = PackageIdentity::try_new("example.package")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let package = ExternalSymbolKey::package(package_identity.clone());

    let module = ExternalSymbolKey::module(
        package.clone(),
        ModulePathKey::try_new(["math"])
            .unwrap_or_else(|| panic!("test module path must be valid")),
    )
    .unwrap_or_else(|| panic!("test module key must be valid"));

    let callable = ExternalSymbolKey::named(
        module,
        SymbolKind::Function,
        SymbolName::try_new("selected")
            .unwrap_or_else(|| panic!("test callable name must be valid")),
    )
    .unwrap_or_else(|| panic!("test callable key must be valid"));

    let owner = bundle
        .surface()
        .symbol_by_external_key(&callable)
        .unwrap_or_else(|| panic!("constant callable must be exported"));

    let body = implementation
        .constant_callable_body(owner, bundle.surface())
        .unwrap_or_else(|error| panic!("constant body must decode: {error:?}"))
        .unwrap_or_else(|| panic!("constant body must be present"));

    assert_eq!(
        body.template().kind(),
        CheckedTemplateKind::ConstantCallableBody
    );

    let dependency = DependencyInterfaceInput::new(
        package_identity,
        InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("provider product identity must be valid")),
        "provider.brayi",
        interface.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .with_implementation_artifact("provider.brayimpl", Arc::new(implementation));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        r#"
            module app;

            using example.package.math.selected;

            const result: i32 = example.package.math.selected(37);
        "#,
    );

    let consumer = Compilation::load(
        CompilationRequest::new(
            PackageIdentity::try_new("consumer.package")
                .unwrap_or_else(|| panic!("consumer package identity must be valid")),
            vec![source],
        )
        .with_dependency_interfaces([dependency]),
    )
    .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn generic_constant_type_members_export_forwarded_self_results() {
    let provider = compilation(
        r#"
            module types;

            struct Container<T>
            {
                internal value: T?;

                static const func empty() -> Self
                {
                    return internal make_empty<T>();
                }

                static const func empty_with<U>() -> Self?
                {
                    return internal make_empty<T>();
                }
            }

            internal const func make_empty<T>() -> Container<T>
            {
                return
                {
                    value = none
                };
            }
        "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    export(&provider);
}

#[test]
fn generic_constant_type_members_can_call_generic_constant_helpers() {
    let compilation = compilation(
        r#"
            module values;

            struct Cell<T>
            {
                marker: usize;

                static const func empty() -> Self
                {
                    return internal empty_cell<T>();
                }
            }

            internal const func empty_cell<T>() -> Cell<T>
            {
                return
                {
                    marker = 0
                };
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let request = compilation
        .package_interface_export_request()
        .unwrap_or_else(|| panic!("constant export request must exist"));

    compilation
        .build_package_interface_export_bundle(request)
        .unwrap_or_else(|error| panic!("generic constant member export must build: {error:?}"));
}

#[test]
fn imported_constant_tuple_projections_retain_checked_ordinals() {
    let provider = compilation(
        r#"
        module api;
        public const func first(pos value: (bool, bool)) -> bool { return value.0; }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);
    let interface = encode_package_interface(bundle).unwrap();

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap();

    let dependency = super::fixtures::execution_dependency(&provider)
        .with_implementation_artifact("provider.brayimpl", Arc::new(implementation));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;
        using example.package.api;
        const VALUE: bool = example.package.api.first((false, true));
    "#,
        [dependency],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );
}
