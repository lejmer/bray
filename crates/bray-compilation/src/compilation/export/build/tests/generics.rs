use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    ValidatedPackageInterface, encode_package_interface,
};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{
    AnySymbolId, CallableParameterDefaultValue, PackageIdentity, RuntimeDefaultTemplateReference,
    StaticStorageDuration, TypeExpressionTemplate,
};

use crate::test_support::source_function_body_key;
use crate::{Compilation, CompilationRequest, DependencyInterfaceInput};

use super::fixtures::{compilation, compilation_from_sources, execution_consumer, export};

#[test]
fn imported_generic_references_preserve_declared_argument_counts() {
    const REFERENCE_TYPE: &str = "func(pos value: bool, pos other: bool) -> bool";

    let provider = compilation(
        r#"
        module api;

        func identity<T, U>(pos value: T, pos other: U) -> T
        {
            return value;
        }
    "#,
    );

    for (expression, ty, valid) in [
        ("example.package.api.identity<bool>", REFERENCE_TYPE, false),
        (
            "example.package.api.identity<bool, bool, bool>",
            REFERENCE_TYPE,
            false,
        ),
        (
            "example.package.api.identity<bool, bool>",
            REFERENCE_TYPE,
            true,
        ),
        (
            "example.package.api.identity<bool>(true, true)",
            "bool",
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;

            func check()
            {{
                let value: {ty} = {expression};
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::BindingGenericArgumentCountMismatch,
            );
        }
    }
}

#[test]
fn generic_application_recovery_preserves_imported_parameter_counts() {
    let provider = compilation(
        r#"
        module api;

        struct Boxed<T>
        {
            value: T;
        }

        callable action<T> = func(pos value: T) -> T;

        trait Marker<T>
        {
        }
    "#,
    );

    for (ty, valid) in [
        ("example.package.api.Boxed", false),
        ("example.package.api.Boxed<bool>", true),
        ("example.package.api.action", false),
        ("example.package.api.action<bool>", true),
        ("&view example.package.api.Marker", false),
        ("&view example.package.api.Marker<bool>", true),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;

            func check(pos value: {ty})
            {{
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::BindingGenericArgumentCountMismatch,
            );
        }
    }
}

#[test]
fn abstract_trait_results_survive_package_interfaces() {
    let provider = compilation(
        r#"
        module api;

        trait Project
        {
            func project() -> &bool;

            func next() -> &Self
            {
                return &self;
            }
        }

        func forward<T>(pos value: &T, pos repeat: bool = false) -> &bool with(T: Project)
        {
            if repeat
            {
                return forward<T>(value, false);
            }

            return value.project();
        }

        func forward_loop<T>(pos value: &T, pos repeat: bool = false) -> &bool with(T: Project)
        {
            let mut current: &T = value;
            let mut pending = repeat;

            while pending
            {
                current = current.next();
                pending = false;
            }

            return current.project();
        }
    "#,
    );

    for forward in ["forward", "forward_loop"] {
        for (body, valid) in [
            (
                r#"
            func caller(pos value: &Holder) -> &bool
            {
                return example.package.api.forward(value);
            }
        "#,
                true,
            ),
            (
                r#"
            func caller() -> &bool
            {
                let local = Holder
                {
                    value = true,
                };

                return example.package.api.forward(&local);
            }
        "#,
                false,
            ),
        ] {
            let source = format!(
                r#"
            module app;

            using example.package.api.Project;
            using example.package.api.forward;

            struct Holder
            {{
                value: bool;
            }}

            impl HolderProject = Holder(example.package.api.Project)
            {{
                func project() -> &bool
                {{
                    return &self.value;
                }}
            }}

            {body}
        "#
            );

            let source = source.replace("api.forward", &format!("api.{forward}"));
            let consumer = execution_consumer(&provider, &source);

            let flow = consumer
                .storage_flow(source_function_body_key(&consumer, "caller"))
                .unwrap();

            if valid {
                assert!(
                    !flow.diagnostics().has_errors(),
                    "{source}: {:?}",
                    flow.diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn type_owned_callable_overloads_round_trip_through_package_interfaces() {
    let compilation = compilation(
        r#"
            module app;

            struct Value<T>
            {
                stored: T;

                internal construct single(pos value: T) -> Self
                {
                    return
                    {
                        stored = value
                    };
                }

                internal construct pair(pos first: T, pos second: T) -> Self
                {
                    return
                    {
                        stored = first
                    };
                }

                overload new =
                {
                    single,
                    pair
                }
            }
        "#,
    );

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
    let compilation = compilation(
        r#"
            module app;

            trait Base
            {
            }

            trait Extension
            {
            }

            impl DefaultExtension = Subject(Extension)
                with(Subject: Base)
            {
            }
        "#,
    );

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
fn public_callable_and_type_semantics_round_trip_without_source() {
    let compilation = compilation(
        r#"
            module app;

            struct Boxed<T>
            {
                value: T;
            }

            union Maybe<T>
            {
                Some(value: T);
                None;
            }

            func identity<T>(pos value: T) -> T
                with(true)
            {
                return value;
            }

            func count(pos value: i32 = 1) -> usize
                requires(value > 0)
            {
                return 1;
            }
        "#,
    );

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
fn callable_signatures_export_fixed_array_lengths() {
    let compilation = compilation(
        r#"
            module app;

            internal func first(pos values: &[u8; 32]
            ) -> u8
                {
                    return values[0];
                }
        "#,
    );

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_signatures().len(), 1);
}

#[test]
fn exported_callable_and_type_semantics_intern_without_provider_source() {
    let provider = compilation(
        r#"
            module app;

            struct Boxed<T>
            {
                value: T;
            }

            trait Provides
            {
                type Item;
            }

            impl Boxed<i32>
            {
                type Local = i32;
            }

            impl Boxed<i32>(Provides)
            {
                type Item = i32;
            }

            func count(pos value: i32 = 1) -> usize requires(value > 0)
            {
                return 1;
            }

            static ProductValue: i32 = 1;
            @thread_local static ThreadValue: i32 = 2;
        "#,
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
        r#"
            module app;
        "#,
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
fn imported_generic_type_members_reuse_the_receiver_substitution() {
    let provider = compilation(
        r#"
            module types;

            struct Factory<T>
            {
                static func empty() -> Self
                {
                    panic("fixture");
                }

                static func identity<U>(pos value: U) -> U
                {
                    return value;
                }
            }

            struct Guard<T>
            {
                internal value: T;

                mut func get() -> &mut T
                {
                    panic("fixture");
                }
            }
        "#,
    );

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
        r#"
            module app;

            using example.package.types.Factory;
            using example.package.types.Guard;

            func run(pos guard: &mut example.package.types.Guard<i32>)
            {
                let value: example.package.types.Factory<i32> = example.package.types.Factory<i32>.empty();
                let text: string = example.package.types.Factory<i32>.identity<string>("ok");
                let value_ref: &mut i32 = guard.get();

                value_ref += 1;
            }
        "#,
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
fn named_callable_contract_applications_export_in_public_signatures() {
    let compilation = compilation_from_sources([r#"
        trusted module app;

        callable ThreadStart = @abi(c)
        func(pos context: RawPointer<u8>) -> RawPointer<u8>;

        callable Transform<T> = @abi(c)
        func(pos value: T) -> T;

        callable FixedTransform<const N: usize> = @abi(c)
        func(pos value: RawPointer<[u8; N]>) -> RawPointer<[u8; N]>;

        trusted func register(pos start: ThreadStart, pos transform: Transform<i32>, pos fixed: FixedTransform<4>) -> i32
        {
            return 0;
        }
    "#]);

    let bundle = export(&compilation);

    assert_eq!(bundle.semantics().callable_signatures().len(), 1);
}

#[test]
fn member_access_distinguishes_trait_applications_from_value_arguments() {
    let declarations = r#"
        @copy
        struct Flag
        {
            value: bool;
        }

        trait Reader<T>
        {
            func read() -> T;
        }

        impl FlagReader = Flag(Reader<bool>)
        {
            func read() -> bool
            {
                return self.value;
            }
        }

        func identity(pos value: Flag) -> Flag
        {
            return value;
        }

        func generic_identity<T>(pos value: T) -> T
        {
            return value;
        }
    "#;

    let provider = compilation(&format!("module api;\n{declarations}"));

    for imported in [false, true] {
        let prefix = if imported {
            "example.package.api."
        } else {
            "api."
        };

        for (expression, failure) in [
            (format!("flag({prefix}Reader<bool>).read()"), None),
            (format!("{prefix}identity(Reader).value"), None),
            (
                format!("{prefix}generic_identity<{prefix}Flag>(Reader).value"),
                None,
            ),
            (
                format!("{prefix}identity<{prefix}Flag>(Reader).value"),
                Some(bray_diagnostics::DiagnosticKind::CheckingIncompatibleCandidate),
            ),
            (
                format!("flag({prefix}Reader).read()"),
                Some(bray_diagnostics::DiagnosticKind::BindingGenericArgumentCountMismatch),
            ),
            (
                format!("{prefix}identity(true).value"),
                Some(bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType),
            ),
        ] {
            let body = format!(
                r#"
                func check(pos flag: {prefix}Flag) -> bool
                {{
                    let Reader = flag;

                    return {expression};
                }}
            "#
            );

            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &format!(
                        "module app;\nusing example.package.api;\nusing example.package.api.FlagReader;\n{body}"
                    ),
                )
            } else {
                compilation_from_sources([
                    &format!("module api;\n{declarations}"),
                    &format!("module app;\nusing api;\n{body}"),
                ])
            };

            let diagnostics = consumer.check_diagnostics();

            match failure {
                Some(kind) => bray_testing::assert_goal_state_diagnostic_kind(&diagnostics, kind),
                None => assert!(!diagnostics.has_errors(), "{body}\n{diagnostics:?}"),
            }
        }
    }
}
