use std::sync::Arc;

use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    PackageInterfaceExportBundle, ValidatedPackageInterface, encode_package_interface,
};
use bray_symbols::{PackageIdentity, ProductKind};
use bray_testing::test_source_inputs;

use crate::test_support::package_version;
use crate::{
    Compilation, CompilationRequest, DependencyInterfaceInput, PackageInterfaceExportRequest,
    WorkerBudget,
};

use super::fixtures::{
    compilation, compilation_from_sources_for_product_with_platform_services_and_worker_budget,
    execution_bundle_dependency, execution_consumer, execution_dependency, export,
};

#[test]
fn imported_ordinary_predicate_meaning_tracks_completion_and_mutation() {
    let provider = compilation(
        r#"
        module api;

        struct Resource {
            mut open: bool;
            predicate complete(value: &Resource) = !value.open;
        }

        predicate finished(value: &Resource) = Resource.complete(value = value);
    "#,
    );

    for (body, valid) in [
        ("value.open = false;", true),
        ("value.open = true;", false),
        ("value.open = false; value.open = true;", false),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api;

            func root(pos value: &mut example.package.api.Resource)
                executes(total)
                ensures(example.package.api.finished(value = &value))
            {{
                {body}
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn imported_predicate_meanings_reject_a_callable_result_as_a_parameter() {
    let provider = compilation(
        r#"
        module api;
        predicate ready(value: bool) = value;

        func answer(pos value: bool) -> bool
            ensures(result)
        {
            return true;
        }
    "#,
    );

    let original = export(&provider);
    let source = original.semantics();

    let result = source
        .constant_terms()
        .iter()
        .position(|term| {
            matches!(term, bray_package_interface::InterfaceConstantTerm::CallableArgument(ordinal)
            if ordinal.raw() == 1)
        })
        .unwrap();

    let result =
        bray_package_interface::InterfaceConstantTermId::new(u32::try_from(result).unwrap());

    for corrupt in [false, true] {
        let predicates = source.predicate_definitions().iter().map(|predicate| {
            if corrupt {
                predicate.clone().with_condition(Some(result))
            } else {
                predicate.clone()
            }
        });

        let semantics = source.clone().with_declarations(
            source.callable_signatures().iter().cloned(),
            source.generic_declarations().iter().cloned(),
            source.callable_parameter_defaults().iter().cloned(),
            predicates,
        );

        let exported = PackageInterfaceExportBundle::try_new(
            original.surface().clone(),
            semantics,
            original.language_revision(),
            original.implementation_configuration().clone(),
        );

        assert_eq!(
            exported.is_ok(),
            !corrupt,
            "corrupt {corrupt}: {exported:?}"
        );
    }
}

#[test]
fn generic_predicate_meanings_export_const_parameters() {
    let provider = compilation(
        r#"
        module api;
        predicate enough<const minimum: usize>(value: usize) = value >= minimum;
    "#,
    );

    let original = export(&provider);

    assert!(
        original
            .semantics()
            .predicate_definitions()
            .iter()
            .any(|predicate| { predicate.condition().is_some() })
    );
}

#[test]
fn conditional_finalizer_completion_exports_with_declared_postconditions() {
    for explicit_success in ["", "ensures(result matches Ok(_))"] {
        let source = r#"
            module api;

            struct Resource
            {
                mut open: bool;
                predicate complete(value: &Resource) = !value.open;

                finalize() -> Result<unit, unit>
                    ensures(result matches Error(_) || Resource.complete(value = &self))
                    when(Resource.complete(value = &self))
                    {
                        executes(pure, total)
                        CONDITIONAL_POST
                    }
                {
                    if self.open
                    {
                        return Error(unit);
                    }

                    return Ok(unit);
                }
            }
        "#
        .replace("CONDITIONAL_POST", explicit_success);

        let provider = compilation(&source);

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{explicit_success}: {:?}",
            provider.check_diagnostics(),
        );

        let bundle = export(&provider);
        let artifact = encode_package_interface(bundle).unwrap();

        let validated = ValidatedPackageInterface::try_new(
            artifact.bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap();

        let decoded = validated.decode_semantics(bundle.surface()).unwrap();

        assert_eq!(
            decoded.callable_contracts(),
            bundle.semantics().callable_contracts()
        );

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;

            func root(pos value: example.package.api.Resource)
                requires(example.package.api.Resource.complete(value = &value))
            {
            }
        "#,
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{explicit_success}: {:?}",
            consumer.check_diagnostics(),
        );
    }
}

#[test]
fn imported_execution_domains_require_live_authority_and_ordinary_conditions() {
    let provider = compilation(
        r#"
        trusted module api;
        struct Owner { mut value: u64; }
        trusted predicate live(owner: &Owner);
        trusted func produce(pos value: u64) -> Owner
            ensures(result.value == value, trusted live(owner = &result)) executes(total)
        { return { value = value }; }
        trusted func observe(pos owner: &Owner) -> u64
            requires(trusted live(owner = owner), owner.value == 1) executes(total)
        { return owner.value; }
        func change(pos owner: &mut Owner) executes(total) { owner.value = 2; }
        "#,
    );

    for (body, valid) in [
        (
            "let owner = trusted produce(1); return trusted observe(&owner);",
            true,
        ),
        (
            "let owner = trusted produce(2); return trusted observe(&owner);",
            false,
        ),
        (
            "let owner: Owner = { value = 1 }; return trusted observe(&owner);",
            false,
        ),
        (
            "let mut owner = trusted produce(1); change(&mut owner); return trusted observe(&owner);",
            false,
        ),
    ] {
        let source = format!(
            "module app; using example.package.api; func root() -> u64 executes(total) {{ {} }}",
            body.replace("Owner", "example.package.api.Owner")
                .replace("produce(", "example.package.api.produce(")
                .replace("observe(", "example.package.api.observe(")
                .replace("change(", "example.package.api.change(")
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn execution_guarantees_reject_malformed_interface_evidence() {
    use bray_symbols::{CallableExecutionOrigin, SymbolOrdinal};

    let provider = compilation(
        r#"
            module api;

            func helper()
                executes(pure, total)
            {
            }

            func root()
                executes(pure, total)
            {
                helper();
            }
        "#,
    );

    let original = export(&provider);

    let dependency = original
        .semantics()
        .callable_contracts()
        .iter()
        .flat_map(|contract| contract.execution_contract().evidence.iter())
        .flat_map(|proof| proof.dependencies.iter())
        .find(|(_, obligation)| {
            obligation.property() == Some(bray_symbols::ExecutionProperty::Total)
        })
        .copied()
        .unwrap();

    for corruption in 0..10 {
        let contracts = original
            .semantics()
            .callable_contracts()
            .iter()
            .filter(|_| corruption != 8)
            .map(|contract| {
                if corruption == 7 {
                    return bray_package_interface::InterfaceCallableContract::new(
                        contract.owner().clone(),
                        [],
                        contract
                            .invocation_behavior()
                            .clone()
                            .with_execution_properties([]),
                        contract
                            .deferred_execution_behavior()
                            .map(|behavior| behavior.clone().with_execution_properties([])),
                    );
                }

                let mut execution = contract.execution_contract().clone();

                if !execution.evidence.is_empty() {
                    match corruption {
                        0 => execution.evidence = Arc::from([]),
                        1 => {
                            Arc::make_mut(&mut execution.domains)[0].ordinal =
                                SymbolOrdinal::new(99)
                        }
                        2 => {
                            Arc::make_mut(&mut execution.evidence)[0].origin =
                                CallableExecutionOrigin::ForeignAssertion
                        }
                        3 => {
                            Arc::make_mut(&mut execution.evidence)[0].origin =
                                CallableExecutionOrigin::Requirement
                        }
                        4 => {
                            for proof in Arc::make_mut(&mut execution.evidence) {
                                for (target, _) in Arc::make_mut(&mut proof.dependencies) {
                                    *target = bray_symbols::CallableExecutionTarget::Callable(
                                        bray_package_interface::InterfaceCallableInstanceId::new(
                                            u32::MAX,
                                        ),
                                    );
                                }
                            }
                        }
                        5 => {
                            for proof in Arc::make_mut(&mut execution.evidence) {
                                for (_, obligation) in Arc::make_mut(&mut proof.dependencies) {
                                    *obligation =
                                        bray_symbols::CallableExecutionObligation::Postcondition(
                                            SymbolOrdinal::new(99),
                                        );
                                }
                            }
                        }
                        6 => {
                            for proof in Arc::make_mut(&mut execution.evidence) {
                                if proof.obligation.property()
                                    == Some(bray_symbols::ExecutionProperty::Total)
                                {
                                    proof.dependencies = Arc::from([dependency]);
                                }
                            }
                        }
                        9 => {
                            Arc::make_mut(&mut execution.evidence)[0].origin =
                                CallableExecutionOrigin::CompilerIntrinsic;
                        }
                        _ => unreachable!(),
                    }
                }

                contract.clone().with_execution_contract(execution)
            })
            .collect::<Vec<_>>();

        let semantics = original.semantics().clone().with_contracts(
            original.semantics().constraints().iter().cloned(),
            contracts,
        );

        let malformed = PackageInterfaceExportBundle::try_new(
            original.surface().clone(),
            semantics,
            original.language_revision(),
            original.implementation_configuration().clone(),
        );

        assert!(
            malformed.is_err(),
            "corruption {corruption} must be rejected"
        );
    }
}

#[test]
fn execution_guarantees_export_only_certified_evidence() {
    for (clause, valid) in [
        ("executes(pure, total)", true),
        (
            r#"
                when(true)
                    {
                        ensures(false)
                    }
            "#,
            false,
        ),
    ] {
        let source = format!(
            r#"
                module app;

                func checked()
                    {clause}
                {{
                }}
            "#
        );

        let compilation = compilation(&source);
        let result = compilation.package_interface_export_bundle().unwrap();

        assert_eq!(result.is_ok(), valid, "{source}: {result:?}");
    }
}

#[test]
fn execution_guarantees_survive_provider_consumer_compilation() {
    let provider = compilation(
        r#"
            module api;

            func helper<T>() -> bool
                executes(total)
            {
                return true;
            }

            func guarded(pos flag: bool) -> bool
                when(flag)
                {
                    executes(pure, total)
                    ensures(result)
                }
            {
                if flag
                {
                    return true;
                }

                loop
                {
                }
            }

            func root() -> bool
                executes(total)
            {
                return helper<bool>();
            }
        "#,
    );

    let bundle = export(&provider);
    let artifact = encode_package_interface(bundle).unwrap();

    for (argument, valid) in [("true", true), ("false", false)] {
        let source = format!(
            r#"
                module app;

                using example.package.api.guarded;

                func caller() -> bool
                    executes(pure, total)
                    when(true)
                    {{
                        ensures(result)
                    }}
                {{
                    return example.package.api.guarded({argument});
                }}
            "#
        );

        let consumer = crate::test_support::compilation_with_dependencies(
            &source,
            [DependencyInterfaceInput::new(
                PackageIdentity::try_new("example.package").unwrap(),
                InterfaceProductIdentity::try_new("library").unwrap(),
                "provider.brayi",
                artifact.shared_bytes(),
                InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
            )],
        );

        assert_eq!(
            consumer.check_diagnostics().is_empty(),
            valid,
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn execution_guarantees_preserve_imported_trait_requirements() {
    let provider = compilation(
        r#"
            module api;

            trait Readable
            {
                func read(pos flag: bool) -> bool
                    when(flag)
                    {
                        executes(total)
                        ensures(result)
                    };
            }
        "#,
    );

    for (property, valid) in [("executes(total)", true), ("", false)] {
        let source = format!(
            r#"
                module app;

                using example.package.api.Readable;

                struct Value
                {{
                }}

                impl Value(example.package.api.Readable)
                {{
                    func read(pos flag: bool) -> bool
                        when(flag)
                        {{
                            {property}
                            ensures(result)
                        }}
                    {{
                        return true;
                    }}
                }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            consumer.check_diagnostics().is_empty(),
            valid,
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn execution_guarantees_preserve_selected_predicate_guards() {
    let provider = compilation(
        r#"
            module api;

            predicate ready(flag: bool) = flag;

            func guarded(pos flag: bool)
                executes(total)
                requires(ready(flag))
            {
            }
        "#,
    );

    for (negation, valid) in [("", true), ("!", false)] {
        let source = format!(
            r#"
                module app;

                using example.package.api;

                func caller(pos flag: bool)
                    requires({negation}example.package.api.ready(flag))
                    executes(total)
                {{
                    example.package.api.guarded(flag);
                }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            consumer.check_diagnostics().is_empty(),
            valid,
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn execution_guarantees_round_trip_and_reject_result_as_an_entry_guard() {
    let provider = compilation(
        r#"
            module api;

            func checked(pos flag: bool) -> bool
                when(flag)
                {
                    executes(pure, total)
                    ensures(result, flag)
                }
            {
                return true;
            }

            func required(pos flag: bool) -> bool
                requires(flag)
                executes(pure, total)
            {
                return flag;
            }

            func caller() -> bool
            {
                return required(true);
            }

            func required_components(pos flags: (bool, [bool; 2])) -> (bool, [bool; 2])
                requires(flags.0, flags.1[0])
                executes(pure, total)
                ensures(result.0, result.1[0])
            {
                return flags;
            }
        "#,
    );

    let original = export(&provider);
    let artifact = encode_package_interface(original).unwrap();

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap();

    let decoded = validated.decode_semantics(original.surface()).unwrap();

    assert_eq!(
        decoded.callable_contracts(),
        original.semantics().callable_contracts()
    );

    let contracts = decoded
        .callable_contracts()
        .iter()
        .map(|contract| {
            let mut execution = contract.execution_contract().clone();

            for domain in Arc::make_mut(&mut execution.domains) {
                if let Some((_, post)) = domain.postconditions.first() {
                    domain.entry = Arc::from([*post]);
                }
            }

            contract.clone().with_execution_contract(execution)
        })
        .collect::<Vec<_>>();

    let semantics = decoded
        .clone()
        .with_contracts(decoded.constraints().iter().cloned(), contracts);

    assert!(
        PackageInterfaceExportBundle::try_new(
            original.surface().clone(),
            semantics,
            original.language_revision(),
            original.implementation_configuration().clone()
        )
        .is_err()
    );
}

#[test]
fn execution_guarantees_retain_foreign_trust_and_caller_obligations() {
    let provider = compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        [r#"
            trusted module api;

            @link(name = "c")
            @abi(c)
            @symbol(name = "foreign_truth")
            extern trusted func asserted() -> bool
                executes(total)
                uses(foreign_call);
        "#],
        ProductKind::Library,
        [],
        WorkerBudget::default(),
        None,
        [bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c").unwrap(),
            bray_symbols::NativeLinkKind::Dynamic,
        )],
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(
        bundle
            .semantics()
            .callable_contracts()
            .iter()
            .flat_map(|contract| &*contract.execution_contract().evidence)
            .all(|proof| proof.origin == bray_symbols::CallableExecutionOrigin::ForeignAssertion)
    );

    for (trust, valid) in [("trusted ", true), ("", false)] {
        let source = format!(
            r#"
                trusted module app;

                using example.package.api.asserted;

                {trust}func caller() -> bool
                    uses(foreign_call)
                    executes(total)
                {{
                    return example.package.api.asserted();
                }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            consumer.check_diagnostics().is_empty(),
            valid,
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }

    let contracts = bundle
        .semantics()
        .callable_contracts()
        .iter()
        .map(|contract| {
            let mut execution = contract.execution_contract().clone();

            for proof in Arc::make_mut(&mut execution.evidence) {
                proof.origin = bray_symbols::CallableExecutionOrigin::CompilerIntrinsic;
            }

            contract.clone().with_execution_contract(execution)
        });

    let forged = PackageInterfaceExportBundle::try_new(
        bundle.surface().clone(),
        bundle
            .semantics()
            .clone()
            .with_contracts(bundle.semantics().constraints().iter().cloned(), contracts),
        bundle.language_revision(),
        bundle.implementation_configuration().clone(),
    )
    .unwrap();

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
            trusted module app;

            using example.package.api.asserted;

            trusted func caller() -> bool
                uses(foreign_call)
                executes(total)
            {
                return example.package.api.asserted();
            }
        "#,
        [execution_bundle_dependency(&forged)],
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        consumer.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn execution_guarantees_follow_transitive_provider_dependencies() {
    let provider = compilation(
        r#"
            module api;

            func checked()
                executes(pure, total)
            {
            }
        "#,
    );

    let package = PackageIdentity::try_new("example.wrapper").unwrap();

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        InterfaceProductIdentity::try_new("library").unwrap(),
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .unwrap();

    let source = r#"
        module api;

        using example.package.api;

        func wrapped()
            executes(pure, total)
        {
            example.package.api.checked();
        }
    "#;

    let request = CompilationRequest::new(package, test_source_inputs("wrapper", [source]))
        .with_dependency_interfaces([execution_dependency(&provider)])
        .with_package_interface_export(PackageInterfaceExportRequest::new(
            identity,
            InterfaceLanguageRevision::new(0),
        ));

    let wrapper = Compilation::load(request).unwrap();

    assert!(
        wrapper.check_diagnostics().is_empty(),
        "{:?}",
        wrapper.check_diagnostics()
    );

    for include_provider in [true, false] {
        let mut dependencies = vec![execution_dependency(&wrapper)];

        if include_provider {
            dependencies.push(execution_dependency(&provider));
        }

        let consumer = crate::test_support::compilation_with_dependencies(
            r#"
                module app;

                using example.wrapper.api;

                func caller()
                    executes(pure, total)
                {
                    example.wrapper.api.wrapped();
                }
            "#,
            dependencies,
        );

        assert_eq!(
            consumer.check_diagnostics().is_empty(),
            include_provider,
            "{:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn imported_trusted_contracts_preserve_requirements_and_live_witnesses() {
    let provider = compilation(
        r#"
        trusted module api;
        public struct Owner { public mut epoch: u64; }
        @copy public struct NegativeOwner { public epoch: u64; }
        public trusted predicate dangerous(owner: &NegativeOwner);
        public trusted func negative_owner() -> NegativeOwner
            ensures(!(trusted dangerous(&result))) { return { epoch = 1 }; }
        public trusted predicate live(owner: &Owner);
        public trusted func owner() -> Owner
            ensures(trusted live(&result)) { return { epoch = 1 }; }
        public trusted func conditional_owner(pos ready: bool) -> Owner
            when(ready) { ensures(trusted live(&result)) } { return { epoch = 1 }; }
        public trusted func observe(pos value: &Owner)
            requires(trusted live(value)) {}
        public callable Observer = func(pos value: &Owner)
            requires(trusted live(value));
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);
    let artifact = encode_package_interface(bundle).unwrap();

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap();

    let decoded = validated.decode_semantics(bundle.surface()).unwrap();

    assert_eq!(&decoded, bundle.semantics());

    for (body, valid) in [
        (
            "let value = example.package.api.negative_owner(); let copied = value;",
            false,
        ),
        (
            "let value = example.package.api.owner(); example.package.api.observe(&value);",
            true,
        ),
        (
            "let value = example.package.api.conditional_owner(true); example.package.api.observe(&value);",
            true,
        ),
        (
            "let value = example.package.api.conditional_owner(false); example.package.api.observe(&value);",
            false,
        ),
        (
            "let value = example.package.api.owner(); let moved = value; example.package.api.observe(&moved);",
            true,
        ),
        (
            "let value: example.package.api.Owner = { epoch = 1 }; example.package.api.observe(&value);",
            false,
        ),
        (
            "let mut value = example.package.api.owner(); value.epoch = 2; example.package.api.observe(&value);",
            false,
        ),
        (
            "let erased: func(pos value: &example.package.api.Owner) = example.package.api.observe;",
            false,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            trusted module app;
            using example.package.api;
            func caller() {{ {body} }}
        "#
            ),
        );

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn imported_mixed_predicate_conditions_preserve_trusted_occurrences() {
    let provider = compilation(
        r#"
        trusted module api;
        public trusted predicate live(value: u64);
        public trusted predicate ready(value: u64);
        public trusted predicate live_bool(value: &bool);
        public trusted predicate live_byte(value: &u8);
        public func observe_array(pos value: &[u8; 1]) requires(trusted live_byte(&value[0])) {}
        public func observe_component(pos value: &(bool, bool)) requires(trusted live_bool(&value.0)) {}
        public func observe(pos value: u64, pos flag: bool)
            requires(((trusted live(value)) || flag)) {}
        public func observe_guarded(pos value: u64)
            requires(ready(value) || trusted live(value)) {}
        public trusted func make(pos value: u64) -> u64
            when(ready(value)) { ensures(trusted live(result)) } { return value; }
        public trusted predicate borrow_ready(value: &u8);
        public trusted func make_borrowed(pos value: &u8) -> u64
            when(borrow_ready(value)) { ensures(trusted live(result)) } { return 0; }
        public trusted func make_borrowed_pure(pos value: &u8) -> u64 executes(pure, total)
            when(borrow_ready(value)) { ensures(trusted live(result)) } { return 0; }
        public func observe_live(pos value: u64) requires(trusted live(value)) {}
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);
    let artifact = encode_package_interface(bundle).unwrap();

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .unwrap();

    assert_eq!(
        &validated.decode_semantics(bundle.surface()).unwrap(),
        bundle.semantics()
    );

    for (precondition, body, valid) in [
        ("requires(live(value))", "observe(value, flag);", false),
        (
            "requires(trusted live(value))",
            "observe(value, flag);",
            true,
        ),
        ("requires(flag)", "observe(value, flag);", true),
        ("requires(ready(value))", "observe_guarded(value);", true),
        ("requires(live(value))", "observe_guarded(value);", false),
        ("requires(ready(value))", "observe_live(make(value));", true),
        ("", "observe_live(make(value));", false),
    ] {
        let precondition = precondition
            .replace("live(", "example.package.api.live(")
            .replace("ready(", "example.package.api.ready(");

        let body = body
            .replace("observe(", "example.package.api.observe(")
            .replace("observe_guarded(", "example.package.api.observe_guarded(")
            .replace("observe_live(", "example.package.api.observe_live(")
            .replace("make(", "example.package.api.make(");

        let source = format!(
            r#"
            trusted module app;
            using example.package.api;
            func caller(pos value: u64, pos flag: bool) {precondition} {{ {body} }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{source}: {:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    for (body, valid) in [
        (
            "example.package.api.observe_live(example.package.api.make_borrowed(&value));",
            true,
        ),
        (
            "example.package.api.observe_live(example.package.api.make_borrowed_pure(&value));",
            true,
        ),
        (
            "value = 1; example.package.api.observe_live(example.package.api.make_borrowed_pure(&value));",
            false,
        ),
        (
            "value = 1; example.package.api.observe_live(example.package.api.make_borrowed(&value));",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using example.package.api;
            func caller(pos mut value: u8) requires(example.package.api.borrow_ready(&value)) {{ {body} }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{source}: {:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    for (precondition, valid) in [
        (
            "requires(trusted example.package.api.live_bool(&value.0))",
            true,
        ),
        ("requires(example.package.api.live_bool(&value.0))", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using example.package.api;
            func caller(pos value: &(bool, bool)) {precondition} {{ example.package.api.observe_component(value); }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{source}: {:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    for (precondition, valid) in [
        (
            "requires(trusted example.package.api.live_byte(&value[0]))",
            true,
        ),
        ("requires(example.package.api.live_byte(&value[0]))", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using example.package.api;
            func caller(pos value: &[u8; 1]) {precondition} {{ example.package.api.observe_array(value); }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{source}: {:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn nullable_pattern_contracts_survive_interface_transport() {
    for (parameter, argument) in [("u32?", "value"), ("&(u32?)", "&value")] {
        for pattern in ["none", "?_"] {
            let provider = compilation(&format!(
                r#"
        module api;
        func selected(pos value: {parameter}) requires(value matches {pattern}) executes(total) {{}}
    "#,
            ));

            assert!(
                !provider.check_diagnostics().has_errors(),
                "{:?}",
                provider.check_diagnostics()
            );

            for (value, valid) in [("none", pattern == "none"), ("1", pattern == "?_")] {
                let source = format!(
                    "module app; using example.package.api; func root() executes(total) {{ let value: u32? = {value}; example.package.api.selected({argument}); }}"
                );

                let consumer = execution_consumer(&provider, &source);

                assert_eq!(
                    !consumer.check_diagnostics().has_errors(),
                    valid,
                    "{source}: {:?}",
                    consumer.check_diagnostics()
                );
            }
        }
    }
}

#[test]
fn imported_cleanup_guarantees_reach_following_calls() {
    let provider = compilation(
        r#"
        trusted module api;
        trusted predicate ready();
        struct Producer {
            tag: u8;
            trusted destruct() ensures(trusted ready()) executes(total) {}
        }
        func observe() requires(trusted ready()) executes(pure, total) {}
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (interference, valid) in [
        ("", true),
        ("{ let other: Interference = { tag = 0 }; }", true),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            struct Interference {{ tag: u8; destruct() executes(total) {{}} }}
            func caller() {{
                {{ let value: example.package.api.Producer = {{ tag = 0 }}; }}
                {interference}
                example.package.api.observe();
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
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
fn imported_cleanup_postconditions_follow_the_updated_borrowed_storage() {
    let provider = compilation(
        r#"
        trusted module api;
        struct Owner { mut epoch: u64; }
        trusted predicate live(value: &Owner);
        struct Writer {
            slot: &mut Owner;
            trusted destruct() ensures(trusted live(&self.slot)) { self.slot.epoch = 1; }
        }
        func observe(pos owner: &Owner) requires(trusted live(owner)) {}
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (mutation, observed, valid) in [
        ("", "first", true),
        ("", "second", false),
        ("first.epoch = 2;", "first", false),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            func caller() {{
                let mut first: example.package.api.Owner = {{ epoch = 0 }};
                let second: example.package.api.Owner = {{ epoch = 0 }};
                {{ let writer: example.package.api.Writer = {{ slot = &mut first }}; }}
                {mutation}
                example.package.api.observe(&{observed});
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
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
fn imported_union_variant_contracts_preserve_checked_tags() {
    let provider = compilation(
        r#"
        module api;
        func create() -> Result<u32, u32> ensures(result matches Ok(_)) executes(total) { return Ok(7); }
        const func observe(pos value: &Result<u32, u32>) -> u32
            requires(value matches Ok(_)) executes(pure, total)
        {
            match value {
                case Ok(_) { return 7; }
                case Error(_) { panic("unreachable"); }
            }
        }
    "#,
    );

    for (body, valid) in [
        (
            "let value = example.package.api.create(); let _ = example.package.api.observe(&value);",
            true,
        ),
        (
            "let value: Result<u32, u32> = Error(7); let _ = example.package.api.observe(&value);",
            false,
        ),
    ] {
        let source = format!(
            "module app; using example.package.api; func root() executes(total) {{ {body} }}"
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn imported_forwarded_carrier_preserves_its_original_boundary() {
    let provider = compilation(
        r#"
        trusted module api;
        struct Marker {}
        struct Guard { marker: Marker; }
        struct Wrapper { guard: Guard; }
        trusted predicate held(marker: &Marker);
        trusted func acquire() -> Guard
            ensures(trusted held(&result.marker)) executes(pure, total)
        { return { marker = Marker {} }; }
        func wrap() -> Wrapper
            ensures(trusted held(&result.guard.marker)) executes(pure, total)
        { return { guard = acquire() }; }
        func observe(pos guard: &Guard)
            requires(trusted held(&guard.marker)) executes(pure, total) {}
    "#,
    );

    for (body, valid) in [
        (
            "let wrapper = example.package.api.wrap(); let guard = wrapper.guard; example.package.api.observe(&guard);",
            true,
        ),
        (
            "let marker = example.package.api.wrap().guard.marker;",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            func root() {{ {body} }}
        "#
        );

        let application = execution_consumer(&provider, &source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );
    }
}

#[test]
fn imported_static_owner_retains_initializer_authority() {
    let provider = compilation(
        r#"
        trusted module api;
        struct Marker {}
        struct Owner { marker: Marker; }
        trusted predicate held(marker: &Marker);
        trusted const func create() -> Owner
            ensures(trusted held(&result.marker)) executes(pure, total)
        { return { marker = Marker {} }; }
        static OWNER: Owner = create();
        extern func opaque();
        func observe(pos owner: &Owner) requires(trusted held(&owner.marker)) {}
    "#,
    );

    for (body, valid) in [
        (
            "example.package.api.observe(&example.package.api.OWNER); example.package.api.opaque(); example.package.api.observe(&example.package.api.OWNER);",
            true,
        ),
        (
            "let owner = example.package.api.Owner { marker = example.package.api.Marker {} }; example.package.api.observe(&owner);",
            false,
        ),
    ] {
        let source = format!("module app; using example.package.api; func root() {{ {body} }}");
        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{body}: {:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn imported_total_cleanup_certification_preserves_the_entry_domain() {
    for (guarantee, body, total) in [
        ("executes(total)", "", true),
        (
            "when(self.ready) { executes(total) }",
            "if !self.ready { panic(\"not ready\"); }",
            false,
        ),
    ] {
        let declarations = format!(
            r#"
            trusted module api;
            trusted predicate live<T>(owner: &Owner<T>);
            struct Owner<T> {{
                value: bool;
                ready: bool;
                trusted destruct() requires(trusted live<T>(owner = &self)) {guarantee} {{ {body} }}
            }}
            func dispose(pos owner: Owner<bool>) requires(trusted live<bool>(owner = &owner)) {{}}
        "#
        );

        let provider = compilation(&declarations);

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{declarations}: {:#?}",
            provider.check_diagnostics()
        );

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;
            func dispose(pos owner: example.package.api.Owner<bool>)
                requires(trusted example.package.api.live<bool>(owner = &owner)) {}
            func forged() { let owner: example.package.api.Owner<bool> = { value = false, ready = true }; }
        "#,
        );

        for application in [&provider, &consumer] {
            let key = crate::test_support::source_function_body_key(application, "dispose");
            let bound = application.bound_unit(key).unwrap();

            assert!(!bound.diagnostics().has_errors(), "{bound:#?}");

            let lowered = application
                .lowered_unit(crate::test_support::source_function_body_key(
                    application,
                    "dispose",
                ))
                .unwrap();

            assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

            let ty = lowered
                .value()
                .as_ref()
                .unwrap()
                .mir()
                .unwrap()
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                .unwrap()
                .ty();

            let cancellation = crate::CancellationToken::new();

            let selected = application
                .selected_lifecycle_signature(
                    ty,
                    bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
                    &cancellation,
                )
                .unwrap();

            let (callable, _) = selected.value().as_ref().unwrap();

            assert_eq!(
                application
                    .certified_entry_property(
                        *callable,
                        bray_checker::ExecutionProperty::Total,
                        &cancellation
                    )
                    .unwrap(),
                total
            );

            assert!(
                application
                    .verified_execution_obligation(
                        *callable,
                        bray_checker::ExecutionObligation::Property(
                            bray_checker::ExecutionProperty::Total,
                            None
                        ),
                        None,
                        &cancellation
                    )
                    .unwrap()
                    .is_none(),
                "a capacity certificate must not authorize a caller"
            );
        }

        bray_testing::assert_goal_state_diagnostic_kind(
            consumer.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn imported_predicate_iteration_reuses_checked_producer_bodies_and_finite_evidence() {
    let provider = compilation(&crate::test_support::finite_iterator_source(
        "self.remaining -= 1; return true;",
    ));

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(!bundle.constant_callable_bodies().is_empty());

    let consumer = crate::test_support::compilation_with_dependencies(
        r#"
        module app;
        using example.package.api;

        func caller(pos value: example.package.api.Items)
            requires(example.package.api.every(value = value))
        {
        }
        "#,
        [super::fixtures::implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn imported_const_trait_calls_retain_their_declared_generic_premises() {
    let provider = compilation(
        r#"
        module api;

        trait Readable
        {
            const func read() -> bool executes(pure, total);
        }

        const func forward<T>(pos value: &T) -> bool with(T: Readable)
        {
            return value.read();
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        struct Flag
        {
        }

        impl Flag(example.package.api.Readable)
        {
            const func read() -> bool executes(pure, total)
            {
                return true;
            }
        }

        func main(pos value: Flag)
            requires(example.package.api.forward<Flag>(&value))
        {
        }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn imported_checked_edges_retain_the_producers_selected_entry_domain() {
    let provider = compilation(
        r"
        trusted module api;

        struct Token
        {
        }

        trusted predicate admitted(value: &Token);

        trusted func leaf(pos value: &Token)
            requires(trusted admitted(value))
            executes(total)
        {
        }

        trusted func wrapper(pos value: &Token)
            requires(trusted admitted(value))
            executes(total)
        {
            leaf(value);
        }
    ",
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    for (entry, valid) in [
        (
            "requires(trusted example.package.api.admitted(value))",
            true,
        ),
        ("", false),
    ] {
        let source = format!(
            r"
            trusted module app;

            using example.package.api;

            trusted func caller(pos value: &example.package.api.Token)
                {entry}
                executes(total)
            {{
                example.package.api.wrapper(value);
            }}
        "
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            consumer.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_checked_edges_preserve_the_selected_conditional_property() {
    let provider = compilation(
        r"
        module api;

        func leaf(pos ready: bool)
            when(ready)
            {
                executes(total)
            }
        {
            if !ready
            {
                assert(false);
            }
        }

        func wrapper(pos ready: bool)
            when(ready)
            {
                executes(total)
            }
        {
            leaf(ready);
        }
    ",
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    for (value, valid) in [("true", true), ("false", false)] {
        let source = format!(
            r"
            module app;

            using example.package.api;

            func caller()
                executes(total)
            {{
                example.package.api.wrapper({value});
            }}
        "
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{source}: {:#?}",
            consumer.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                consumer.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn checked_total_calls_support_imported_static_termination() {
    let declarations = r"
        trusted module api;

        using core.memory;

        struct Marker
        {
        }

        struct Owner
        {
            marker: Marker;
            value: u32;
        }

        const func prepared() -> (Marker, u32)
        {
            return (
                Marker
                {
                },
                7,
            );
        }
    ";

    let provider = compilation(declarations);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let source = compilation(&format!(
        r"
        {declarations}

        static STORAGE: Uninit<Owner> =
        {{
            let value = prepared();

            yield core.memory.uninit_from<Owner>(
                {{
                    marker = value.0,
                    value = value.1,
                }}
            );
        }};
    "
    ));

    assert!(
        !source.check_diagnostics().has_errors(),
        "{:#?}",
        source.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r"
        trusted module app;

        using core.memory;
        using example.package.api;

        static STORAGE: Uninit<example.package.api.Owner> =
        {
            let value = example.package.api.prepared();

            yield core.memory.uninit_from<example.package.api.Owner>(
                {
                    marker = value.0,
                    value = value.1,
                }
            );
        };
    ",
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );
}
