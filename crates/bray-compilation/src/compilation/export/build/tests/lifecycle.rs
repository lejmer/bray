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
    for execution in ["", "async "] {
        let provider_source = r#"
            module api;
            struct Value<T>
            {
                mut ready: bool;
                payload: T;

                $EXECUTIONfinalize()
                    when(self.ready) { executes(pure, total) }
                {
                    if !self.ready
                    {
                        panic("unfinished");
                    }
                }
            }
        "#
        .replace("$EXECUTION", execution);

        let provider = compilation(&provider_source);

        for (condition, operation, completed) in [
            ("value.ready", "", true),
            ("!value.ready", "", false),
            ("value.ready", "value.ready = false;", false),
        ] {
            let consumer = execution_consumer(
                &provider,
                &format!(
                    r#"
                module app;
                using example.package.api;
                async func root(pos mut value: example.package.api.Value<bool>)
                    requires({condition})
                {{
                    {operation}
                }}
            "#
                ),
            );

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );

            let selected = selected_cleanup(&consumer);

            assert_eq!(!selected.is_empty(), completed);

            let lowered = consumer
                .lowered_unit(source_function_body_key(&consumer, "root"))
                .unwrap();

            assert!(lowered.value().as_ref().unwrap().mir().is_some());
        }
    }
}

#[test]
fn imported_finalizers_preserve_known_successful_results() {
    for execution in ["", "async "] {
        for (result, completed) in [("Ok(unit)", true), ("Error(true)", false)] {
            let provider = compilation(&format!(
                r#"
                module api;
                struct Value
                {{
                    {execution}finalize() -> Result<unit, bool> executes(pure, total)
                    {{
                        return {result};
                    }}
                }}
            "#
            ));

            assert!(
                !provider.check_diagnostics().has_errors(),
                "{:?}",
                provider.check_diagnostics()
            );

            let consumer = execution_consumer(
                &provider,
                r#"
                module app;
                using example.package.api;
                async func root(pos value: example.package.api.Value)
                {
                    panic("abnormal cleanup");
                }
            "#,
            );

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );

            let selected = selected_cleanup(&consumer);

            assert_eq!(!selected.is_empty(), completed, "{execution}{result}");
        }
    }
}

#[test]
fn imported_finalizer_postcondition_proves_infallibility_without_total() {
    let provider = compilation(
        r#"
        module api;
        struct Value
        {
            ready: bool;
            finalize() -> Result<unit, bool>
                when(self.ready) { ensures(result matches Ok(_)) }
            {
                if self.ready
                {
                    return Ok(unit);
                }
                return Error(true);
            }
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (condition, valid) in [("value.ready", true), ("!value.ready", false)] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            func root(pos value: example.package.api.Value) requires({condition})
            {{
            }}
        "#
            ),
        );

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );

            assert!(
                selected_cleanup(&consumer).is_empty(),
                "infallible invocation still executes"
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
            );
        }
    }
}

