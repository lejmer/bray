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
fn disjoint_raw_slot_moves_still_invalidate_opaque_reachable_observations() {
    let provider = compilation(
        r"
        trusted module api;
        using core.memory;
        struct Header { value: usize; }
        trusted predicate observed(pointer: DataPointer<Header>);
        trusted func inspect(pos pointer: DataPointer<Header>)
            requires(trusted observed(pointer = pointer)) {}
        trusted func read_both(pos primary: DataPointer<Header>, pos incident: DataPointer<Header>) -> usize
            requires(
                trusted observed(pointer = incident),
                trusted core.memory.valid_read<Header>(pointer = primary, count = 1),
                trusted core.memory.aligned_for<Header>(pointer = primary),
                trusted core.memory.initialized_as<Header>(pointer = primary),
                trusted core.memory.valid_read<Header>(pointer = incident, count = 1),
                trusted core.memory.aligned_for<Header>(pointer = incident),
                trusted core.memory.initialized_as<Header>(pointer = incident),
                trusted core.memory.non_overlapping<Header>(left = primary, left_count = 1, right = incident, right_count = 1),
            )
            uses(raw_memory)
        {
            let first = trusted core.memory.read<Header>(primary);
            trusted inspect(incident);
            let second = trusted core.memory.read<Header>(incident);
            return first.value + second.value;
        }
    ",
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        provider.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn raw_slot_moves_preserve_only_live_disjoint_initialization() {
    for (non_overlapping, second, valid) in [
        (
            "trusted core.memory.non_overlapping<Header>(left = primary, left_count = 1, right = incident, right_count = 1),",
            "incident",
            true,
        ),
        (
            "trusted core.memory.non_overlapping<Header>(left = incident, left_count = 1, right = primary, right_count = 1),",
            "incident",
            true,
        ),
        ("", "incident", false),
        (
            "trusted core.memory.non_overlapping<Header>(left = primary, left_count = 0, right = incident, right_count = 1),",
            "incident",
            false,
        ),
        (
            "trusted core.memory.non_overlapping<Header>(left = primary, left_count = 1, right = incident, right_count = 1),",
            "primary",
            false,
        ),
    ] {
        let provider = compilation(&format!(
            r"
            trusted module api;
            using core.memory;
            public struct Header {{ value: usize; }}
            public trusted func read_both(pos primary: DataPointer<Header>, pos incident: DataPointer<Header>) -> usize
                requires(
                    trusted core.memory.valid_read<Header>(pointer = primary, count = 1),
                    trusted core.memory.aligned_for<Header>(pointer = primary),
                    trusted core.memory.initialized_as<Header>(pointer = primary),
                    trusted core.memory.valid_read<Header>(pointer = incident, count = 1),
                    trusted core.memory.aligned_for<Header>(pointer = incident),
                    trusted core.memory.initialized_as<Header>(pointer = incident),
                    {non_overlapping}
                )
                uses(raw_memory)
            {{
                let first = trusted core.memory.read<Header>(primary);
                let second = trusted core.memory.read<Header>({second});
                return first.value + second.value;
            }}
        "
        ));

        let diagnostics = provider.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{non_overlapping} {second}: {diagnostics:?}"
        );

        if valid {
            let consumer_source = r"
                trusted module app;
                using core.memory;
                using example.package.api;
                trusted func forward(pos primary: DataPointer<example.package.api.Header>, pos incident: DataPointer<example.package.api.Header>) -> usize
                    requires(
                        trusted core.memory.valid_read<example.package.api.Header>(pointer = primary, count = 1),
                        trusted core.memory.aligned_for<example.package.api.Header>(pointer = primary),
                        trusted core.memory.initialized_as<example.package.api.Header>(pointer = primary),
                        trusted core.memory.valid_read<example.package.api.Header>(pointer = incident, count = 1),
                        trusted core.memory.aligned_for<example.package.api.Header>(pointer = incident),
                        trusted core.memory.initialized_as<example.package.api.Header>(pointer = incident),
                        CONTRACT
                    )
                { return trusted example.package.api.read_both(primary, incident); }
            "
            .replace(
                "CONTRACT",
                &non_overlapping.replace("<Header>", "<example.package.api.Header>"),
            );

            let consumer = execution_consumer(&provider, &consumer_source);

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{non_overlapping}: {:?}",
                consumer.check_diagnostics()
            );
        }
    }
}

