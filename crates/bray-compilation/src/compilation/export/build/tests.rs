use std::collections::BTreeSet;
use std::sync::Arc;

use bray_bound_tree::CheckedTemplateKind;
use bray_compiler_known::{CompilerKnownDeclarationKey, RecognizedStandardLibraryDeclarationKey};
use bray_ir::{MirOperationKind, MirProjectionKind};
use bray_package_interface::{
    InterfaceCheckedTemplateOperation, InterfaceConstantValueKind, InterfaceLanguageRevision,
    InterfaceProductIdentity, InterfaceSymbolReference, InterfaceValidationLimits,
    InterfaceValidationPolicy, PackageImplementationArtifact, PackageInterfaceExportBundle,
    ValidatedPackageInterface, build_package_interface_surface, encode_package_interface,
};
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    AnySymbolId, CallableParameterDefaultValue, ExternalSymbolKey, InherentImplementationSymbolId,
    IntegerConstant, MemberLookupResult, ModulePathKey, PackageIdentity, ProductKind,
    RuntimeDefaultTemplateReference, StaticStorageDuration, SymbolKey, SymbolKind, SymbolName,
    TypeCallableMemberSymbolId, TypeExpressionTemplate,
};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};
use bray_testing::test_source_inputs;

use crate::test_support::{
    package_version, source_function_body_key, source_named_trait_callable_fulfillment_body_key,
};
use crate::{
    Compilation, CompilationOptions, CompilationProfileConfiguration, CompilationProfileMode,
    CompilationRequest, DependencyInterfaceInput, PackageInterfaceExportRequest, SelectedTarget,
    WorkerBudget,
};

#[test]
fn module_only_library_exports_are_cached_on_demand() {
    let compilation = compilation("module app;");

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
    let compilation = compilation(concat!(
        "module net;\n",
        "\n",
        "func parse_packet()\n",
        "{\n",
        "}\n",
        "\n",
        "@test\n",
        "module net.tests\n",
        "{\n",
        "    @test\n",
        "    func parses_minimal_packet()\n",
        "    {\n",
        "    }\n",
        "}\n",
    ));

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
    let compilation = compilation(concat!(
        "module app;\n",
        "internal struct Hidden\n",
        "{\n",
        "    func method()\n",
        "    {\n",
        "    }\n",
        "}\n",
    ));

    let bundle = export(&compilation);

    assert_eq!(bundle.surface().symbols().symbols().len(), 5);
    assert!(bundle.surface().exports().is_empty());
}