#[test]
fn imported_finalizer_success_requires_its_certified_guarded_domain() {
    for execution in ["", "async "] {
        let provider = compilation(&format!(
            r#"
            module api;
            struct Value
            {{
                ready: bool;
                {execution}finalize() -> Result<unit, bool>
                    executes(pure, total)
                    when(self.ready) {{ executes(pure, total) }}
                {{
                    if self.ready
                    {{
                        return Ok(unit);
                    }}
                    return Error(true);
                }}
            }}
        "#
        ));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{:?}",
            provider.check_diagnostics()
        );

        for (condition, completed) in [("value.ready", true), ("!value.ready", false)] {
            let consumer = execution_consumer(
                &provider,
                &format!(
                    r#"
                module app;
                using example.package.api;
                func root(pos value: example.package.api.Value) requires({condition})
                {{
                }}
            "#
                ),
            );

            if completed {
                assert!(
                    !consumer.check_diagnostics().has_errors(),
                    "{:?}",
                    consumer.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    consumer.check_diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingUnresolvedFinalization,
                );
            }

            let selected = selected_cleanup(&consumer);

            assert_eq!(!selected.is_empty(), completed, "{execution}{condition}");
        }

        let original = export(&provider);
        let mut removed = 0;

        let contracts = original
            .semantics()
            .callable_contracts()
            .iter()
            .map(|contract| {
                let mut execution = contract.execution_contract().clone();

                let evidence = execution
                    .evidence
                    .iter()
                    .filter(|proof| {
                        if matches!(
                            proof.obligation,
                            bray_symbols::CallableExecutionObligation::Postcondition(_)
                        ) {
                            removed += 1;

                            false
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                execution.evidence = evidence.into();

                contract.clone().with_execution_contract(execution)
            })
            .collect::<Vec<_>>();

        assert!(
            removed > 0,
            "provider must publish its checked result proof"
        );

        let semantics = original.semantics().clone().with_contracts(
            original.semantics().constraints().iter().cloned(),
            contracts,
        );

        assert!(
            PackageInterfaceExportBundle::try_new(
                original.surface().clone(),
                semantics,
                original.language_revision(),
                original.implementation_configuration().clone()
            )
            .is_err(),
            "a result condition without its checked evidence must not be accepted"
        );
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

        using core.atomic;

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
                InterfaceCheckedTemplateOperation::Construct { .. }
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

#[test]
fn imported_inherent_constructor_self_resolves_with_its_generic_subject() {
    for (subject, construct_result, call, caller_result) in [
        ("Value", "Self", "Value(true)", "Value"),
        ("Value<T>", "Self", "Value<bool>(true)", "Value<bool>"),
        (
            "Value<T>",
            "Result<Self, bool>",
            "Value<bool>(true)",
            "Result<Value<bool>, bool>",
        ),
    ] {
        let field = if subject == "Value" { "bool" } else { "T" };

        let result = if construct_result == "Self" {
            "value"
        } else {
            "Ok(value)"
        };

        let source = format!(
            r#"
            module api;
            struct {subject} {{ byte: {field}; }}
            impl {subject}
            {{
                construct(pos byte: {field}) -> {construct_result}
                {{
                    let value: {subject} = {{ byte = byte }};
                    return {result};
                }}
            }}
        "#
        );

        let provider = compilation(&source);

        assert!(
            provider.check_diagnostics().is_empty(),
            "{source}: {:?}",
            provider.check_diagnostics()
        );

        let call = call.replacen("Value", "example.package.api.Value", 1);
        let caller_result = caller_result.replacen("Value", "example.package.api.Value", 1);

        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            func caller() -> {caller_result} {{ return {call}; }}
        "#
            ),
        );

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn source_and_imported_scope_lifecycle_self_matches_the_concrete_generic_subject() {
    let provider = compilation(
        r#"
        module api;
        struct Value<T> { byte: T; }
        impl Value<T>
        {
            consume enter() -> Self { return self; }
            exit(pos lease: Self) {}
        }
        func caller(pos value: Value<bool>) -> Value<bool> { return value; }
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
        func caller(pos value: example.package.api.Value<bool>) -> example.package.api.Value<bool>
        { return value; }
    "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    for compilation in [&provider, &consumer] {
        let key = source_function_body_key(compilation, "caller");
        let bound = compilation.bound_unit(key.clone()).unwrap();
        let types = compilation.expression_types(key).unwrap();

        let ty = bound
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, bray_bound_tree::BoundExpression::Name(_))
                    .then(|| types.value().expression(id).unwrap().ty())
            })
            .expect("caller observes its concrete resource parameter");

        let cancellation = crate::fact::CancellationToken::new();

        for slot in [
            bray_symbols::TypeAssociatedLifecycleSlot::ScopeEnter,
            bray_symbols::TypeAssociatedLifecycleSlot::ScopeExit,
        ] {
            let selected = compilation
                .selected_lifecycle_signature(ty, slot, &cancellation)
                .unwrap();

            assert!(
                selected.diagnostics().is_empty(),
                "{:?}",
                selected.diagnostics()
            );

            let (_, signature) = selected
                .value()
                .as_ref()
                .expect("resource retains its scoped-use declarations");

            match slot {
                bray_symbols::TypeAssociatedLifecycleSlot::ScopeEnter => {
                    assert_eq!(signature.receiver().unwrap().ty(), ty);
                    assert_eq!(signature.result(), ty);
                }
                bray_symbols::TypeAssociatedLifecycleSlot::ScopeExit => {
                    assert!(signature.receiver().is_none());
                    assert_eq!(signature.parameters()[0].ty(), ty);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn scoped_capability_annotations_do_not_select_enter_in_source_or_imported_interfaces() {
    let declarations = r#"
        module api;
        struct Resource<T> {}
        impl Resource<T>
        {
            consume enter() -> bool { return true; }
            exit(pos lease: bool) {}
        }
    "#;

    let provider = compilation(declarations);

    for annotation in ["bool", "i32"] {
        let source = compilation(&format!(
            "{declarations}
            func caller(pos value: Resource<bool>) -> bool
            {{ return with lease: {annotation} = value {{ yield true; }}; }}"
        ));

        let imported = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            func caller(pos value: example.package.api.Resource<bool>) -> bool
            {{ return with lease: {annotation} = value {{ yield true; }}; }}
        "#
            ),
        );

        for compilation in [&source, &imported] {
            let key = source_function_body_key(compilation, "caller");
            let bound = compilation.bound_unit(key.clone()).unwrap();
            let types = compilation.expression_types(key.clone()).unwrap();
            let patterns = compilation.patterns(key.clone()).unwrap();
            let selections = compilation.semantic_selections(key.clone()).unwrap();

            let scoped = selections
                .value()
                .entries()
                .iter()
                .find_map(|entry| match entry.selection() {
                    bray_bound_tree::SemanticSelection::ScopedUse(scoped) => Some(scoped),
                    _ => None,
                })
                .expect("the resource initializer selects its actual enter/exit pair");

            assert_ne!(scoped.source_type(), scoped.capability_type());
            assert_eq!(scoped.enter().1.result(), scoped.capability_type());

            assert_eq!(
                scoped.exit().1.parameters()[0].ty(),
                scoped.capability_type()
            );

            assert!(
                matches!(bound.value().view().expression(scoped.expression()),
                Some(bray_bound_tree::BoundExpression::Structured(expression))
                    if expression.kind() == bray_bound_tree::BoundStructuredExpressionKind::With)
            );

            if annotation == "bool" {
                assert!(types.diagnostics().is_empty(), "{:?}", types.diagnostics());

                assert!(
                    patterns.diagnostics().is_empty(),
                    "{:?}",
                    patterns.diagnostics()
                );

                let storage = compilation.storage_plan(key.clone()).unwrap();

                assert!(
                    storage.diagnostics().is_empty(),
                    "{:?}",
                    storage.diagnostics()
                );

                let (identity, provenance) = storage.value().identity_entries().find(|(_, identity)| matches!(identity,
                    bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. } if *expression == scoped.expression()
                )).expect("successful entry retains separate capability storage");

                let bray_bound_tree::StorageIdentity::ScopedCapability { pattern, .. } = provenance
                else {
                    unreachable!()
                };

                assert_eq!(
                    storage.value().identity_type(identity),
                    Some(scoped.capability_type())
                );

                assert_eq!(provenance.definition_node(), Some(pattern.into()));

                let pattern_node = bound.value().view().pattern(pattern).unwrap();

                assert_eq!(
                    pattern_node.mode(),
                    bray_bound_tree::BoundPatternMode::Scoped
                );

                assert_eq!(
                    patterns.value().pattern(pattern).unwrap().operation(),
                    bray_bound_tree::PatternOperation::Observe
                );

                for binding in pattern_node.bindings() {
                    let Some(bray_bound_tree::StorageBinding::Access(access)) = storage
                        .value()
                        .binding(bray_bound_tree::StorageBindingTarget::Local(*binding))
                    else {
                        panic!("scoped binding aliases its retained capability");
                    };

                    assert_eq!(storage.value().root_identity(access), Some(identity));
                }

                let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                    bound.value().view().expression(scoped.expression())
                else {
                    unreachable!()
                };

                let owners = bray_bound_tree::StorageScopeOwners::collect(bound.value()).unwrap();

                assert_eq!(
                    owners.identity_scope(storage.value(), identity),
                    Some(expression.blocks()[0])
                );
            } else {
                assert!(
                    patterns
                        .diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.kind()
                            == bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern),
                    "{:?}",
                    patterns.diagnostics()
                );
            }
        }
    }
}

fn selected_cleanup(
    consumer: &Compilation,
) -> std::collections::BTreeSet<bray_bound_tree::CompletedFinalizer> {
    let key = source_function_body_key(consumer, "root");
    let cancellation = crate::CancellationToken::new();

    consumer
        .completed_unit_cleanup(&key, &cancellation)
        .unwrap()
        .result()
        .value()
        .clone()
}

#[test]
fn imported_async_finalizers_compose_in_the_consuming_frame() {
    let provider = compilation(
        r#"
        module api;
        extern async func pause();
        struct Value
        {
            value: u32;
            async finalize()
            {
                await pause();
            }
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for body in ["return;", "panic(\"cleanup\");"] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            async func root(pos value: example.package.api.Value)
            {{
                {body}
            }}
        "#
            ),
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let lowered = consumer
            .lowered_unit(source_function_body_key(&consumer, "root"))
            .unwrap();

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(mir.is_valid());

        assert!(
            mir.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { .. })
            )),
            "{body}: imported async finalization must retain its execution contract"
        );
    }
}

