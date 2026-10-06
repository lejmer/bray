use bray_bound_tree::CheckedTemplateKind;
use bray_checker::ConstantEvaluationLimits;
use bray_ir::{MirOperationKind, MirProjectionKind};
use bray_package_interface::{
    InterfaceCheckedTemplateOperation, InterfaceLanguageRevision, InterfaceProductIdentity,
    InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
    PackageInterfaceExportBundle, ValidatedPackageInterface, encode_package_interface,
};
use bray_symbols::{ConstantValueId, ConstantValueKind, PackageIdentity};

use crate::test_support::source_function_body_key;
use crate::{Compilation, DependencyInterfaceInput};

use super::fixtures::{compilation, execution_consumer, export};

#[test]
fn imported_callable_static_access_orders_local_cleanup() {
    let provider = compilation(
        r#"
        module api;
        static ROOT: i32 = 1;
        func read_root()
        {
            let value = ROOT;
            value;
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        struct Resource {}
        impl Resource
        {
            finalize() { example.package.api.read_root(); }
        }
        static STORED: Resource = Resource {};
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must publish: {error:?}"));

    let root = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("provider skeleton must exist"))
        .statics()[0]
        .id();

    let stored = consumer
        .symbol_graph()
        .unwrap_or_else(|error| panic!("consumer symbols must publish: {error:?}"))
        .statics()[0]
        .id();

    let template = consumer
        .static_instance_template(stored)
        .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

    assert!(
        template.diagnostics().is_empty(),
        "{:#?}",
        template.diagnostics()
    );

    assert_eq!(template.value().lifecycle_dependencies(), [root]);
}

#[test]
fn imported_static_initializer_preserves_helper_static_access() {
    let provider = compilation(
        r#"
        module api;
        static ROOT: i32 = 1;
        const func read_root() -> i32
        {
            return ROOT;
        }
        static COPY: i32 = read_root();
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func observe() { let value = example.package.api.COPY; value; }
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must publish: {error:?}"));

    let statics = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("provider skeleton must exist"))
        .statics();

    let [copy, root] = statics else {
        panic!("provider must export two static declarations");
    };

    let template = consumer
        .static_instance_template(copy.id())
        .unwrap_or_else(|error| panic!("imported static template must publish: {error:?}"));

    assert!(
        template.diagnostics().is_empty(),
        "{:#?}",
        template.diagnostics()
    );

    assert_eq!(template.value().lifecycle_dependencies(), [root.id()]);
}

#[test]
fn imported_runtime_default_preserves_helper_static_access() {
    let provider = compilation(
        r#"
        module api;
        static ROOT: i32 = 1;
        func read_root() -> i32
        {
            return ROOT;
        }
        func with_default(pos value: i32 = read_root())
        {
            value;
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        struct Resource {}
        impl Resource
        {
            finalize() { example.package.api.with_default(); }
        }
        static STORED: Resource = Resource {};
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let imported = consumer
        .imported_symbol_skeleton_result()
        .unwrap_or_else(|error| panic!("imported skeleton must publish: {error:?}"));

    let root = imported
        .value()
        .as_deref()
        .unwrap_or_else(|| panic!("provider skeleton must exist"))
        .statics()[0]
        .id();

    let stored = consumer
        .symbol_graph()
        .unwrap_or_else(|error| panic!("consumer symbols must publish: {error:?}"))
        .statics()[0]
        .id();

    let template = consumer
        .static_instance_template(stored)
        .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

    assert!(
        template.diagnostics().is_empty(),
        "{:#?}",
        template.diagnostics()
    );

    assert_eq!(template.value().lifecycle_dependencies(), [root]);
}

#[test]
fn imported_finalizers_use_verified_entry_conditions() {
    let provider = compilation(
        r#"
        module api;
        struct Value<T>
        {
            ready: bool;
            payload: T;

            finalize()
                when(self.ready) { executes(pure, total) }
            {
                if !self.ready
                {
                    panic("unfinished");
                }
            }
        }
    "#,
    );

    for (condition, completed) in [("value.ready", true), ("!value.ready", false)] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            func root(pos value: example.package.api.Value<bool>)
                requires({condition})
            {{
            }}
        "#
            ),
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let key = source_function_body_key(&consumer, "root");
        let cancellation = crate::CancellationToken::new();

        let body = consumer
            .body_semantics_with_cancellation(key.clone(), &cancellation)
            .unwrap();

        let selected = consumer
            .completed_unit_cleanup(&key, body.result().value().asynchronous(), &cancellation)
            .unwrap();

        assert_eq!(!selected.is_empty(), completed);

        let lowered = consumer.lowered_unit(key).unwrap();

        assert!(lowered.value().as_ref().unwrap().mir().is_some());
    }
}

#[test]
fn imported_union_cleanup_retains_members_without_exported_field_identities() {
    use bray_package_interface::{
        InterfaceStorageMember, InterfaceStorageShape, InterfaceUnionStorageVariant,
    };

    let provider = compilation(
        r#"
            module types;

            struct Guard
            {
                destruct()
                {
                }
            }

            union Choice
            {
                Pair(pos left: Guard, pos right: Guard);
            }
        "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let original = export(&provider);
    let baseline_artifact = encode_package_interface(original).unwrap();

    let baseline = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.types;

            func take(pos item: example.package.types.Guard)
            {
            }

            func partial(pos value: example.package.types.Choice)
            {
                match consume value
                {
                    case Pair(left,..)
                    {
                        take(left);
                    }
                }
            }
        "#,
        [DependencyInterfaceInput::new(
            PackageIdentity::try_new("example.package").unwrap(),
            InterfaceProductIdentity::try_new("library").unwrap(),
            "provider.brayi",
            baseline_artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )],
    );

    assert!(
        baseline.check_diagnostics().is_empty(),
        "baseline: {:?}",
        baseline.check_diagnostics()
    );

    let representations = original
        .semantics()
        .type_representations()
        .iter()
        .map(|representation| {
            let InterfaceStorageShape::Union(variants) = representation.storage() else {
                return representation.clone();
            };

            let variants = variants
                .iter()
                .map(|variant| {
                    InterfaceUnionStorageVariant::new(
                        variant.variant().clone(),
                        variant.members().iter().enumerate().map(|(index, member)| {
                            InterfaceStorageMember::new(
                                if index == 0 {
                                    member.field().cloned()
                                } else {
                                    None
                                },
                                member.ty(),
                            )
                        }),
                    )
                })
                .collect::<Vec<_>>();

            representation
                .clone()
                .with_storage(InterfaceStorageShape::Union(variants.into()))
        })
        .collect::<Vec<_>>();

    let semantics = original
        .semantics()
        .clone()
        .with_type_representations(representations);

    let bundle = PackageInterfaceExportBundle::try_new(
        original.surface().clone(),
        semantics,
        original.language_revision(),
        original.implementation_configuration().clone(),
    )
    .unwrap();

    let artifact = encode_package_interface(&bundle).unwrap();

    let dependency = DependencyInterfaceInput::new(
        PackageIdentity::try_new("example.package").unwrap(),
        InterfaceProductIdentity::try_new("library").unwrap(),
        "provider.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    );

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            module app;

            using example.package.types;

            func take(pos item: example.package.types.Guard)
            {
            }

            func partial(pos value: example.package.types.Choice)
            {
                match consume value
                {
                    case Pair(left,..)
                    {
                        take(left);
                    }
                }
            }
        "#,
        [dependency],
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let key = source_function_body_key(&consumer, "partial");
    let analysis = consumer.async_analysis(key.clone()).unwrap();

    assert!(analysis.value().storage_requirements().iter().filter_map(|requirement| requirement.parts()).flatten().any(|part| {
        part.projections().iter().any(|projection| matches!(projection.projection(), bray_bound_tree::StorageCleanupProjectionKind::UnionPayloadElement { ordinal, .. } if ordinal.raw() == 1))
    }), "{analysis:?}");

    let lowered = consumer.lowered_unit(key).unwrap();
    let mir = lowered.value().as_ref().unwrap().mir().unwrap();

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Cleanup { place, .. } if place.projections().iter().any(|projection| matches!(projection.kind(), MirProjectionKind::ActiveUnionPayloadElement { ordinal, .. } if ordinal.raw() == 1)))), "{mir:?}");
}