#[test]
fn type_owned_callable_overloads_round_trip_through_package_interfaces() {
    let compilation = compilation(concat!(
        "module app;\n",
        "public struct Value<T>\n",
        "{\n",
        "    stored: T;\n",
        "    internal construct single(pos value: T) -> Self\n",
        "    {\n",
        "        return { stored = value };\n",
        "    }\n",
        "    internal construct pair(pos first: T, pos second: T) -> Self\n",
        "    {\n",
        "        return { stored = first };\n",
        "    }\n",
        "    overload new = {single, pair}\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "unexpected diagnostics: {:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let artifact = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("overloaded interface must encode: {error:?}"));

    ValidatedPackageInterface::try_new(
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("overloaded interface must validate: {error:?}"));
}

#[test]
fn generic_trait_implementations_round_trip_through_package_interfaces() {
    let compilation = compilation(concat!(
        "module app;\n",
        "public trait Base\n",
        "{\n",
        "}\n",
        "public trait Extension\n",
        "{\n",
        "}\n",
        "impl DefaultExtension = Subject(Extension) with(Subject: Base)\n",
        "{\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "unexpected diagnostics: {:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let artifact = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("generic implementation interface must encode: {error:?}"));

    ValidatedPackageInterface::try_new(
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("generic implementation interface must validate: {error:?}"));
}

#[test]
fn public_module_re_exports_enter_the_interface_lookup_surface() {
    let compilation =
        compilation_from_sources(["module a;\n", concat!("module b;\n", "\n", "export a;\n",)]);

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
fn public_callable_and_type_semantics_round_trip_without_source() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "public struct Boxed<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "\n",
        "public union Maybe<T>\n",
        "{\n",
        "    Some(value: T);\n",
        "    None;\n",
        "}\n",
        "\n",
        "public func identity<T>(pos value: T) -> T with(true)\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "\n",
        "public func count(pos value: i32 = 1) -> usize requires(value > 0)\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let bundle = export(&compilation);

    let artifact = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("public interface must encode: {error:?}"));

    let validated = ValidatedPackageInterface::try_new(
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("public interface must validate: {error:?}"));

    let surface = validated
        .decode_identity_surface()
        .unwrap_or_else(|error| panic!("identity surface must decode: {error:?}"));

    let semantics = validated
        .decode_semantics(&surface)
        .unwrap_or_else(|error| panic!("semantics must decode: {error:?}"));

    assert_eq!(semantics.callable_signatures().len(), 2);
    assert_eq!(semantics.generic_declarations().len(), 4);
    assert_eq!(semantics.constraints().len(), 1);
    assert_eq!(semantics.checked_templates().len(), 3);
    assert_eq!(semantics.declaration_templates().len(), 3);
    assert_eq!(semantics.declared_types().len(), 2);
    assert_eq!(semantics.type_representations().len(), 2);
}

#[test]
fn parallel_interface_discovery_preserves_encoded_identity() {
    let sources = [
        concat!(
            "module app.first;\n",
            "public struct Boxed<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "public func first(pos value: Boxed<i32>) -> Boxed<i32>\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ),
        concat!(
            "module app.second;\n",
            "public func second(pos value: app.first.Boxed<i32>) -> app.first.Boxed<i32>\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ),
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
fn public_static_initializers_round_trip_as_checked_source_templates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "public static Root: i32 = 1;\n",
        "public static Alias: &i32 = &Root;\n",
        "public static Generic<const N: i32>: i32 with(true) = N;\n",
        "public static Selected: &i32 = &Generic<1>;\n",
        "@thread_local public static ThreadValue: i32 = 2;\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let artifact = encode_package_interface(bundle)
        .unwrap_or_else(|error| panic!("static interface must encode: {error:?}"));

    PackageImplementationArtifact::try_from_export_bundle(
        &artifact,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("static implementation must encode: {error:?}"));

    let validated = ValidatedPackageInterface::try_new(
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap_or_else(|error| panic!("static interface must validate: {error:?}"));

    let surface = validated
        .decode_identity_surface()
        .unwrap_or_else(|error| panic!("static identity surface must decode: {error:?}"));

    let semantics = validated
        .decode_semantics(&surface)
        .unwrap_or_else(|error| panic!("static semantics must decode: {error:?}"));

    assert_eq!(
        semantics
            .checked_templates()
            .iter()
            .filter(|template| { template.kind() == CheckedTemplateKind::ProductStaticInitializer })
            .count(),
        4
    );

    assert!(semantics.checked_templates().iter().any(|template| {
        template.nodes().iter().any(|node| {
            matches!(
                node.operation(),
                InterfaceCheckedTemplateOperation::Declaration {
                    substitution: Some(_),
                    ..
                }
            )
        })
    }));

    assert_eq!(
        semantics
            .checked_templates()
            .iter()
            .filter(|template| {
                template.kind() == CheckedTemplateKind::ThreadLocalStaticInitializer
            })
            .count(),
        1
    );

    assert_eq!(semantics.target_dependencies().len(), 1);
    assert_eq!(semantics.constraints().len(), 1);
}

#[test]
fn callable_signatures_export_fixed_array_lengths() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "internal func first(pos values: &[u8; 32]) -> u8\n",
        "{\n",
        "    return values[0];\n",
        "}\n",
    ));

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_signatures().len(), 1);
}

#[test]
fn generic_container_lifecycle_bodies_publish_executable_templates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "public struct Boxed<T>\n",
        "{\n",
        "    value: T;\n",
        "\n",
        "    construct(value: T) -> Self\n",
        "    {\n",
        "        return { value = value, };\n",
        "    }\n",
        "\n",
        "    destruct()\n",
        "    {\n",
        "    }\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    assert_eq!(bundle.executable_templates().len(), 2);
}

#[test]
fn qualified_union_case_and_generic_constructor_defaults_export() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "public union Radix\n",
        "{\n",
        "    Decimal;\n",
        "}\n",
        "\n",
        "public union Alignment\n",
        "{\n",
        "    Right;\n",
        "}\n",
        "\n",
        "public union Sign\n",
        "{\n",
        "    NegativeOnly;\n",
        "}\n",
        "\n",
        "public union Escaping\n",
        "{\n",
        "    Raw;\n",
        "}\n",
        "\n",
        "public struct Options\n",
        "{\n",
        "    radix: Radix;\n",
        "    precision: usize?;\n",
        "    width: usize;\n",
        "    alignment: Alignment;\n",
        "    sign: Sign;\n",
        "    escaping: Escaping;\n",
        "\n",
        "    construct(\n",
        "        radix: Radix = Radix.Decimal,\n",
        "        precision: usize? = none,\n",
        "        width: usize = 0,\n",
        "        alignment: Alignment = Right,\n",
        "        sign: Sign = NegativeOnly,\n",
        "        escaping: Escaping = Raw,\n",
        "    ) -> Self\n",
        "    {\n",
        "        return\n",
        "        {\n",
        "            radix = radix,\n",
        "            precision = precision,\n",
        "            width = width,\n",
        "            alignment = alignment,\n",
        "            sign = sign,\n",
        "            escaping = escaping,\n",
        "        };\n",
        "    }\n",
        "}\n",
        "\n",
        "public func options_with_precision(precision: usize) -> Options\n",
        "{\n",
        "    return Options(precision = precision);\n",
        "}\n",
        "\n",
        "internal func default_options() -> Options\n",
        "{\n",
        "    return Options();\n",
        "}\n",
        "\n",
        "public struct Argument<T>\n",
        "{\n",
        "    value: T;\n",
        "    options: Options;\n",
        "\n",
        "    construct(value: T, options: Options = default_options()) -> Self\n",
        "    {\n",
        "        return { value = value, options = options };\n",
        "    }\n",
        "}\n",
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    assert!(!bundle.semantics().callable_parameter_defaults().is_empty());
}