#[test]
fn imported_inactive_futures_retain_capture_cleanup_suspensions() {
    let provider = compilation(
        r#"
        module api;
        struct Value
        {
            count: u32;
        }

        async func make() -> Value
        {
            return Value { count = 3 };
        }

        async func discard()
        {
            let pending = make();
        }
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
        .with_implementation_artifact("provider.brayimpl", std::sync::Arc::new(implementation));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;
        using example.package.api;
        async func root()
        {
            let pending = example.package.api.make();
            await example.package.api.discard();
        }
    "#,
        [dependency],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let lowered = consumer
        .lowered_unit(source_function_body_key(&consumer, "root"))
        .unwrap();

    let mir = lowered.value().as_ref().unwrap().mir().unwrap();

    assert!(mir.is_valid());

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
            composition: bray_ir::MirFrameComposition::Abandon,
            ..
        })
    )));

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
            composition: bray_ir::MirFrameComposition::Completion(_),
            ..
        })
    )));

    let skeleton = consumer.imported_symbol_skeleton_result().unwrap();
    let skeleton = skeleton.value().as_deref().unwrap();
    let mut abandoned = 0;

    for template in bundle.executable_templates() {
        let function = skeleton
            .functions()
            .iter()
            .find(|function| {
                function
                    .imported_semantic_key()
                    .is_some_and(|key| key.symbol() == template.owner())
            })
            .unwrap();

        let symbol = skeleton
            .imported_semantic_address(function.id().into())
            .unwrap();

        let address =
            crate::fact::ImportedExecutableTemplateAddress::new(symbol, template.identity());

        let decoded = consumer
            .imported_executable_template_with_cancellation(
                address,
                &crate::CancellationToken::new(),
            )
            .unwrap();

        assert!(
            !decoded.diagnostics().has_errors(),
            "{:?}",
            decoded.diagnostics()
        );

        let mir = decoded.value().as_deref().unwrap();

        assert!(mir.is_valid());

        abandoned += mir
            .operations()
            .iter()
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                        composition: bray_ir::MirFrameComposition::Abandon,
                        ..
                    })
                )
            })
            .count();
    }

    assert!(
        abandoned > 0,
        "imported executable templates retain inactive disposal"
    );
}