#[test]
fn public_static_initializers_round_trip_as_checked_source_templates() {
    let compilation = compilation(
        r#"
            module app;

            static Root: i32 = 1;

            static Alias: &i32 = &Root;

            static Generic<const N: i32>: i32
                with(true) = N;

            static Selected: &i32 = &Generic<1>;

            @thread_local static ThreadValue: i32 = 2;
        "#,
    );

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
fn generic_container_lifecycle_bodies_publish_executable_templates() {
    let compilation = compilation(
        r#"
            module app;

            struct Boxed<T>
            {
                value: T;

                construct(value: T) -> Self
                {
                    return
                    {
                        value = value,
                    };
                }

                destruct()
                {
                }
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bundle = export(&compilation);

    assert_eq!(bundle.executable_templates().len(), 2);
}

#[test]
fn aggregate_static_initializers_export_for_source_independent_consumers() {
    let provider = compilation(
        r#"
        module api;

        struct Pair
        {
            first: i32;
            second: i32;
        }

        struct State
        {
            nested: Pair;
            marker: i32;
        }

        struct AtomicState
        {
            value: core.atomic.Atomic<u32>;
        }

        static Stored: State = State
        {
            marker = 39,
            nested = Pair
            {
                second = 38,
                first = 37
            }
        };

        static Generic<const N: i32>: State
            with(true) = State
        {
            marker = N + 2,
            nested = Pair
            {
                second = N + 1,
                first = N
            }
        };

        static Atomic: AtomicState = AtomicState
        {
            value = core.atomic.initialize<u32>(0)
        };
    "#,
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(
        bundle
            .semantics()
            .checked_templates()
            .iter()
            .flat_map(|template| template.nodes())
            .any(|node| matches!(
                node.operation(),
                InterfaceCheckedTemplateOperation::Product(_)
            ))
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func stored_value() -> i32
        {
            return example.package.api.Stored.nested.first;
        }

        func generic_value() -> i32
        {
            return example.package.api.Generic<41>.nested.second;
        }

        func atomic_value() -> &example.package.api.AtomicState
        {
            return &example.package.api.Atomic;
        }
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let values = [
        ("stored_value", vec![39, 38, 37]),
        ("generic_value", vec![43, 42, 41]),
        ("atomic_value", vec![0]),
    ];

    for (function, expected) in values {
        let key = source_function_body_key(&consumer, function);

        let lowered = consumer
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("imported aggregate static must lower: {error:?}"));

        assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let value = imported_static_initializer_value(&consumer, function);
        let mut actual = Vec::new();

        collect_integer_constants(&consumer, value, &mut actual);

        assert_eq!(actual, expected, "{function}");
    }
}

fn imported_static_initializer_value(consumer: &Compilation, function: &str) -> ConstantValueId {
    let semantics = consumer
        .expression_semantics_with_cancellation(
            source_function_body_key(consumer, function),
            &consumer.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("imported static semantics must publish: {error:?}"));

    let (expression, reference) = semantics
        .result()
        .value()
        .selections()
        .entries()
        .iter()
        .find_map(|entry| match entry.selection() {
            bray_bound_tree::SemanticSelection::StaticReference(reference) => {
                Some((entry.expression(), reference))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{function} must select an imported static"));

    let instance = reference
        .closed_instance()
        .unwrap_or_else(|| panic!("{function} must select a closed static instance"));

    let result_type = semantics
        .result()
        .value()
        .types()
        .expression(expression)
        .unwrap_or_else(|| panic!("{function} static reference must have a checked type"))
        .ty();

    let template = consumer
        .static_instance_template(instance.template().declaration())
        .unwrap_or_else(|error| panic!("imported static template must resolve: {error:?}"));

    let evaluated = consumer
        .evaluate_static_initializer(
            instance,
            template.value().duration(),
            result_type,
            ConstantEvaluationLimits::default(),
            &consumer.state.cancellation,
        )
        .unwrap_or_else(|error| panic!("imported static initializer must evaluate: {error:?}"));

    assert!(
        evaluated.diagnostics().is_empty(),
        "{:#?}",
        evaluated.diagnostics()
    );

    evaluated.value().value()
}

fn collect_integer_constants(
    compilation: &Compilation,
    value: ConstantValueId,
    integers: &mut Vec<u64>,
) {
    let values = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must load: {error:?}"));

    let value = values.constant_value_data(value);

    match value.kind() {
        ConstantValueKind::Integer(value) => integers.push(
            value
                .to_u64()
                .unwrap_or_else(|| panic!("fixture integer must fit in u64")),
        ),
        ConstantValueKind::Product(fields) => {
            for field in fields.iter() {
                collect_integer_constants(compilation, *field.value(), integers);
            }
        }
        kind => panic!("aggregate fixture must contain products and integers, found {kind:?}"),
    }
}