#[test]
fn spare_capacity_proofs_keep_contract_only_scalar_input_types() {
    let provider = compilation(
        r"
        module api;
        public func accepted(pos initialized: usize, pos count: usize, pos capacity: usize) -> usize
            requires(initialized <= capacity, count <= capacity - initialized)
            ensures(result <= capacity)
            executes(pure, total)
        { return initialized + count; }
    ",
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r"
        module app;
        using example.package.api;
        func forward(pos initialized: usize, pos count: usize, pos capacity: usize) -> usize
            requires(initialized <= capacity, count <= capacity - initialized)
            ensures(result <= capacity)
            executes(pure, total)
        { return example.package.api.accepted(initialized, count, capacity); }
    ",
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );
}

#[test]
fn relational_slice_bounds_use_live_count_facts_in_source_and_imports() {
    for (requirement, mutation, valid) in [
        ("count > 0, count <= values.length()", "", true),
        ("count >= 0, count <= values.length()", "", false),
        (
            "count > 0, count <= values.length()",
            "values = &empty[..];",
            false,
        ),
    ] {
        let provider = compilation(&format!(
            r"
            module api;
            public func first(pos mut values: &[u8], pos count: usize) -> u8
                requires({requirement})
                executes(pure, total)
            {{
                let empty: [u8; 0] = [0; 0];
                {mutation}
                return values[0];
            }}
        "
        ));

        let diagnostics = provider.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{mutation}: {diagnostics:?}"
        );

        if valid {
            let consumer = execution_consumer(
                &provider,
                r"
                module app;
                using example.package.api;
                func forward(pos values: &[u8], pos count: usize) -> u8
                    requires(count > 0, count <= values.length())
                    executes(pure, total)
                { return example.package.api.first(values, count); }
            ",
            );

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );
        }
    }
}

#[test]
fn integer_precondition_proofs_use_current_entry_values() {
    for (mutation, valid) in [("", true), ("selection.address = 0;", false)] {
        let provider = compilation(&format!(
            r"
            module api;
            struct Selection {{ mut address: usize; }}
            func selected(pos value: usize) -> Selection
                requires(value > 0)
                ensures(result.address > 0)
                executes(pure, total)
            {{ return {{ address = value }}; }}
            func read(pos selection: &Selection) -> usize
                requires(selection.address > 1)
                ensures(result == selection.address)
                executes(pure, total)
            {{ return selection.address; }}
            public func narrow(pos value: usize) -> usize
                requires(value > 0)
                ensures(result > 1)
                executes(pure, total)
            {{
                let mut selection = selected(value);
                if selection.address == 1 {{ return 2; }}
                {mutation}
                return read(&selection);
            }}
        "
        ));

        let diagnostics = provider.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{mutation}: {diagnostics:?}"
        );

        if valid {
            let consumer = execution_consumer(
                &provider,
                r"
                module app;
                using example.package.api;
                func forward(pos value: usize) -> usize
                    requires(value > 0)
                    ensures(result > 1)
                    executes(pure, total)
                { return example.package.api.narrow(value); }
            ",
            );

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn positive_scalar_and_field_bounds_survive_excluded_endpoints() {
    for projection in ["value", "selection.address"] {
        let provider = compilation(&format!(
            r"
            module api;
            struct Selection {{ address: usize; }}
            func selected(pos value: usize) -> Selection
                requires(value > 0)
                ensures(result.address > 0)
                executes(pure, total)
            {{ return {{ address = value }}; }}
            public func narrow(pos value: usize) -> usize
                requires(value > 0)
                ensures(result > 1)
                executes(pure, total)
            {{
                let selection = selected(value);
                if {projection} == 1 {{ return 2; }}
                return {projection};
            }}
        "
        ));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{:?}",
            provider.check_diagnostics()
        );

        let consumer = execution_consumer(
            &provider,
            r"
            module app;
            using example.package.api;
            func forward(pos value: usize) -> usize
                requires(value > 0)
                ensures(result > 1)
                executes(pure, total)
            { return example.package.api.narrow(value); }
        ",
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:?}",
            consumer.check_diagnostics()
        );
    }
}