#[test]
fn imported_incident_transfer_retains_its_selected_producer_and_payload() {
    let provider = compilation(
        r#"
        module api;
        struct Value
        {
            async finalize() -> Result<unit, u32>
            {
                return Error(7);
            }
        }

        async func fail(pos value: Value)
        {
            panic("cleanup");
        }
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
        .with_implementation_artifact("provider.brayimpl", std::sync::Arc::new(implementation));

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;
        using example.package.api;
        async func root(pos value: example.package.api.Value)
        {
            await example.package.api.fail(value);
        }
        "#,
        [dependency],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let skeleton = consumer.imported_symbol_skeleton_result().unwrap();
    let skeleton = skeleton.value().as_deref().unwrap();

    let function = skeleton
        .functions()
        .iter()
        .find(|function| {
            skeleton
                .member_name(function.id().into())
                .is_some_and(|name| name.as_str() == "fail")
        })
        .unwrap();

    let symbol = skeleton
        .imported_semantic_address(function.id().into())
        .unwrap();

    let template = bundle
        .executable_templates()
        .iter()
        .find(|template| {
            function
                .imported_semantic_key()
                .is_some_and(|key| key.symbol() == template.owner())
        })
        .unwrap();

    let address = crate::fact::ImportedExecutableTemplateAddress::new(symbol, template.identity());

    let decoded = consumer
        .imported_executable_template_with_cancellation(address, &crate::CancellationToken::new())
        .unwrap();

    assert!(
        !decoded.diagnostics().has_errors(),
        "{:?}",
        decoded.diagnostics()
    );

    let mir = decoded.value().as_deref().unwrap();

    assert!(mir.is_valid());

    let (mir, _) = bray_ir::reconstruct_reachable(mir).unwrap();

    assert!(mir.is_valid());

    let mut transferred = 0;

    for operation in mir.operations() {
        let MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
            incident,
            producer,
            invocation,
            ..
        }) = operation.kind()
        else {
            continue;
        };

        let place = incident;

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Quiesce { place: quiescent, .. } if quiescent == place)));

        assert!(mir.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(call), ..
            }) if call.target() == &bray_ir::MirCallTarget::Direct(*producer) && call.result() == *invocation
        )));

        transferred += 1;
    }

    assert!(
        transferred > 0,
        "the exported cleanup retains its incident producer"
    );
}