#[test]
fn platform_service_implementations_publish_their_role_with_the_root_template() {
    let Some(binding) =
        PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
    else {
        panic!("test platform binding must be valid");
    };

    let compilation = compilation_from_sources_for_product_with_platform_services(
        [r#"trusted module app;

@layout(c)
internal struct PlatformStatus
{
category: u32;
reserved: u32;
native_code: i64;
}

@abi(c)
trusted internal func flush() -> PlatformStatus
{
return { category = 0, reserved = 0, native_code = 0 };
}
"#],
        ProductKind::Library,
        [binding],
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    let platform_templates = bundle
        .executable_templates()
        .iter()
        .filter(|template| template.platform_service().is_some())
        .collect::<Vec<_>>();

    assert_eq!(platform_templates.len(), 1);

    assert_eq!(
        platform_templates[0].identity(),
        bray_ir::MirExecutableTemplateId::ROOT
    );

    assert_eq!(
        platform_templates[0].platform_service(),
        Some(PlatformServiceRole::StandardOutputFlush)
    );
}

#[test]
fn exported_callable_and_type_semantics_intern_without_provider_source() {
    let provider = compilation(concat!(
        "module app;\n",
        "\n",
        "public struct Boxed<T>\n",
        "{\n",
        "    value: T;\n",
        "}\n",
        "\n",
        "public trait Provides\n",
        "{\n",
        "    type Item;\n",
        "}\n",
        "\n",
        "public impl Boxed<i32>\n",
        "{\n",
        "    type Local = i32;\n",
        "}\n",
        "\n",
        "public impl Boxed<i32>(Provides)\n",
        "{\n",
        "    type Item = i32;\n",
        "}\n",
        "\n",
        "public func count(pos value: i32 = 1) -> usize requires(value > 0)\n",
        "{\n",
        "    return 1;\n",
        "}\n",
        "\n",
        "public static ProductValue: i32 = 1;\n",
        "@thread_local public static ThreadValue: i32 = 2;\n",
    ));

    let artifact = encode_package_interface(export(&provider))
        .unwrap_or_else(|error| panic!("provider interface must encode: {error:?}"));

    let provider_package = PackageIdentity::try_new("example.package")
        .unwrap_or_else(|| panic!("provider package identity must be valid"));

    let provider_product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("provider product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        provider_package,
        provider_product,
        "provider.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer_package = PackageIdentity::try_new("consumer.package")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        "module app;\n",
    );

    let request = CompilationRequest::new(consumer_package, vec![source])
        .with_dependency_interfaces([dependency]);

    let consumer = Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.imported_diagnostics().is_empty(),
        "{:?}",
        consumer.imported_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must build: {error:?}"));

    let skeleton = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("provider interface must contribute an imported skeleton"));

    assert_eq!(skeleton.structures().len(), 1);
    assert_eq!(skeleton.functions().len(), 1);

    let [product_static, thread_static] = skeleton.statics() else {
        panic!("provider must export two static declarations");
    };

    for (declaration, duration) in [
        (product_static.id(), StaticStorageDuration::Product),
        (thread_static.id(), StaticStorageDuration::ExactThread),
    ] {
        consumer
            .symbol_type_template(declaration.into())
            .unwrap_or_else(|error| panic!("imported static type must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("imported static must carry a declared type"));

        let template = consumer
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("imported {duration:?} static must resolve: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "{:?}",
            template.diagnostics()
        );

        assert_eq!(template.value().duration(), duration);
    }

    let [inherent_type_member] = skeleton.inherent_type_members() else {
        panic!("provider must export one inherent type-valued member");
    };

    let [trait_type_fulfillment] = skeleton.trait_type_fulfillments() else {
        panic!("provider must export one trait type-valued fulfillment");
    };

    for symbol in [
        AnySymbolId::InherentTypeMember(inherent_type_member.id()),
        AnySymbolId::TraitTypeFulfillment(trait_type_fulfillment.id()),
    ] {
        let semantics = consumer
            .symbol_type_template(symbol)
            .unwrap_or_else(|error| panic!("imported type semantics must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("imported symbol must carry a declared type"));

        assert!(
            semantics.diagnostics().is_empty(),
            "{:?}",
            semantics.diagnostics()
        );

        assert!(matches!(
            semantics.value(),
            TypeExpressionTemplate::Resolved(_)
        ));
    }

    let [parameter] = skeleton.callable_parameters() else {
        panic!("provider must export one callable parameter");
    };

    let default = consumer
        .callable_parameter_default(parameter.id())
        .unwrap_or_else(|error| panic!("imported parameter default must resolve: {error:?}"));

    assert!(
        default.diagnostics().is_empty(),
        "{:?}",
        default.diagnostics()
    );

    let CallableParameterDefaultValue::Valid(default) = default.value().value() else {
        panic!("imported parameter default must be valid");
    };

    assert!(matches!(
        default.template_reference(),
        RuntimeDefaultTemplateReference::Interface { .. }
    ));
}

#[test]
fn public_constant_callables_round_trip_as_implementation_bodies() {
    let compilation = compilation(concat!(
        "module math;\n",
        "public const func selected(pos value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
    ));

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
        concat!(
            "module app;\n",
            "using example.package.math.selected;\n",
            "const result: i32 = example.package.math.selected(37);\n",
        ),
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
    let provider = compilation(concat!(
        "module types;\n",
        "public struct Container<T>\n",
        "{\n",
        "    internal value: T?;\n",
        "    public static const func empty() -> Self\n",
        "    {\n",
        "        return internal make_empty<T>();\n",
        "    }\n",
        "    public static const func empty_with<U>() -> Self?\n",
        "    {\n",
        "        return internal make_empty<T>();\n",
        "    }\n",
        "}\n",
        "internal const func make_empty<T>() -> Container<T>\n",
        "{\n",
        "    return { value = none };\n",
        "}\n",
    ));

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    export(&provider);
}

#[test]
fn generic_constant_type_members_can_call_generic_constant_helpers() {
    let compilation = compilation(concat!(
        "module values;\n",
        "public struct Cell<T>\n",
        "{\n",
        "    public marker: usize;\n",
        "    public static const func empty() -> Self\n",
        "    {\n",
        "        return internal empty_cell<T>();\n",
        "    }\n",
        "}\n",
        "internal const func empty_cell<T>() -> Cell<T>\n",
        "{\n",
        "    return { marker = 0 };\n",
        "}\n",
    ));

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
fn imported_generic_type_members_reuse_the_receiver_substitution() {
    let provider = compilation(concat!(
        "module types;\n",
        "\n",
        "public struct Factory<T>\n",
        "{\n",
        "    public static func empty() -> Self\n",
        "    {\n",
        "        panic(\"fixture\");\n",
        "    }\n",
        "\n",
        "    public static func identity<U>(pos value: U) -> U\n",
        "    {\n",
        "        return value;\n",
        "    }\n",
        "}\n",
        "\n",
        "public struct Guard<T>\n",
        "{\n",
        "    internal value: T;\n",
        "\n",
        "    public mut func get() -> &mut T\n",
        "    {\n",
        "        panic(\"fixture\");\n",
        "    }\n",
        "}\n",
    ));

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let artifact = encode_package_interface(export(&provider))
        .unwrap_or_else(|error| panic!("provider interface must encode: {error:?}"));

    let provider_package = PackageIdentity::try_new("example.package")
        .unwrap_or_else(|| panic!("provider package identity must be valid"));

    let provider_product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("provider product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        provider_package,
        provider_product,
        "provider.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer_package = PackageIdentity::try_new("consumer.package")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        concat!(
            "module app;\n",
            "\n",
            "using example.package.types.Factory;\n",
            "using example.package.types.Guard;\n",
            "\n",
            "func run(pos guard: &mut example.package.types.Guard<i32>)\n",
            "{\n",
            "    let value: example.package.types.Factory<i32> =\n",
            "        example.package.types.Factory<i32>.empty();\n",
            "    let text: string =\n",
            "        example.package.types.Factory<i32>.identity<string>(\"ok\");\n",
            "    let value_ref: &mut i32 = guard.get();\n",
            "    value_ref += 1;\n",
            "}\n",
        ),
    );

    let request = CompilationRequest::new(consumer_package, vec![source])
        .with_dependency_interfaces([dependency]);

    let consumer = Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let lowered = consumer
        .lowered_unit(source_function_body_key(&consumer, "run"))
        .unwrap_or_else(|error| panic!("imported mutable generic access must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());
}

#[test]
fn standard_memory_surface_exports_uninitialized_storage() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../standard-library/std/src/memory.bray"),
    ]);

    let source_graph = compilation
        .product_source_graph()
        .unwrap_or_else(|error| panic!("standard memory source graph must build: {error:?}"));

    assert!(
        source_graph.diagnostics().is_empty(),
        "standard memory source graph diagnostics: {:?}",
        source_graph.diagnostics()
    );

    let product = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("standard memory product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "standard memory product diagnostics: {:?}",
        product.diagnostics()
    );

    assert!(
        !product.value().is_recovered(),
        "standard memory product semantics must not recover"
    );

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("standard memory symbol graph must build: {error:?}"));

    let identity = super::construction::build_identity_surface(
        &compilation,
        symbols,
        product.value().public_symbols(),
    )
    .unwrap_or_else(|error| panic!("standard memory identity surface must build: {error:?}"));

    let request = compilation
        .package_interface_export_request()
        .unwrap_or_else(|| panic!("standard memory export request must exist"));

    let surface = build_package_interface_surface(
        request.identity().clone(),
        [],
        identity.symbols,
        identity.relationships,
        identity.exports,
    )
    .unwrap_or_else(|error| panic!("standard memory interface surface must build: {error:?}"));

    let (semantics, _, _, _) = super::super::semantic::build_semantics(
        &compilation,
        symbols,
        &surface,
        &identity.selected,
        &identity.keys,
    )
    .unwrap_or_else(|error| panic!("standard memory semantic export must build: {error:?}"));

    for (template_index, template) in semantics.checked_templates().iter().enumerate() {
        for (node_index, node) in template.nodes().iter().enumerate() {
            let InterfaceCheckedTemplateOperation::Borrow { kind, operand } = node.operation()
            else {
                continue;
            };

            let operand_index = usize::try_from(operand.raw()).unwrap_or_else(|error| {
                panic!(
                    "standard memory borrow operand for template {template_index} node {node_index} must fit: {error:?}"
                )
            });

            let operand_ty = template.nodes()[operand_index].ty();

            let node_ty = semantics
                .types()
                .iter()
                .enumerate()
                .find_map(|(index, ty)| {
                    u32::try_from(index)
                        .ok()
                        .filter(|index| {
                            bray_package_interface::InterfaceTypeId::new(*index) == node.ty()
                        })
                        .map(|_| ty)
                });

            assert!(
                matches!(
                    node_ty,
                    Some(bray_package_interface::InterfaceType::Borrow {
                        kind: type_kind,
                        target,
                    }) if *type_kind == *kind && *target == operand_ty
                ),
                "standard memory borrow template {template_index} node {node_index} kind {kind:?} operand type {operand_ty:?} must match node type {node_ty:?}"
            );
        }
    }

    assert_strictly_canonical("constraints", semantics.constraints());
    assert_strictly_canonical("callable contracts", semantics.callable_contracts());
    assert_strictly_canonical("callable signatures", semantics.callable_signatures());
    assert_strictly_canonical("generic declarations", semantics.generic_declarations());

    assert_strictly_canonical(
        "callable parameter defaults",
        semantics.callable_parameter_defaults(),
    );

    assert_strictly_canonical("predicate definitions", semantics.predicate_definitions());
    assert_strictly_canonical("declared types", semantics.declared_types());
    assert_strictly_canonical("type representations", semantics.type_representations());
    assert_strictly_canonical("implementations", semantics.implementations());
    assert_strictly_canonical("coherence", semantics.coherence());
    assert_strictly_canonical("target dependencies", semantics.target_dependencies());
    assert_strictly_canonical("ABI dependencies", semantics.abi_dependencies());
    assert_strictly_canonical("runtime requirements", semantics.runtime_requirements());
    assert_strictly_canonical("provenance", semantics.provenance());

    compilation
        .package_implementation_configuration(None)
        .unwrap_or_else(|error| {
            panic!("standard memory implementation configuration must build: {error:?}")
        });

    let _ = export(&compilation);
}

#[test]
fn standard_memory_api_fixture_checks() {
    let compilation = standard_library_compilation([
        include_str!("../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../../standard-library/std/tests/api/memory.bray"),
    ]);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "standard memory API diagnostics: {:?}",
        compilation.check_diagnostics()
    );

    let mut recovered = Vec::new();

    walk_syntax_tree(compilation.syntax_tree(), |event| {
        if let SyntaxWalkEvent::EnterNode(node) = event
            && node.is_recovered()
        {
            recovered.push((node.kind(), node.full_range()));
        }

        SyntaxWalkControl::Continue
    });

    assert!(
        recovered.is_empty(),
        "standard memory API syntax must not recover: {recovered:?}"
    );
}

#[test]
fn standard_string_equality_satisfies_source_and_imported_generic_constraints() {
    let provider = standard_library_compilation([
        include_str!("../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../standard-library/std/src/string.bray"),
        concat!(
            "module bray.standard_library_tests.string_operations;\n",
            "using std.string.StringEquatable;\n",
            "func generic_equal<T>(pos left: T, pos right: T) -> bool\n",
            "    with(T: Equatable<T>)\n",
            "{\n",
            "    return left == right;\n",
            "}\n",
            "func source_string_equality()\n",
            "{\n",
            "    assert(generic_equal<string>(\"same\", \"same\"));\n",
            "}\n",
        ),
    ]);

    assert!(
        provider.check_diagnostics().is_empty(),
        "source standard-library equality diagnostics: {:#?}",
        provider.check_diagnostics()
    );

    let artifact = encode_package_interface(export(&provider))
        .unwrap_or_else(|error| panic!("string interface must encode: {error:?}"));

    let standard_library = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        standard_library,
        product,
        "std.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer_package = PackageIdentity::try_new("std.tests.api")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        concat!(
            "module bray.standard_library_tests.string_operations;\n",
            "using std.string.StringEquatable;\n",
            "func generic_equal<T>(pos left: T, pos right: T) -> bool\n",
            "    with(T: Equatable<T>)\n",
            "{\n",
            "    return left == right;\n",
            "}\n",
            "func imported_string_equality()\n",
            "{\n",
            "    assert(generic_equal<string>(\"same\", \"same\"));\n",
            "}\n",
        ),
    );

    let consumer = Compilation::load(
        CompilationRequest::new(consumer_package, vec![source])
            .with_dependency_interfaces([dependency])
            .with_standard_library_source_authority(),
    )
    .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.check_diagnostics().is_empty(),
        "imported standard-library equality diagnostics: {:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn standard_formatting_surface_round_trips_and_specializes_without_provider_source() {
    let provider = standard_library_compilation([
        include_str!("../../../../../../standard-library/std/src/std.bray"),
        concat!(
            "module std.memory;\n",
            "union MemoryLayoutError\n",
            "{\n",
            "    SizeOverflow;\n",
            "    UnsupportedAlignment;\n",
            "}\n",
            "extern func byte_slice_pointer(pos bytes: &[u8]) -> RawPointer<u8>;\n",
            "extern func byte_slice_pointer_mut(pos bytes: &mut [u8]) -> RawPointer<u8>;\n",
            "extern trusted func byte_buffer_copy(\n",
            "    pos source: RawPointer<u8>,\n",
            "    pos destination: RawPointer<u8>,\n",
            "    count: usize,\n",
            ");\n",
            "extern trusted func byte_buffer_fill(\n",
            "    destination: RawPointer<u8>,\n",
            "    value: u8,\n",
            "    count: usize,\n",
            ");\n",
        ),
        concat!(
            "module std.bytes;\n",
            "using std.memory;\n",
            "struct Buffer\n",
            "{\n",
            "    internal value: bool;\n",
            "\n",
            "    internal construct(capacity: usize = 0)\n",
            "        -> Result<Self, std.memory.MemoryLayoutError>\n",
            "    {\n",
            "        let buffer: Buffer =\n",
            "        {\n",
            "            value = false,\n",
            "        };\n",
            "\n",
            "        return Ok(buffer);\n",
            "    }\n",
            "\n",
            "    func as_slice() -> &[u8]\n",
            "    {\n",
            "        return as_slice(&self);\n",
            "    }\n",
            "}\n",
            "extern func as_slice(pos buffer: &Buffer) -> &[u8];\n",
            "extern func length(pos buffer: &Buffer) -> usize;\n",
            "extern func push(pos buffer: &mut Buffer, value: u8)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
            "extern internal func append_slice(pos buffer: &mut Buffer, pos bytes: &[u8])\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
            "extern internal func append_repeated(pos buffer: &mut Buffer, pos value: u8, pos count: usize)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
            "overload append =\n",
            "{\n",
            "    append_slice,\n",
            "    append_repeated,\n",
            "}\n",
            "extern func reserve(pos buffer: &mut Buffer, additional: usize)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
            "extern func resize(pos buffer: &mut Buffer, new_length: usize, fill: u8 = 0)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>;\n",
        ),
        concat!(
            "module std.string;\n",
            "union Utf8Error\n",
            "{\n",
            "    InvalidEncoding;\n",
            "}\n",
            "impl string\n",
            "{\n",
            "    func as_bytes() -> &[u8]\n",
            "    {\n",
            "        return internal utf8(&self);\n",
            "    }\n",
            "    static func from_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>\n",
            "    {\n",
            "        return internal decode_utf8(bytes);\n",
            "    }\n",
            "}\n",
            "extern internal func utf8(pos value: &string) -> &[u8];\n",
            "extern internal func decode_utf8(pos bytes: &[u8]) -> Result<string, Utf8Error>;\n",
        ),
        include_str!("../../../../../../standard-library/std/src/character.bray"),
        include_str!("../../../../../../standard-library/std/src/numeric/checked.bray"),
        include_str!("../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../standard-library/std/src/format/options.bray"),
        include_str!("../../../../../../standard-library/std/src/format/argument.bray"),
        include_str!("../../../../../../standard-library/std/src/format/sink.bray"),
        include_str!("../../../../../../standard-library/std/src/format/integer_width.bray"),
        include_str!("../../../../../../standard-library/std/src/format/rendering.bray"),
        concat!(
            "trusted module std.io;\n",
            "union IoErrorKind\n",
            "{\n",
            "    BrokenStream;\n",
            "}\n",
            "struct IoError\n",
            "{\n",
            "    kind: IoErrorKind;\n",
            "    transferred: usize;\n",
            "}\n",
            "trait Writer\n",
            "{\n",
            "    mut func write(pos source: &[u8]) -> Result<usize, IoError>\n",
            "        requires(blocking_execution());\n",
            "    mut func flush() -> Result<unit, IoError>\n",
            "        requires(blocking_execution());\n",
            "    mut func write_all(pos source: &[u8]) -> Result<unit, IoError>\n",
            "        requires(blocking_execution())\n",
            "    {\n",
            "        let length: usize = source.length();\n",
            "        let mut written: usize = 0;\n",
            "        while written < length\n",
            "        {\n",
            "            let result: Result<usize, IoError> = self.write(&source[written..length]);\n",
            "            match consume result\n",
            "            {\n",
            "                case Ok(count)\n",
            "                {\n",
            "                    if count == 0 || count > length - written\n",
            "                    {\n",
            "                        return Error({ kind = IoErrorKind.BrokenStream, transferred = written });\n",
            "                    }\n",
            "                    written += count;\n",
            "                }\n",
            "                case Error(error) { return Error(prefixed_error(error, prefix = written)); }\n",
            "            }\n",
            "        }\n",
            "        return Ok(unit);\n",
            "    }\n",
            "}\n",
            "internal func smaller(pos left: usize, pos right: usize) -> usize\n",
            "{\n",
            "    if left < right\n",
            "    {\n",
            "        return left;\n",
            "    }\n",
            "    return right;\n",
            "}\n",
            "internal func prefixed_error(pos error: IoError, prefix: usize) -> IoError\n",
            "{\n",
            "    return { kind = error.kind, transferred = prefix + error.transferred };\n",
            "}\n",
        ),
        include_str!("../../../../../../standard-library/std/src/io/formatting.bray"),
        crate::test_support::RUNTIME_MEMORY_SOURCE,
        crate::test_support::RUNTIME_TEXT_SOURCE,
        crate::test_support::RUNTIME_CHARACTER_SOURCE,
    ]);

    assert!(
        provider.syntax_tree_result().diagnostics().is_empty(),
        "{:?}",
        provider.syntax_tree_result().diagnostics()
    );

    assert!(
        provider.declaration_diagnostics().is_empty(),
        "{:?}",
        provider.declaration_diagnostics()
    );

    let product = provider
        .product_semantics()
        .unwrap_or_else(|error| panic!("formatting product semantics must build: {error:?}"));

    assert!(
        product.diagnostics().is_empty(),
        "{:?}",
        product.diagnostics()
    );

    assert!(!product.value().is_recovered());

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let adapter = provider
        .lowered_unit(source_named_trait_callable_fulfillment_body_key(
            &provider,
            "WriterFormattingSink",
            "write",
        ))
        .unwrap_or_else(|error| panic!("writer formatting adapter must lower: {error:?}"));

    let adapter = adapter
        .value()
        .as_ref()
        .and_then(bray_lowering::LoweredUnit::mir)
        .unwrap_or_else(|| panic!("writer formatting adapter must produce MIR: {adapter:#?}"));

    assert!(
        adapter.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Borrow { place, .. }
                if place
                    .projections()
                    .iter()
                    .any(|projection| matches!(projection.kind(), MirProjectionKind::Field(_)))
                    && matches!(
                        place.projections().last().map(bray_ir::MirProjection::kind),
                        Some(MirProjectionKind::Dereference)
                    )
        )),
        "generic writer field borrow must reach the destination value: {adapter:#?}"
    );

    let interface = export(&provider);

    let runtime_capabilities: BTreeSet<_> = interface
        .semantics()
        .runtime_requirements()
        .iter()
        .flat_map(|requirement| requirement.requirements().capabilities())
        .copied()
        .collect();

    assert!(runtime_capabilities.is_empty());

    let artifact = encode_package_interface(interface)
        .unwrap_or_else(|error| panic!("formatting interface must encode: {error:?}"));

    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated = ValidatedPackageInterface::try_new(artifact.bytes(), policy)
        .unwrap_or_else(|error| panic!("formatting interface must validate: {error:?}"));

    let implementation = PackageImplementationArtifact::try_new(
        &validated,
        interface.surface(),
        interface.semantics(),
        interface.implementation_configuration().clone(),
        [],
        interface.executable_templates().iter().cloned(),
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .unwrap_or_else(|error| panic!("formatting implementation must encode: {error:?}"));

    let provider_package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let provider_product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let dependency = DependencyInterfaceInput::new(
        provider_package.clone(),
        provider_product,
        "std.brayi",
        artifact.shared_bytes(),
        policy,
    )
    .with_implementation_artifact("std.brayimpl", Arc::new(implementation));

    let consumer_package = PackageIdentity::try_new("example.application")
        .unwrap_or_else(|| panic!("consumer package identity must be valid"));

    let source = SourceInput::virtual_text(
        SourceIdentity::new(0),
        "consumer.bray",
        SourceVersion::new(0),
        concat!(
            "module app;\n",
            "using std.format;\n",
            "using std.format.ByteSinkFormatting;\n",
            "using std.format.StringFormat;\n",
            "using std.format.I32Format;\n",
            "using std.format.U32Format;\n",
            "using std.bytes;\n",
            "using std.io;\n",
            "using std.io.WriterFormattingSink;\n",
            "using std.memory;\n",
            "struct RecordingWriter\n",
            "{\n",
            "    mut written: usize;\n",
            "}\n",
            "impl RecordingWriterIo = RecordingWriter(std.io.Writer)\n",
            "{\n",
            "    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>\n",
            "        requires(blocking_execution())\n",
            "    {\n",
            "        let length: usize = source.length();\n",
            "        self.written += length;\n",
            "        return Ok(length);\n",
            "    }\n",
            "    mut func flush() -> Result<unit, std.io.IoError>\n",
            "        requires(blocking_execution())\n",
            "    {\n",
            "        return Ok(unit);\n",
            "    }\n",
            "}\n",
            "func render(pos destination: &mut std.format.ByteSink, pos value: string)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "    return std.format.write(\n",
            "        destination,\n",
            "        std.format.Argument<string>(&value),\n",
            "    );\n",
            "}\n",
            "func render_integer(pos destination: &mut std.format.ByteSink, pos value: i32)\n",
            "    -> Result<unit, std.memory.MemoryLayoutError>\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "    return std.format.write(\n",
            "        destination,\n",
            "        std.format.Argument<i32>(&value),\n",
            "    );\n",
            "}\n",
            "func resolved_defaults() -> std.format.Options\n",
            "{\n",
            "    return std.format.Options();\n",
            "}\n",
            "public trusted func stream_integer(pos writer: &mut RecordingWriter, pos value: u32)\n",
            "    -> Result<unit, std.io.IoError>\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "    let mut destination: std.io.FormattingSink<RecordingWriter> =\n",
            "        std.io.FormattingSink<RecordingWriter>(writer);\n",
            "    return trusted std.format.write_to<\n",
            "        u32,\n",
            "        std.io.FormattingSink<RecordingWriter>,\n",
            "        std.io.IoError\n",
            "    >(\n",
            "        &mut destination,\n",
            "        std.format.Argument<u32>(&value),\n",
            "    );\n",
            "}\n",
        ),
    );

    let options = CompilationOptions::new(
        WorkerBudget::default(),
        ProductKind::Library,
        SelectedTarget::default(),
    );

    let request = CompilationRequest::with_options(consumer_package, vec![source], options)
        .with_dependency_interfaces([dependency]);

    let consumer = Compilation::load(request)
        .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"));

    assert!(
        consumer.imported_diagnostics().is_empty(),
        "{:?}",
        consumer.imported_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("formatting skeleton must build: {error:?}"));

    let skeleton = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton"));

    let recognized = Arc::clone(
        imported
            .value()
            .as_ref()
            .unwrap_or_else(|| panic!("formatting interface must contribute a skeleton")),
    )
    .recognize_standard_library(&provider_package, |_| true);

    let recognized_key = |value| {
        RecognizedStandardLibraryDeclarationKey::try_new(value)
            .unwrap_or_else(|| panic!("recognized standard-library key must be valid: {value}"))
    };

    assert!(
        recognized
            .declaration_symbol::<InherentImplementationSymbolId>(&recognized_key(
                "StandardStringImplementation",
            ))
            .is_some(),
        "the string implementation must retain its imported identity"
    );

    for key in ["StandardStringAsBytes", "StandardStringFromUtf8"] {
        assert!(
            recognized
                .declaration_symbol::<TypeCallableMemberSymbolId>(&recognized_key(key))
                .is_some(),
            "{key} must retain its nested imported identity"
        );
    }

    let package = skeleton
        .package_by_identity(&provider_package)
        .unwrap_or_else(|| panic!("standard-library package must be imported"));

    assert!(
        consumer
            .symbol_graph()
            .unwrap_or_else(|error| panic!("consumer source symbols must build: {error:?}"))
            .packages()
            .iter()
            .all(|package| package.identity() != &provider_package),
        "provider symbols must come only from the package interface"
    );

    let format_path = ModulePathKey::try_new(["format"])
        .unwrap_or_else(|| panic!("format module path must be valid"));

    let format = skeleton
        .module_by_path(package.id(), &format_path)
        .unwrap_or_else(|| panic!("format module must be imported"));

    for name in [
        "Argument",
        "ByteSink",
        "ByteSinkFormatting",
        "Options",
        "write",
        "write_to",
    ] {
        assert!(
            matches!(
                skeleton.lookup(format.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    let bytes_path = ModulePathKey::try_new(["bytes"])
        .unwrap_or_else(|| panic!("bytes module path must be valid"));

    let bytes = skeleton
        .module_by_path(package.id(), &bytes_path)
        .unwrap_or_else(|| panic!("bytes module must be imported"));

    assert!(matches!(
        skeleton.lookup(bytes.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let memory_path = ModulePathKey::try_new(["memory"])
        .unwrap_or_else(|| panic!("memory module path must be valid"));

    let memory = skeleton
        .module_by_path(package.id(), &memory_path)
        .unwrap_or_else(|| panic!("memory module must be imported"));

    assert!(matches!(
        skeleton.lookup(memory.id().into(), "slice_length"),
        MemberLookupResult::NotFound
    ));

    let io_path =
        ModulePathKey::try_new(["io"]).unwrap_or_else(|| panic!("io module path must be valid"));

    let io = skeleton
        .module_by_path(package.id(), &io_path)
        .unwrap_or_else(|| panic!("io module must be imported"));

    for name in ["IoError", "Writer", "WriterFormattingSink"] {
        assert!(
            matches!(
                skeleton.lookup(io.id().into(), name),
                MemberLookupResult::Found(_)
            ),
            "{name} must be supplied by the imported package interface"
        );
    }

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let lowered = consumer
        .lowered_unit(source_function_body_key(&consumer, "resolved_defaults"))
        .unwrap_or_else(|error| panic!("imported named constructor must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(!skeleton.traits().is_empty());
    assert!(!skeleton.structures().is_empty());
    assert!(!skeleton.named_trait_implementations().is_empty());

    let streamed = consumer
        .lowered_unit(source_function_body_key(&consumer, "stream_integer"))
        .unwrap_or_else(|error| panic!("imported formatting adapter must lower: {error:?}"));

    assert!(streamed.value().is_some(), "{:#?}", streamed.diagnostics());

    assert!(
        streamed.diagnostics().is_empty(),
        "{:#?}",
        streamed.diagnostics()
    );

    let imported_instances = consumer
        .imported_codegen_instance_count_for_test()
        .unwrap_or_else(|error| panic!("imported formatting reachability must close: {error:?}"));

    assert!(imported_instances > 0);
}

#[test]
fn non_library_products_cannot_export_package_interfaces() {
    for product_kind in [ProductKind::Executable, ProductKind::Test] {
        let compilation = compilation_from_sources_for_product(["module app;"], product_kind);

        assert_eq!(
            compilation.package_interface_export_bundle(),
            Some(&Err(
                super::super::PackageInterfaceExportError::InvalidCompilationCause(
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
        concat!(
            "@target(false)\n",
            "module app;\n",
            "\n",
            "func disabled()\n",
            "{\n",
            "}\n",
        ),
        concat!(
            "@target(target.pointer.BITS == 64)\n",
            "module app;\n",
            "\n",
            "func enabled()\n",
            "{\n",
            "}\n",
        ),
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

#[test]
fn named_callable_contract_applications_export_in_public_signatures() {
    let compilation = compilation_from_sources([concat!(
        "trusted module app;\n",
        "\n",
        "callable ThreadStart = @abi(c)\n",
        "func(pos context: RawPointer<u8>) -> RawPointer<u8>;\n",
        "\n",
        "callable Transform<T> = @abi(c)\n",
        "func(pos value: T) -> T;\n",
        "\n",
        "callable FixedTransform<const N: usize> = @abi(c)\n",
        "func(pos value: RawPointer<[u8; N]>) -> RawPointer<[u8; N]>;\n",
        "\n",
        "trusted func register(\n",
        "    pos start: ThreadStart,\n",
        "    pos transform: Transform<i32>,\n",
        "    pos fixed: FixedTransform<4>\n",
        ") -> i32\n",
        "{\n",
        "    return 0;\n",
        "}\n",
    )]);

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_signatures().len(), 1);
}

#[test]
fn runtime_defaults_export_after_disabled_target_gated_contributions() {
    let compilation = compilation_from_sources([
        concat!(
            "@target(false)\n",
            "module app;\n",
            "\n",
            "func disabled()\n",
            "{\n",
            "}\n",
        ),
        concat!(
            "module app;\n",
            "\n",
            "func selected(pos value: i64? = none) -> i64?\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ),
    ]);

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_parameter_defaults().len(), 1);
}

fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
    match compilation.package_interface_export_bundle() {
        Some(Ok(bundle)) => bundle,
        Some(Err(error)) => panic!("test library interface must build: {error:?}"),
        None => panic!("test compilation must configure a library interface"),
    }
}

fn assert_strictly_canonical<T>(table: &str, values: &[T])
where
    T: std::fmt::Debug + Ord,
{
    if let Some(pair) = values.windows(2).find(|pair| pair[0] >= pair[1]) {
        panic!(
            "standard memory {table} are not canonical: {:?} then {:?}",
            pair[0], pair[1]
        );
    }
}

fn compilation(source: &str) -> Compilation {
    compilation_from_sources([source])
}

fn compilation_from_sources<const N: usize>(sources: [&str; N]) -> Compilation {
    compilation_from_sources_for_product(sources, ProductKind::Library)
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
    )
}

fn compilation_from_sources_for_product<const N: usize>(
    sources: [&str; N],
    product_kind: ProductKind,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services(
        sources,
        product_kind,
        std::iter::empty(),
    )
}

fn compilation_from_sources_for_product_with_platform_services<const N: usize>(
    sources: [&str; N],
    product_kind: ProductKind,
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        sources,
        product_kind,
        platform_services,
        WorkerBudget::default(),
        None,
    )
}

fn compilation_from_sources_for_product_with_platform_services_and_worker_budget<const N: usize>(
    sources: [&str; N],
    product_kind: ProductKind,
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    worker_budget: WorkerBudget,
    profile: Option<CompilationProfileConfiguration>,
) -> Compilation {
    let package = PackageIdentity::try_new("example.package")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("test product identity must be valid"));

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        product,
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .unwrap_or_else(|| panic!("test export identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let sources = test_source_inputs("test", sources);

    let options = CompilationOptions::new(worker_budget, product_kind, SelectedTarget::default());

    let request = CompilationRequest::with_options(package, sources, options)
        .with_platform_services(platform_services)
        .with_package_interface_export(export);

    let request = match profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
}

fn standard_library_compilation<const N: usize>(sources: [&str; N]) -> Compilation {
    let package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("standard-library product identity must be valid"));

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        product,
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .unwrap_or_else(|| panic!("standard-library export identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let sources = test_source_inputs("standard", sources);

    let request = CompilationRequest::new(package, sources)
        .with_standard_library_source_authority()
        .with_package_interface_export(export);

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard-library compilation must load: {error:?}"))
}
