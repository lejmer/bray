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
fn initialized_protected_storage_round_trips_through_an_interface() {
    let provider = compilation(
        r#"
        module api;

        using core.memory;
        struct State { count: u64; ready: bool; }
        static STATE: Uninit<State> = core.memory.uninit_from<State>({ count = 23, ready = true });
        const func protect<T>(pos value: T) -> Uninit<T>
            ensures(trusted core.memory.uninit_initialized<T>(storage = &result))
        {
            return core.memory.uninit_from<T>(value);
        }
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
        trusted module app;

        using core.memory;
        using example.package.api;

        trusted func main() -> u32 uses(unchecked_init)
        {
            let mut storage = example.package.api.protect<u32>(42);
            _ = core.memory.uninit_pointer<example.package.api.State>(&example.package.api.STATE);
            return trusted core.memory.move_initialized<u32>(&mut storage);
        }
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

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

            const func selected(pos value: u8) -> u32
            {
                return value as u32;
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

            const result: u32 = example.package.math.selected(37);
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

    let interface =
        encode_package_interface(bundle).expect("fixture package interface must encode");

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &interface,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .expect("fixture from export bundle must succeed");

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

#[test]
fn construction_inputs_preserve_nullable_constant_values_across_imports() {
    let provider = compilation(
        r#"
        module api;
        public struct Holder { value: i32?; }
        public union Choice { Some(value: i32?); None; }

        public const func hold(pos value: i32) -> Holder { return { value = value }; }
        public const func choose(pos value: i32) -> Choice { return Some(value = value); }
        public const func optional(pos value: i32?) -> i32? { return value; }
        const VALUE: Holder = { value = 17 };
        public const func forwarded(pos value: i32) -> i32? { return optional(value); }
    "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);
    let artifact = encode_package_interface(bundle).expect("fixture package interface must encode");

    let implementation = PackageImplementationArtifact::try_from_export_bundle(
        &artifact,
        bundle,
        InterfaceValidationLimits::default(),
    )
    .expect("fixture from export bundle must succeed");

    let dependency = super::fixtures::execution_bundle_dependency(bundle)
        .with_implementation_artifact("provider.brayimpl", Arc::new(implementation));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;
        using example.package.api;
        const DIRECT: example.package.api.Holder = { value = 23 };
        const HELD: example.package.api.Holder = example.package.api.hold(23);
        const CHOSEN: example.package.api.Choice = example.package.api.choose(23);
        const OPTIONAL: i32? = example.package.api.forwarded(23);
    "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn repeated_array_constant_templates_round_trip_and_instantiate_in_consumers() {
    let provider = compilation(
        r#"
        module api;

        const COUNT: usize = 3;

        const VALUES: [i32; COUNT] = [7; COUNT];

        const func fill<const N: usize>() -> [i32; N]
        {
            return [9; N];
        }

        struct Buffer
        {
            values: [i32; 3] = [5; COUNT];
        }

        struct Repeated<const N: usize>
        {
            const VALUES: [i32; N] = [7; N];
        }
    "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let dependency = super::fixtures::implementation_dependency(export(&provider));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        const VALUES: [i32; 3] = example.package.api.VALUES;

        const GENERATED: [i32; 3] = example.package.api.fill<3>();

        const FIRST: i32 = GENERATED[2];

        const STATIC_LAST: i32 = VALUES[2];

        func main()
        {
            let buffer = example.package.api.Buffer
            {
            };

            assert(buffer.values[2] == 5);
        }
    "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    for (name, expected) in [("FIRST", 9), ("STATIC_LAST", 7)] {
        let value = super::fixtures::source_constant_value(&consumer, name);

        assert!(
            matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(value)
            if value == &bray_symbols::IntegerConstant::from_u64(expected))
        );
    }

    let lowered = consumer
        .lowered_unit(crate::test_support::source_function_body_key(
            &consumer, "main",
        ))
        .expect("imported repeated-array defaults lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn associated_constants_round_trip_with_arguments_and_trait_requirements() {
    let provider = compilation(
        r#"
        module api;

        struct Buffer<const N: usize>
        {
            const VALUE: usize = N;
            const VALUES: [i32; N] = [7; N];

            static const func read() -> usize
            {
                return N;
            }
        }

        impl Buffer<M>
        {
            const EXTRA: usize = M + 1;
            type Target = Buffer<M>;
            type Subject = Large;
        }

        const func indirect<const N: usize>() -> usize
        {
            return Buffer<N>.Target.read();
        }

        struct Wrapper<T>
        {
            const VALUE: usize = 3;
        }

        trait Capacity
        {
            const VALUE: usize;
            const DOUBLE: usize = VALUE + VALUE;
            const WRAPPED: usize = Wrapper<Self>.VALUE;
            const BORROWED: usize = Wrapper<(&Self)>.VALUE;
            const VALUES: [i32; VALUE] = [8; VALUE];
        }

        struct Small
        {
        }

        struct Large
        {
        }

        impl SmallCapacity = Small(Capacity)
        {
            const VALUE: usize = 2;
        }

        impl LargeCapacity = Large(Capacity)
        {
            const VALUE: usize = 5;
        }

        trait Project
        {
            type Target;
        }

        impl BufferProject = Buffer<M>(Project)
        {
            type Target = Small;
        }

        func projected<T>() -> usize with(T: Project, T(Project).Target: Capacity)
        {
            return T(Project).Target(Capacity).VALUE;
        }

        trait Scale<const K: usize>
        {
            const VALUE: usize = K;
        }

        impl BufferScale = Buffer<M>(Scale<M>)
        {
        }

        const func scaled<T, const K: usize>() -> usize with(T: Scale<K>)
        {
            return T(Scale<K>).VALUE;
        }

        const func capacity<T>() -> usize with(T: Capacity)
        {
            return T(Capacity).VALUE;
        }

        const func fill<const N: usize>() -> [i32; N]
        {
            return Buffer<N>.VALUES;
        }
    "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let dependency = super::fixtures::implementation_dependency(export(&provider));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;
        using example.package.api.SmallCapacity;
        using example.package.api.LargeCapacity;
        using example.package.api.BufferScale;
        using example.package.api.BufferProject;

        const ALIAS: usize = example.package.api.Buffer<3>.Target.VALUE;
        const ALIAS_CALL: usize = example.package.api.Buffer<3>.Target.read();
        const ALIAS_TRAIT: usize = example.package.api.Buffer<3>.Subject(example.package.api.Capacity).VALUE;
        const ALIAS_GENERIC: usize = example.package.api.indirect<5>();
        const FIRST: usize = example.package.api.Buffer<3>.VALUE;
        const SECOND: usize = example.package.api.Buffer<5>.VALUE;
        const EXTRA: usize = example.package.api.Buffer<5>.EXTRA;
        const ELEMENT: i32 = example.package.api.Buffer<3>.VALUES[2];
        const GENERATED: [i32; 5] = example.package.api.fill<5>();
        const LAST: i32 = GENERATED[4];
        const SMALL: usize = example.package.api.capacity<example.package.api.Small>();
        const LARGE: usize = example.package.api.capacity<example.package.api.Large>();
        const DEFAULT_ELEMENT: i32 = example.package.api.Small(example.package.api.Capacity).VALUES[1];
        const SCALED_FIRST: usize = example.package.api.scaled<example.package.api.Buffer<3>, 3>();
        const SCALED_SECOND: usize = example.package.api.scaled<example.package.api.Buffer<5>, 5>();
        const SELF_WRAPPED: usize = example.package.api.Small(example.package.api.Capacity).WRAPPED;
        const SELF_BORROWED: usize = example.package.api.Small(example.package.api.Capacity).BORROWED;
        const DOUBLED: usize = example.package.api.Large(example.package.api.Capacity).DOUBLE;

        func main()
        {
            let values: [i32; example.package.api.Buffer<3>.VALUE] = [7; example.package.api.Buffer<3>.VALUE];
            let copied: [i32; 3] = example.package.api.Buffer<3>.VALUES;
            let deferred: usize = example.package.api.projected<example.package.api.Buffer<3>>();

            assert(values[2] == copied[2]);
        }
    "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    for (name, expected) in [
        ("ALIAS", 3),
        ("ALIAS_CALL", 3),
        ("ALIAS_TRAIT", 5),
        ("ALIAS_GENERIC", 5),
        ("FIRST", 3),
        ("SECOND", 5),
        ("EXTRA", 6),
        ("ELEMENT", 7),
        ("LAST", 7),
        ("SMALL", 2),
        ("LARGE", 5),
        ("SCALED_FIRST", 3),
        ("SCALED_SECOND", 5),
        ("DOUBLED", 10),
        ("SELF_WRAPPED", 3),
        ("SELF_BORROWED", 3),
        ("DEFAULT_ELEMENT", 8),
    ] {
        let value = super::fixtures::source_constant_value(&consumer, name);

        assert!(
            matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(value)
            if value == &bray_symbols::IntegerConstant::from_u64(expected)),
            "{name}: {value:?}"
        );
    }

    let lowered = consumer
        .lowered_unit(crate::test_support::source_function_body_key(
            &consumer, "main",
        ))
        .expect("imported associated constant references lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn associated_constants_keep_provider_witnesses_for_imported_default_extents() {
    let provider = compilation(
        r#"
        module api;

        trait Capacity
        {
            const VALUE: usize;
            const VALUES: [i32; VALUE] = [8; VALUE];
        }

        struct Small
        {
        }

        impl ProviderCapacity = Small(Capacity)
        {
            const VALUE: usize = 2;
        }

        const func values() -> [i32; 2]
        {
            return Small(Capacity).VALUES;
        }
        "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let dependency = super::fixtures::implementation_dependency(export(&provider));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;

        using example.package.api;

        impl ConsumerCapacity = example.package.api.Small(example.package.api.Capacity)
        {
            const VALUE: usize = 5;
        }

        const GENERATED: [i32; 2] = example.package.api.values();
        const ELEMENT: i32 = GENERATED[1];
        const CAPACITY: usize = example.package.api.Small(example.package.api.Capacity).VALUE;
        "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    for (name, expected) in [("ELEMENT", 8), ("CAPACITY", 5)] {
        let value = super::fixtures::source_constant_value(&consumer, name);

        assert!(
            matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(value) if value.to_u64() == Some(expected)),
            "{name}"
        );
    }
}