#[test]
fn imported_generic_destructor_tails_retain_their_execution_context() {
    let provider = compilation(
        r#"
        module api;
        extern async func pause();
        struct Child
        {
            async finalize()
            {
                await pause();
            }
        }
        struct Owner<T>
        {
            child: T;
            destruct()
            {
            }
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (execution, valid) in [("async ", true), ("", false)] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            module app;
            using example.package.api;
            {execution}func root(pos owner: example.package.api.Owner<example.package.api.Child>)
            {{
            }}
        "#
            ),
        );

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );

            let lowered = consumer
                .lowered_unit(source_function_body_key(&consumer, "root"))
                .unwrap();

            assert!(lowered.value().as_ref().unwrap().mir().unwrap().is_valid());
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn imported_scoped_calls_enforce_requirements_on_the_entered_capability() {
    for (enter_contract, exit_contract, body, valid) in [
        ("", "", "example.package.api.observe(&lease);", false),
        (
            "ensures(trusted live(&result))",
            "",
            "example.package.api.observe(&lease);",
            true,
        ),
        ("", "requires(trusted live(&lease))", "", false),
        (
            "ensures(trusted live(&result))",
            "requires(trusted live(&lease))",
            "",
            true,
        ),
    ] {
        let provider = compilation(&format!(
            r#"
            trusted module api;
            struct Resource {{}}
            struct Lease {{ tag: bool; }}
            trusted predicate live(value: &Lease);
            impl Resource
            {{
                trusted enter() -> Lease {enter_contract}
                {{
                    return {{ tag = true }};
                }}

                exit(pos lease: Lease) {exit_contract} {{}}
            }}
            func observe(pos value: &Lease) requires(trusted live(value)) {{}}
            "#,
        ));

        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                module app;
                using example.package.api;
                func caller(pos resource: example.package.api.Resource)
                {{
                    with lease = resource {{ {body} }};
                }}
                "#,
            ),
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_scoped_exit_must_explicitly_transfer_trusted_authority() {
    for (exit_contract, valid) in [("", false), ("ensures(trusted live(lease.tag))", true)] {
        let provider = compilation(&format!(
            r#"
            trusted module api;
            struct Resource {{ tag: bool; }}
            struct Lease {{ tag: bool; }}
            trusted predicate live(value: bool);
            impl Resource
            {{
                trusted enter() -> Lease
                    ensures(result.tag == self.tag, trusted live(result.tag))
                    executes(pure)
                {{
                    return {{ tag = self.tag }};
                }}

                trusted exit(pos lease: Lease) {exit_contract} executes(pure) {{}}
            }}
            func observe(pos value: bool) requires(trusted live(value)) executes(pure) {{}}
            "#,
        ));

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;
            func caller(pos resource: &example.package.api.Resource)
            {
                with lease = resource
                {
                    let copied = lease.tag;
                    let _ = copied;
                };

                example.package.api.observe(resource.tag);
            }
            "#,
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{exit_contract}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_scoped_exit_retains_its_owned_error_quiescence() {
    for execution in ["", "async "] {
        let provider = compilation(&format!(
            r#"
            module api;
            async func pending() {{}}
            struct Failure {{ child: Future<unit>; }}
            struct Resource {{}}
            impl Resource
            {{
                enter() -> bool {{ return true; }}
                {execution}exit(pos lease: bool) -> Result<unit, Failure>
                {{
                    return Error(Failure {{ child = pending() }});
                }}
            }}
            "#,
        ));

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;
            async func caller(pos resource: example.package.api.Resource)
                -> Result<unit, example.package.api.Failure>
            {
                let _ = with lease = resource { return Ok(unit); };

                return Ok(unit);
            }
            "#,
        );

        let diagnostics = consumer.check_diagnostics();

        assert!(diagnostics.is_empty(), "{execution}: {diagnostics:?}");

        let key = source_function_body_key(&consumer, "caller");
        let lowered = consumer.lowered_unit(key).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();
        let mut retained = 0;

        for operation in mir.operations() {
            if let MirOperationKind::Quiesce { collection, .. } = operation.kind() {
                assert!(collection.as_ref().unwrap().retention().is_some());

                retained += 1;
            }
        }

        assert!(
            retained > 0,
            "imported scoped error retains its continuation"
        );
    }
}

#[test]
fn inactive_scoped_protocols_do_not_require_whole_value_cleanup() {
    let declarations = r#"
        module api;
        struct Value
        {
            number: i32;
        }
        struct Resource
        {
            value: Value;
            enter() -> bool
            {
                return true;
            }
            exit(pos lease: bool) {}
        }
    "#;

    let provider = compilation(declarations);

    let source = compilation(&format!(
        r#"{declarations}
        func extract(pos resource: Resource) -> Value
        {{
            return resource.value;
        }}
        "#
    ));

    let imported = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func extract(pos resource: example.package.api.Resource) -> example.package.api.Value
        {
            return resource.value;
        }
        "#,
    );

    for compilation in [&source, &imported] {
        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