#[test]
fn strong_compare_exchange_preserves_its_observed_value_relationship() {
    for (operation, scalar, initial, mutation, valid) in [
        ("compare_exchange", "usize", 0, "", true),
        ("compare_exchange", "usize", 0, "expected = 1;", true),
        (
            "compare_exchange",
            "usize",
            0,
            "core.atomic.store<usize, 0>(binding, 0);",
            true,
        ),
        ("compare_exchange", "usize", 1, "expected = 0;", false),
        ("compare_exchange_weak", "usize", 0, "", false),
        ("compare_exchange", "isize", 0, "", false),
    ] {
        let source = format!(
            r"
            module api;
            using core.atomic;
            public func select(pos binding: &core.atomic.Atomic<{scalar}>) -> {scalar}
                ensures(result > 0)
                executes(total)
            {{
                let mut expected: {scalar} = {initial};
                let (previous, acquired) = core.atomic.{operation}<{scalar}, 3, 1>(binding, expected = expected, desired = 1);
                {mutation}
                if acquired {{ return 1; }}
                return previous;
            }}
        "
        );

        let provider = compilation(&source);
        let diagnostics = provider.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:?}"
        );

        if valid {
            let consumer = execution_consumer(
                &provider,
                r"
                module app;
                using core.atomic;
                using example.package.api;
                func forward(pos binding: &core.atomic.Atomic<usize>) -> usize
                    ensures(result > 0)
                    executes(total)
                { return example.package.api.select(binding); }
            ",
            );

            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn product_static_access_is_total_without_becoming_pure() {
    for (duration, property, valid) in [
        ("", "total", true),
        ("", "pure", false),
        ("@thread_local", "total", false),
        ("@thread_local", "pure", false),
    ] {
        for access in ["VALUE", "GENERIC_VALUE<7>", "NESTED.value"] {
            let source = format!(
                r"
                module api;
                struct Nested {{ value: u32; }}
                {duration} static VALUE: u32 = 7;
                {duration} static GENERIC_VALUE<const N: u32>: u32 = N;
                {duration} static NESTED: Nested = {{ value = 7 }};

                func read() -> u32 executes({property}) {{ return {access}; }}
                "
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }
}

#[test]
fn imported_product_static_access_preserves_total_execution() {
    for (duration, valid) in [("", true), ("@thread_local", false)] {
        let provider = compilation(&format!(
            "module api; {duration} static VALUE<const N: u32>: u32 = N;"
        ));

        let consumer = execution_consumer(
            &provider,
            r"
            module app;
            using example.package.api;
            func read() -> u32 executes(total) { return example.package.api.VALUE<7>; }
            ",
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{duration}: {diagnostics:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

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
        .expect("fixture sequence must contain the selected entry");

    let result = bray_package_interface::InterfaceConstantTermId::new(
        u32::try_from(result).expect("fixture value must fit the requested representation"),
    );

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

        let artifact =
            encode_package_interface(bundle).expect("fixture package interface must encode");

        let validated = ValidatedPackageInterface::try_new(
            artifact.bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .expect("fixture validated package interface must be valid");

        let decoded = validated
            .decode_semantics(bundle.surface())
            .expect("fixture interface semantics must decode");

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
        .expect("fixture `copied` call must succeed");

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

        let result = compilation
            .package_interface_export_bundle()
            .expect("fixture `package_interface_export_bundle` call must succeed");

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
    let artifact = encode_package_interface(bundle).expect("fixture package interface must encode");

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
                PackageIdentity::try_new("example.package")
                    .expect("fixture package identity must be valid"),
                InterfaceProductIdentity::try_new("library")
                    .expect("fixture interface product identity must be valid"),
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

    let artifact =
        encode_package_interface(original).expect("fixture package interface must encode");

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .expect("fixture validated package interface must be valid");

    let decoded = validated
        .decode_semantics(original.surface())
        .expect("fixture interface semantics must decode");

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

            @link(name = "c")
            @symbol(name = "foreign_value")
            extern trusted static VALUE: i32;
        "#],
        ProductKind::Library,
        [],
        WorkerBudget::default(),
        None,
        [bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c")
                .expect("fixture non empty shared str must be valid"),
            bray_symbols::NativeLinkKind::Dynamic,
        )],
    );

    assert!(
        provider.check_diagnostics().is_empty(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert_eq!(bundle.native_boundaries().len(), 2);

    for boundary in bundle.native_boundaries() {
        assert_eq!(boundary.links().len(), 1);
        assert_eq!(boundary.links()[0].name(), "c");

        assert_eq!(
            boundary.links()[0].kind(),
            bray_symbols::NativeLinkKind::Dynamic
        );
    }

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
    .expect("fixture package interface export bundle must be valid");

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

    let package = PackageIdentity::try_new("example.wrapper")
        .expect("fixture package identity must be valid");

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        InterfaceProductIdentity::try_new("library")
            .expect("fixture interface product identity must be valid"),
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .expect("fixture package interface identity must be valid");

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

    let wrapper = Compilation::load(request).expect("fixture input must load");

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
    let artifact = encode_package_interface(bundle).expect("fixture package interface must encode");

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .expect("fixture validated package interface must be valid");

    let decoded = validated
        .decode_semantics(bundle.surface())
        .expect("fixture interface semantics must decode");

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
    let artifact = encode_package_interface(bundle).expect("fixture package interface must encode");

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .expect("fixture validated package interface must be valid");

    assert_eq!(
        &validated
            .decode_semantics(bundle.surface())
            .expect("fixture exported semantics must decode"),
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
            "let value = example.package.api.create(); _ = example.package.api.observe(&value);",
            true,
        ),
        (
            "let value: Result<u32, u32> = Error(7); _ = example.package.api.observe(&value);",
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

            let bound = application
                .bound_unit(key)
                .expect("fixture callable must bind successfully");

            assert!(!bound.diagnostics().has_errors(), "{bound:#?}");

            let lowered = application
                .lowered_unit(crate::test_support::source_function_body_key(
                    application,
                    "dispose",
                ))
                .expect("fixture callable must lower successfully");

            assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");

            let ty = lowered
                .value()
                .as_ref()
                .expect("fixture optional value must be present")
                .mir()
                .expect("fixture instance must retain its MIR")
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                .expect("fixture lookup must find its expected value")
                .ty();

            let cancellation = crate::CancellationToken::new();

            let selected = application
                .selected_lifecycle_signature(
                    ty,
                    bray_symbols::TypeAssociatedLifecycleSlot::Destructor,
                    &cancellation,
                )
                .expect("fixture `selected_lifecycle_signature` call must succeed");

            let (callable, _) = selected
                .value()
                .as_ref()
                .expect("fixture optional value must be present");

            assert_eq!(
                application
                    .certified_entry_property(
                        *callable,
                        bray_checker::ExecutionProperty::Total,
                        &cancellation
                    )
                    .expect("fixture `certified_entry_property` call must succeed"),
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
                    .expect("fixture `verified_execution_obligation` call must succeed")
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

#[test]
fn guarded_callable_types_preserve_generic_inputs_across_interfaces() {
    let provider = compilation(
        r#"
        module api;
        callable Action<T> = func(pos flag: bool, pos value: T) -> T
            when(flag) { executes(pure) };
        func supplied<T>(pos flag: bool, pos value: T) -> T
            when(flag) { executes(pure) }
        { return value; }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let original = export(&provider);
    let artifact = encode_package_interface(&original).expect("guarded callable interface encodes");

    let validated = ValidatedPackageInterface::try_new(
        artifact.bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
    .expect("guarded callable interface validates");

    let decoded = validated
        .decode_semantics(original.surface())
        .expect("guarded callable semantics decode");

    assert_eq!(decoded.types(), original.semantics().types());

    for (argument, valid) in [("true", true), ("false", false), ("flag", false)] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            func root(pos flag: bool, pos action: example.package.api.Action<bool>) -> bool
                executes(pure)
            {{ return action({argument}, true); }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        assert_eq!(
            !consumer.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            consumer.check_diagnostics()
        );
    }

    for (argument, valid) in [("true", true), ("false", false), ("flag", false)] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            func root(pos flag: bool) -> bool executes(pure) {{
                let action: example.package.api.Action<bool> = example.package.api.supplied<bool>;
                return action({argument}, true);
            }}
        "#
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

#[test]
fn imported_guarded_callables_cannot_gain_execution_evidence_from_raw_addresses() {
    let provider = compilation(
        r#"
        module api;
        callable Callback = @abi(c) func(pos flag: bool) -> bool
            when(true) { executes(pure, total) };
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        trusted module app;
        using core.memory;
        using example.package.api;
        trusted func reconstruct(pos pointer: FunctionPointer<example.package.api.Callback>)
            -> example.package.api.Callback
            uses(layout_reinterpret)
            requires(trusted core.memory.callable_address_valid<example.package.api.Callback>(pointer = pointer))
        { return trusted core.memory.callable_from_pointer<example.package.api.Callback>(pointer); }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        consumer.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn imported_callable_postconditions_can_weaken_but_cannot_strengthen() {
    let provider = compilation(
        r#"
        module api;
        callable Unconditional = func(pos flag: bool) -> bool
            executes(pure) ensures(result);
        callable Guarded = func(pos flag: bool) -> bool
            when(flag) { executes(pure) ensures(result) };
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (source_type, target_type, valid) in [
        ("Unconditional", "Guarded", true),
        ("Guarded", "Unconditional", false),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api;
            func convert(pos action: example.package.api.{source_type})
                -> example.package.api.{target_type}
            {{ return action as example.package.api.{target_type}; }}
        "#,
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

#[test]
fn imported_callable_conversion_keeps_entry_guards_separate_from_exit_facts() {
    for (parameter, observation) in [
        ("mut flag: bool", "flag"),
        ("flag: &mut Flag", "flag.value"),
    ] {
        let provider = compilation(&format!(
            r#"
            module api;
            struct Flag
            {{
                mut value: bool;
            }}
            callable Source = func(pos {parameter}) -> bool ensures(!{observation});
            callable Target = func(pos {parameter}) -> bool
                when({observation}) {{ ensures(result) }};
            func change(pos {parameter}) -> bool ensures(!{observation})
            {{
                {observation} = false;
                return false;
            }}
        "#
        ));

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{parameter}: {:?}",
            provider.check_diagnostics()
        );

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;
            func convert(pos action: example.package.api.Source)
                -> example.package.api.Target
            {
                return action as example.package.api.Target;
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            consumer.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingNoApplicableCandidate,
        );
    }
}

#[test]
fn guarded_foreign_assertions_and_postconditions_match_imported_behavior() {
    let provider = compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        [r#"
            trusted module api;
            @link(name = "c") @abi(c) @symbol(name = "foreign_truth")
            extern trusted func asserted(pos flag: bool) -> bool
                uses(foreign_call)
                when(flag) { executes(total) ensures(result) };
        "#],
        ProductKind::Library,
        [],
        WorkerBudget::default(),
        None,
        [bray_symbols::NativeLinkRequirement::new(
            bray_base::NonEmptySharedStr::try_new("c").expect("native link name"),
            bray_symbols::NativeLinkKind::Dynamic,
        )],
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let bundle = export(&provider);

    assert!(
        bundle
            .semantics()
            .callable_contracts()
            .iter()
            .flat_map(|c| &*c.execution_contract().evidence)
            .all(|e| e.origin == bray_symbols::CallableExecutionOrigin::ForeignAssertion)
    );

    for (argument, requirement, valid) in [
        ("true", "", true),
        ("false", "", false),
        ("flag", "", false),
        ("flag", "requires(flag)", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using example.package.api;
            trusted func root(pos flag: bool) -> bool
                uses(foreign_call) {requirement} executes(total) ensures(result)
            {{ return example.package.api.asserted({argument}); }}
        "#
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

#[test]
fn imported_generator_predicates_reuse_eager_checked_loop_state() {
    let provider = compilation(
        r#"
        module api;
        predicate generated(count: i32) = all({ each item in 0..count { yield item < count; } });
        predicate positive(count: i32) = any({ each item in 0..count { yield item > 0; } });
        const func all_values(pos count: i32) -> bool { return all({ each item in 0..count { yield item < count; } }); }
        const func any_values(pos count: i32) -> bool { return any({ each item in 0..count { yield item > 0; } }); }
        const func eager(pos flag: bool) -> bool {
            let mut finished: bool = false;
            let mut produced: i32 = 0;
            let values = { each item in 0..3 { produced += 1; yield false; finished = item == 2; yield flag; } };
            return finished && produced == 3 && !all(values) && (any(values) == flag) && produced == 3;
        }
    "#,
    );

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
        const EMPTY: bool = example.package.api.all_values(0) && !example.package.api.any_values(0);
        const ONE: bool = example.package.api.all_values(1) && !example.package.api.any_values(1);
        const MANY: bool = example.package.api.all_values(3) && example.package.api.any_values(3);
        const EAGER: bool = example.package.api.eager(true);
        func root(pos count: i32) requires(example.package.api.generated(count = count), example.package.api.eager(count > 0), EMPTY && ONE && MANY && EAGER) {}
    "#,
        [super::fixtures::implementation_dependency(bundle)],
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    crate::test_support::assert_boolean_constants(
        &consumer,
        &[
            ("EMPTY", true),
            ("ONE", true),
            ("MANY", true),
            ("EAGER", true),
        ],
    );
}

#[test]
fn interleaved_guarded_postconditions_round_trip_in_source_order() {
    let provider = compilation(
        r"
        trusted module api;
        struct Reference { address: usize; }
        trusted predicate retained(owner: &Reference);
        trusted predicate owned(owner: &Reference);
        trusted predicate available(owner: &Reference);
        trusted func preserve(pos owner: &Reference)
            requires(trusted retained(owner = owner))
            ensures(trusted retained(owner = owner))
            when(trusted owned(owner = owner))
            {
                ensures(trusted owned(owner = owner))
            }
            ensures(trusted available(owner = owner))
        {}
        ",
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let consumer = execution_consumer(
        &provider,
        r"
        trusted module app;
        using example.package.api;
        trusted func forward(pos owner: &example.package.api.Reference)
            requires(trusted example.package.api.retained(owner = owner))
            ensures(trusted example.package.api.retained(owner = owner))
            when(trusted example.package.api.owned(owner = owner))
            {
                ensures(trusted example.package.api.owned(owner = owner))
            }
            ensures(trusted example.package.api.available(owner = owner))
        { trusted example.package.api.preserve(owner); }
        ",
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:?}",
        consumer.check_diagnostics()
    );
}
