use crate::test_support::source_function_body_key;

use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

#[test]
fn captured_scalar_results_preserve_unary_and_binary_subtraction() {
    use bray_symbols::{CapturedValueOperation, CapturedValueTemplate, ConstantTermData, ConstantUnaryOperation, ConstantValueKind,
        DependencySubjectRoot, SymbolOrdinal};

    let api = r"
        module api;

        func negate_positive(pos value: i64) -> i64
        {
            if value > 0
            {
                return -value;
            }

            return value;
        }

        func decrement_positive(pos value: i64) -> i64
        {
            if value > 0
            {
                return value - 1;
            }

            return value;
        }
    ";

    let provider = compilation(api);

    let assert_result = |compilation: &crate::Compilation, function, unary| {
        let values = compilation.semantic_value_store().expect("the compilation owns its captured terms");
        let summary = crate::compilation::result_dependencies::source_function_dependencies(compilation, function);
        let parameter = ConstantTermData::CallableArgument(SymbolOrdinal::new(0));

        let mut returned = summary.captured_values().iter().flat_map(|row| row.bindings.iter())
            .filter(|(subject, _)| subject.subject_root() == DependencySubjectRoot::Result)
            .flat_map(|(_, value)| value.nodes());

        assert!(returned.any(|value| match value {
            CapturedValueTemplate::Data(term) if unary => matches!(values.constant_term_data(*term).as_ref(),
                ConstantTermData::Unary { operation: ConstantUnaryOperation::Negate, operand }
                if values.constant_term_data(*operand).as_ref() == &parameter),
            CapturedValueTemplate::Operation(CapturedValueOperation::Subtract, operands) if !unary => {
                let [CapturedValueTemplate::Input(left), CapturedValueTemplate::Data(right)] = operands.as_ref() else {
                    return false;
                };

                values.constant_term_data(*left).as_ref() == &parameter
                    && matches!(values.constant_term_data(*right).as_ref(), ConstantTermData::Value(value)
                        if matches!(values.constant_value_data(*value).kind(), ConstantValueKind::Integer(integer)
                            if integer.to_u64() == Some(1)))
            }
            _ => false,
        }), "{function} must preserve the actual arithmetic: {summary:?}");
    };

    for (callee, unary) in [("negate_positive", true), ("decrement_positive", false)] {
        assert_result(&provider, callee, unary);

        for imported in [false, true] {
            let module = if imported { "example.package.api" } else { "api" };

            let caller = format!(r"
                module app;
                using {module};

                func forward(pos value: i64) -> i64
                {{
                    if value > 0
                    {{
                        return {module}.{callee}(value);
                    }}

                    return value;
                }}
            ");

            let consumer = if imported { execution_consumer(&provider, &caller) }
                else { compilation_from_sources([api, &caller]) };

            assert_result(&consumer, "forward", unary);

            let lowered = consumer.lowered_unit(source_function_body_key(&consumer, "forward"))
                .expect("forwarding the captured scalar result lowers");

            assert!(!lowered.diagnostics().has_errors(), "{callee}, imported={imported}: {:?}", lowered.diagnostics());
        }
    }
}

#[test]
fn allocation_producers_preserve_freshness_in_source_and_imported_calls() {
    let api = include_str!("../../../execution_guarantees/fixtures/fresh-allocation-returns.bray")
        .replace("trusted module app;", "trusted module api;");

    let provider = compilation(&api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = r#"
            trusted module app;

            using core.memory;
            using API;

            trusted func probe(pos owner: &API.Owner)
                requires(trusted API.live(owner = &owner.marker))
                uses(manual_alloc)
            {
                let pointer = API.forward();

                trusted core.memory.deallocate(pointer, bytes = 8, align = 8);
                API.observe(owner);
            }
        "#.replace("API", prefix);

        let consumer = if imported { execution_consumer(&provider, &caller) }
            else { compilation_from_sources([&api, &caller]) };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "imported={imported}: {diagnostics:#?}");

        let lowered = consumer.lowered_unit(source_function_body_key(&consumer, "probe"))
            .expect("fresh allocation consumer lowers");

        assert!(!lowered.diagnostics().has_errors(), "imported={imported}: {:?}", lowered.diagnostics());
    }
}

#[test]
fn returned_owners_preserve_siblings_of_nested_union_mutations() {
    let api = r#"
        trusted module api;

        struct Marker
        {
        }

        struct Counter
        {
            mut value: u32;
        }

        union Content
        {
            Value(pos mut counter: Counter);
            Empty;
        }

        struct Owner
        {
            marker: Marker;
            mut content: Content;
        }

        trusted predicate live(owner: &Marker);

        func update(pos mut owner: Owner) -> Owner
        {
            match &mut owner.content
            {
                case Content.Value(counter)
                {
                    counter.value = 2;
                }
                case Content.Empty
                {
                }
            };

            return owner;
        }

        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:#?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = format!(r#"
            module app;

            using {prefix};

            func caller(pos owner: {prefix}.Owner)
                requires(trusted {prefix}.live(&owner.marker))
            {{
                let result = {prefix}.update(owner);

                {prefix}.inspect(&result);
            }}
        "#);

        let application = if imported {
            execution_consumer(&provider, &caller)
        } else {
            compilation_from_sources([api, &caller])
        };

        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "imported={imported}: {diagnostics:#?}; owned layout: {:#?}",
            crate::compilation::result_dependencies::source_function_dependencies(&provider, "update").owned_result());
    }
}

#[test]
fn owned_match_results_release_the_subject_loan() {
    let api = r#"
        module api;

        using core.memory;

        struct Control
        {
            value: usize;
        }

        struct Slot
        {
            mut control: Uninit<Control>?;
            generation: usize;
        }

        func clear(pos slot: &mut Slot)
        {
            slot.control = none;
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = r#"
            module app;

            using core.memory;
            using API;

            func take_constructed(pos slot: &mut API.Slot) -> API.Control
            {
                let retired_control = match &mut slot.control
                {
                    case ?storage
                    {
                        yield API.Control
                        {
                            value = 1
                        };
                    }
                    case none
                    {
                        panic("missing control");
                    }
                };

                API.clear(slot);

                return retired_control;
            }

            func take_view(pos slot: &mut API.Slot) -> &Uninit<API.Control>
            {
                let view = match &slot.control
                {
                    case ?storage
                    {
                        yield &storage;
                    }
                    case none
                    {
                        panic("missing control");
                    }
                };

                API.clear(slot);

                return view;
            }
        "#.replace("API", prefix);

        let consumer = if imported {
            execution_consumer(&provider, &caller)
        } else {
            compilation_from_sources([api, caller.as_str()])
        };

        for (name, valid) in [("take_view", false), ("take_constructed", true)] {
            let checked = consumer.lowered_unit(source_function_body_key(&consumer, name))
                .expect("owned match result retains its checked Source body");

            assert_eq!(!checked.diagnostics().has_errors(), valid,
                "{name}, imported={imported}: {:?}", checked.diagnostics());

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow);
            }
        }
    }
}

#[test]
fn guarded_borrowed_fields_compose_through_construction_and_calls() {
    let api = r#"
        trusted module api;

        struct Marker
        {
        }

        struct Lease
        {
            marker: Marker;
            active: bool;

            trusted destruct()
                requires(trusted live(owner = &self.marker))
            {
            }
        }

        trusted predicate live(owner: &Marker);

        trusted extern func acquire() -> Lease
            ensures(trusted live(owner = &result.marker));

        extern func unproven() -> Lease;

        struct Anchor
        {
            lease: &Lease;
        }

        predicate access(owner: &Anchor) = owner.lease.active && live(owner = &owner.lease.marker);

        trusted func anchor(pos lease: &Lease) -> Anchor
            requires(lease.active, trusted live(owner = &lease.marker))
            ensures(trusted live(owner = &lease.marker), trusted access(owner = &result))
            executes(pure, total)
        {
            return
            {
                lease = lease
            };
        }

        trusted func inspect(pos owner: &mut Anchor)
            requires(trusted access(owner = &owner))
            ensures(trusted access(owner = &owner))
        {
        }

    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        for (producer, guarded, valid) in [("acquire", true, true), ("acquire", false, true), ("unproven", true, false)] {
            let guard = if guarded {
                r#"
                    if !lease.active
                    {
                        return;
                    }
                "#
            } else {
                ""
            };

            let caller = format!(r#"
                module app;

                using {prefix};

                func caller()
                {{
                    let lease = {prefix}.{producer}();
                    {guard}
                    let mut audit_lease_anchor = {prefix}.anchor(&lease);
                    {prefix}.inspect(&mut audit_lease_anchor);
                }}
            "#);

            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([api, caller.as_str()])
            };

            let checked = consumer.lowered_unit(source_function_body_key(&consumer, "caller"))
                .expect("guarded borrowed constructor retains its checked Source body");

            assert_eq!(!checked.diagnostics().has_errors(), valid,
                "imported={imported}, producer={producer}, guarded={guarded}: {:?}", checked.diagnostics());

            if !valid {
                assert!(checked.diagnostics().by_kind(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven)
                    .any(|diagnostic| diagnostic.primary_span().is_some_and(|span| {
                        crate::test_support::diagnostic_at_call(diagnostic, span.source_id(), &caller, "anchor(&lease)")
                    })), "{:#?}", checked.diagnostics());
            }
        }
    }
}

#[test]
fn observed_nullable_patterns_keep_their_subject_until_scope_exit() {
    let api = r#"
        trusted module api;

        callable Callback = func(pos value: usize) -> usize;

        struct Payload
        {
            value: usize;
            callback: Callback;
        }

        struct Container
        {
            payload: Payload?;
        }

        trusted func select(pos owner: &(Payload?)) -> &(Payload?)
        {
            return owner;
        }

        trusted func container(pos owner: &Container) -> &Container
        {
            return owner;
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = format!(r#"
            module app;

            using {prefix};

            func project(pos owner: &({prefix}.Payload?)) -> usize
            {{
                if let ?binding = {prefix}.select(owner)
                {{
                    if binding.value != 0 && binding.value != 42
                    {{
                        return binding.callback(binding.value);
                    }}

                    return binding.callback(binding.value);
                }}

                return 0;
            }}

            func nested(pos owner: &{prefix}.Container) -> usize
            {{
                if let ?binding = ({prefix}.container(owner)).payload
                {{
                    if binding.value != 0 && binding.value != 42
                    {{
                        return binding.callback(binding.value);
                    }}

                    return binding.callback(binding.value);
                }}

                return 0;
            }}

            func escaping(pos callback: {prefix}.Callback) -> (&{prefix}.Payload)?
            {{
                let owner: {prefix}.Payload? =
                {{
                    value = 42,
                    callback = callback
                }};

                if let ?binding = {prefix}.select(&owner)
                {{
                    return &binding;
                }}

                return none;
            }}
        "#);

        let consumer = if imported {
            execution_consumer(&provider, &caller)
        } else {
            compilation_from_sources([api, caller.as_str()])
        };

        for name in ["project", "nested", "escaping"] {
            let checked = consumer.lowered_unit(source_function_body_key(&consumer, name))
                .expect("nullable pattern retains its checked Source body");

            if name == "escaping" {
                bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency);
            } else {
                assert!(!checked.diagnostics().has_errors(), "{name}, imported={imported}: {:?}", checked.diagnostics());
            }
        }
    }
}

#[test]
fn borrowed_call_projections_outlive_their_retired_descriptors() {
    let api = r#"
        trusted module api;

        using core.memory;

        struct Marker
        {
        }

        struct Payload
        {
            marker: Marker;
        }

        struct Owner
        {
            pointer: DataPointer<Payload>;
        }

        trusted predicate held(owner: &Owner);
        trusted predicate live(marker: &Marker);

        trusted func borrow_payload(pos owner: &Owner) -> &Payload
            requires(trusted held(owner = owner))
            ensures(trusted held(owner = owner), trusted live(marker = &result.marker))
            executes(pure, total)
            uses(raw_memory, unchecked_alias)
        {
            let permit = trusted lambda(pos owner: &Owner)
                requires(trusted held(owner = owner))
                ensures(
                    trusted core.memory.valid_read<Payload>(pointer = owner.pointer, count = 1),
                    trusted core.memory.initialized_as<Payload>(pointer = owner.pointer),
                    trusted core.memory.aligned_for<Payload>(pointer = owner.pointer),
                    trusted core.memory.shared_alias_valid<Payload>(pointer = owner.pointer),
                    trusted core.memory.epoch_current<Payload>(pointer = owner.pointer),
                    trusted core.memory.synchronized_access<Payload>(pointer = owner.pointer),
                    trusted core.memory.movement_stable<Payload>(pointer = owner.pointer),
                    trusted core.memory.finalization_pending<Payload>(pointer = owner.pointer)
                )
                executes(pure, total)
            {
            };

            trusted permit(owner);
            return trusted core.memory.borrow_from<Payload, Owner>(owner, owner.pointer);
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = format!(r#"
            module app;

            using {prefix};

            func project(pos owner: &{prefix}.Owner)
                requires(trusted {prefix}.held(owner = owner))
            {{
                let marker = &(trusted {prefix}.borrow_payload(owner)).marker;
                _ = marker;
            }}

            func escaping(pos owner: {prefix}.Owner) -> &{prefix}.Marker
                requires(trusted {prefix}.held(owner = &owner))
            {{
                return &(trusted {prefix}.borrow_payload(&owner)).marker;
            }}
        "#);

        let consumer = if imported {
            execution_consumer(&provider, &caller)
        } else {
            compilation_from_sources([api, caller.as_str()])
        };

        for name in ["project", "escaping"] {
            let checked = consumer.lowered_unit(source_function_body_key(&consumer, name))
                .expect("borrowed field projection retains its checked Source body");

            if name == "escaping" {
                bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency);
            } else {
                assert!(!checked.diagnostics().has_errors(), "imported={imported}: {:?}", checked.diagnostics());
            }
        }
    }
}

#[test]
fn borrowed_method_receivers_retain_the_returned_referent() {
    let api = r#"
        module api;

        struct Marker
        {
            func guard() -> Guard
            {
                return
                {
                    owner = &self
                };
            }
        }

        struct Guard
        {
            owner: &Marker;

            destruct()
            {
            }
        }

        static ROOT: Marker = Marker
        {
        };

        func owner() -> &Marker
        {
            return &ROOT;
        }

        func identity(pos value: &Marker) -> &Marker
        {
            return value;
        }

        func acquire() -> Guard
        {
            return owner().guard();
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let caller = format!(r#"
            module app;

            using {prefix};

            func global()
            {{
                let guard = {prefix}.acquire();
            }}

            func local()
            {{
                let marker = {prefix}.Marker
                {{
                }};
                let guard = {prefix}.identity(&marker).guard();
            }}

            func escaping() -> {prefix}.Guard
            {{
                let marker = {prefix}.Marker
                {{
                }};

                return {prefix}.identity(&marker).guard();
            }}
        "#);

        let consumer = if imported {
            execution_consumer(&provider, &caller)
        } else {
            compilation_from_sources([api, caller.as_str()])
        };

        for name in ["global", "local", "escaping"] {
            let checked = consumer.lowered_unit(source_function_body_key(&consumer, name))
                .expect("borrowed method receiver has checked Source output");

            if name == "escaping" {
                bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency);
            } else {
                assert!(!checked.diagnostics().has_errors(), "{name}, imported={imported}: {:?}", checked.diagnostics());
            }
        }
    }
}

#[test]
fn independent_borrowed_results_do_not_claim_owned_marker_preservation() {
    let api = r#"
        trusted module api;
        using core.memory;
        using core.target;

        struct Marker {}
        struct Holder { marker: &Marker; }
        trusted predicate live(marker: &Marker);

        trusted const func make() -> Marker ensures(trusted live(&result))
        {
            return Marker {};
        }

        static TOKEN: Marker = make();

        func global_reference() -> &Marker ensures(trusted live(result))
        {
            return &TOKEN;
        }

        func input_reference(pos marker: &Marker) -> &Marker
            requires(trusted live(marker))
            ensures(trusted live(result))
        {
            return marker;
        }

        trusted func publish() -> usize uses(intrinsic)
        {
            return core.target.expose_address<Marker>(
                core.memory.address_of<Marker>(&TOKEN)
            );
        }

        trusted func publish_reference(pos marker: &Marker) -> usize uses(intrinsic)
        {
            return core.target.expose_address<Marker>(core.memory.address_of<Marker>(marker));
        }

        func observe(pos marker: &Marker)
            requires(trusted live(marker)) executes(pure, total)
        {}
    "#;

    let provider = compilation(api);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for imported in [false, true] {
        for owned in [false, true] {
            for (opaque, loan) in [
                (false, ""), (true, ""),
                (false, "slot"), (true, "slot"),
                (false, "referent"), (true, "referent"),
                (false, "carrier"), (true, "carrier"),
            ] {
                let prefix = if imported { "example.package.api" } else { "api" };

                let reference = if owned {
                    format!("{prefix}.input_reference(&owner)")
                } else {
                    format!("{prefix}.global_reference()")
                };

                let source = format!(
                    r#"
                    module app;
                    using {prefix};
                    extern func opaque();

                    func caller()
                    {{
                        let owner = {prefix}.make();
                        let reference = {reference};
                        {hold}
                        _ = {prefix}.publish();
                        {effect}
                        {prefix}.observe({observed});
                        {release}
                    }}
                    "#,
                    effect = if opaque { "opaque();" } else { "" },
                    observed = if loan == "carrier" { "holder.marker" } else { "reference" },
                    hold = match loan {
                        "carrier" => format!("let holder: {prefix}.Holder = {{ marker = reference }};"),
                        "slot" => format!("let loan: &&{prefix}.Marker = &reference;"),
                        "referent" => format!("let loan: &{prefix}.Marker = &reference;"),
                        _ => String::new(),
                    },
                    release = if loan.is_empty() || loan == "carrier" { "" } else { "_ = loan;" },
                );

                let application = if imported {
                    execution_consumer(&provider, &source)
                } else {
                    compilation_from_sources([api, &source])
                };

                let diagnostics = application.check_diagnostics();

                assert!(
                    !diagnostics.has_errors(),
                    "imported={imported}, owned={owned}, opaque={opaque}, loan={loan}: {diagnostics:#?}"
                );

            }
        }
    }

    for imported in [false, true] {
        for published in [false, true] {
            let prefix = if imported { "example.package.api" } else { "api" };

            let source = format!(r#"
                module app;
                using {prefix};
                extern func opaque();

                func caller(pos holder: &{prefix}.Holder)
                    requires(trusted {prefix}.live(holder.marker))
                {{
                    {publication}
                    opaque();
                    {prefix}.observe(holder.marker);
                }}
            "#, publication = if published {
                format!("_ = {prefix}.publish_reference(holder.marker);")
            } else { String::new() });

            let application = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([api, &source])
            };

            let diagnostics = application.check_diagnostics();

            assert!(!diagnostics.has_errors(),
                "imported={imported}, published={published}: {diagnostics:#?}");

        }
    }

}

#[test]
fn imported_static_publication_only_exposes_the_selected_static() {
    for directive in ["", "@thread_local"] {
        for address in ["&TOKEN<N>", "&owner.marker"] {
            let provider = compilation(&format!(
                r#"
                trusted module api;
                using core.memory;
                using core.target;

                struct Marker {{}}
                struct Owner {{ marker: Marker; }}
                trusted predicate live(marker: &Marker);

                trusted const func make_marker() -> Marker
                    ensures(trusted live(&result))
                {{
                    return Marker {{}};
                }}

                trusted func create() -> Owner
                    ensures(trusted live(&result.marker))
                {{
                    return {{ marker = Marker {{}} }};
                }}

                {directive} static TOKEN<const N: u32>: Marker = make_marker();
                {directive} static OTHER: Marker = make_marker();

                trusted func publish<const N: u32>(pos owner: &Owner) -> usize uses(intrinsic)
                {{
                    return core.target.expose_address<Marker>(
                        core.memory.address_of<Marker>({address})
                    );
                }}

                trusted func relay<const N: u32>(pos owner: &Owner) -> usize
                {{
                    return publish<N>(owner);
                }}

                trusted func observe(pos marker: &Marker)
                    requires(trusted live(marker)) executes(pure, total)
                {{}}
                "#,
            ));

            for observed in ["TOKEN<1>", "TOKEN<2>", "OTHER"] {
                let consumer = execution_consumer(
                    &provider,
                    &format!(
                        r#"
                        module app;
                        using example.package.api;
                        extern func opaque();

                        func caller()
                        {{
                            example.package.api.observe(&example.package.api.TOKEN<1>);
                            example.package.api.observe(&example.package.api.TOKEN<2>);
                            example.package.api.observe(&example.package.api.OTHER);
                            let owner = example.package.api.create();
                            _ = example.package.api.relay<1>(&owner);
                            opaque();
                            example.package.api.observe(&example.package.api.{observed});
                        }}
                        "#,
                    ),
                );

                let diagnostics = consumer.check_diagnostics();
                let valid = address != "&TOKEN<N>" || observed != "TOKEN<1>";

                assert_eq!(
                    !diagnostics.has_errors(),
                    valid,
                    "{directive}, {address}, {observed}: {diagnostics:#?}"
                );

                if !valid {
                    bray_testing::assert_goal_state_diagnostic_kind(
                        diagnostics,
                        bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                    );
                }
            }
        }
    }
}

#[test]
fn imported_static_borrows_keep_physical_origins_separate_from_lifetimes() {
    use bray_symbols::{DependencyProjection, DependencyReferenceKind, DependencyRequirement,
        DependencySubjectRoot, StaticStorageDuration};

    for (directive, exact_thread) in [("", false), ("@thread_local", true)] {
        let provider = compilation(&format!(
            r#"
            module api;
            struct Cell {{ value: u32; }}
            {directive} static STATE: Cell = {{ value = 7 }};
            func borrow(pos unrelated: &u32) -> &Cell {{ return &STATE; }}
            func forward(pos unrelated: &u32) -> &Cell {{ return borrow(unrelated); }}
        "#
        ));

        let consumer = execution_consumer(
            &provider,
            r#"
            module app;
            using example.package.api;
            func caller(pos unrelated: &u32) -> &u32 {
                return &example.package.api.forward(unrelated).value;
            }
        "#,
        );

        let diagnostics = consumer.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "exact_thread={exact_thread}: {diagnostics:#?}"
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "caller",
        );

        let cancellation = crate::CancellationToken::new();
        let binding = consumer.binding_context(&cancellation).expect("consumer symbols bind");
        let values = consumer.semantic_value_store().expect("consumer owns its static selections");
        let duration = if exact_thread { StaticStorageDuration::ExactThread } else { StaticStorageDuration::Product };

        let references = summary.borrows();

        let [reference] = references.as_ref() else {
            panic!("caller returns one exact reference: {summary:#?}");
        };

        let entry = reference.entry().expect("the static reference retains its origin");
        let root = entry.subject_root();
        let state = root.static_declaration(values).expect("the origin is static");

        assert!(matches!(root, DependencySubjectRoot::StaticInstance(_, actual) if actual == duration));
        assert_eq!(binding.member_name(state.into()).expect("imported static name").map(|name| name.as_str()), Some("STATE"));
        assert_eq!(reference.kind(), DependencyReferenceKind::Borrow);
        assert_eq!(reference.exit().subject_root(), DependencySubjectRoot::Result);

        assert!(reference.is_exact() && reference.exit().projections().is_empty()
            && reference.target_projections().is_empty() && reference.dereferences() == 0);

        let [DependencyProjection::ProductField(field)] = entry.projections() else {
            panic!("the borrow reaches exactly one static field: {reference:?}");
        };

        assert_eq!(binding.member_name((*field).into()).expect("imported field name").map(|name| name.as_str()), Some("value"));

        assert!(
            summary.requirements().iter().any(|requirement| {
                let DependencyRequirement::Direct { subject, .. } = requirement
                else {
                    return false;
                };

                subject.subject_root().static_duration() == Some(duration)
                    && subject.subject_root().static_declaration(values) == Some(state)
            }),
            "static lifetime must survive serialization: {summary:#?}"
        );

        assert!(!summary.requirements().iter().any(|requirement| {
            matches!(requirement, DependencyRequirement::Direct { subject, .. }
                if matches!(subject.subject_root(), DependencySubjectRoot::Parameter(_)))
        }), "the unused parameter has no lifetime dependency: {summary:#?}");
    }
}

#[test]
fn imported_call_effects_distinguish_forwarding_from_mutation() {
    let provider = compilation(
        r#"
        module api;

        struct Cell
        {
            mut value: bool;
            mut visits: u32;
        }

        func same<T>(pos value: &T) -> &T
        {
            return value;
        }

        func rewrite(pos cell: &mut Cell)
        {
            cell.value = false;
        }

        func lend(pos cells: &mut [Cell], pos index: usize)
            requires(index < cells.length())
        {
            let entry = &mut cells[index];
            _ = entry.value;
        }

        func rewrite_at(pos cells: &mut [Cell], pos index: usize)
            requires(index < cells.length())
        {
            let entry = &mut cells[index];

            entry.value = false;
        }

        func choose(pos left: &mut Cell, pos right: &mut Cell, pos first: bool) -> &mut Cell
        {
            return if first
            {
                yield left;
            }
            else
            {
                yield right;
            };
        }

        func rewrite_choice(pos left: &mut Cell, pos right: &mut Cell, pos first: bool)
        {
            let selected = choose(left, right, first);

            let before = selected.value;

            selected.visits = 1;
            assert(selected.value == before);

            selected.value = false;
        }

        func rewrite_joined(pos left: &mut Cell, pos right: &mut Cell, pos first: bool)
        {
            let selected = if first
            {
                yield left;
            }
            else
            {
                yield right;
            };

            let before = selected.value;

            selected.visits = 1;
            assert(selected.value == before);

            selected.value = false;
        }
    "#,
    );

    for (body, unchanged) in [
        (
            r#"
            func caller(pos cell: &example.package.api.Cell) -> &example.package.api.Cell
            {
                return example.package.api.same(cell);
            }
            "#,
            true,
        ),
        (
            r#"
            func caller(pos cell: &mut example.package.api.Cell)
            {
                example.package.api.rewrite(cell);
            }
            "#,
            false,
        ),
        (
            r#"
            func caller(pos cells: &mut [example.package.api.Cell])
                requires(cells.length() > 0)
            {
                example.package.api.lend(cells, 0);
            }
            "#,
            true,
        ),
        (
            r#"
            func caller(pos cells: &mut [example.package.api.Cell])
                requires(cells.length() > 0)
            {
                example.package.api.rewrite_at(cells, 0);
            }
            "#,
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;

            using example.package.api;

            {body}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        let contracts = consumer
            .dependency_contracts(source_function_body_key(&consumer, "caller"))
            .expect("fixture `dependency_contracts` call must succeed");

        assert!(
            !contracts.diagnostics().has_errors(),
            "{:?}",
            contracts.diagnostics()
        );

        let calls = contracts.value().calls();

        let call = calls
            .iter()
            .find_map(|(site, call)| matches!(site, bray_bound_tree::BoundExecutionSite::Node(_)).then_some(call))
            .expect("fixture iterator must provide its expected item");

        assert_eq!(call.effects().is_empty(), unchanged, "{:?}", call.effects());

        if !unchanged {
            assert!(!call.effects().iter().any(|effect| matches!(effect,
                bray_symbols::DependencyStorageEffect::UnboundedMutation { .. })),
                "a selected input element write stays within that input: {:?}", call.effects());

            assert!(
                call.effects().iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Replace(_),
                        ..
                    }
                )),
                "the imported body retains its specific write target: {:?}",
                call.effects()
            );
        }
    }

    for function in ["rewrite_choice", "rewrite_joined"] {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(&provider, function);

        let written = summary.effects().iter().map(|effect| match effect {
            bray_symbols::DependencyStorageEffect::Indirect {
                reference, kind: bray_symbols::DependencyMutationKind::Replace(_), ..
            } => reference.subject_root(),
            effect => panic!("finite selected write retains its actual candidates: {function}: {effect:?}"),
        }).collect::<std::collections::BTreeSet<_>>();

        assert_eq!(written, [0, 1].map(|index| bray_symbols::DependencySubjectRoot::Parameter(
            bray_symbols::SymbolOrdinal::new(index))).into());

        for (subject, valid) in [("stable", true), ("left", false), ("right", false)] {
            let caller = format!(r#"
                module app;

                using example.package.api;

                func caller(
                    pos left: &mut example.package.api.Cell,
                    pos right: &mut example.package.api.Cell,
                    pos stable: &example.package.api.Cell,
                    pos first: bool
                )
                    requires({subject}.value)
                    ensures({subject}.value)
                {{
                    example.package.api.{function}(left, right, first);
                }}
            "#);

            let consumer = execution_consumer(&provider, &caller);

            bray_testing::assert_goal_state_diagnostic_outcome(consumer.check_diagnostics(),
                (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingPostconditionNotProven), &caller);
        }
    }
}

#[test]
fn imported_immutable_function_values_reuse_the_producers_report_contract() {
    let provider = compilation(
        r#"
        module api;
        struct Cell { mut value: bool; }
        func rewrite(pos cell: &mut Cell) { cell.value = false; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Cell;
        using example.package.api.rewrite;
        func caller(pos cell: &mut example.package.api.Cell)
        {
            let operation: func(pos cell: &mut example.package.api.Cell) = example.package.api.rewrite;
            operation(cell);
            operation(cell);
        }
    "#,
    );

    let key = source_function_body_key(&consumer, "caller");

    let checked = consumer
        .dependency_contracts(key.clone())
        .expect("imported callable value checks");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:#?}",
        checked.diagnostics()
    );

    let calls = checked.value().calls();

    let invocations = calls.iter().filter_map(|(site, call)|
        matches!(site, bray_bound_tree::BoundExecutionSite::Node(_)).then_some(call)).collect::<Vec<_>>();

    assert_eq!(invocations.len(), 2);

    for call in invocations {
        assert!(call.reports().completed().next().is_none(), "{call:#?}");
    }

    let lowered = consumer
        .lowered_unit(key)
        .expect("imported callable value lowers");

    assert!(
        !lowered.diagnostics().has_errors(),
        "{:#?}",
        lowered.diagnostics()
    );

    let mir = lowered.value().as_ref().and_then(|unit| unit.mir()).expect("caller retains its MIR");
    let reached = bray_ir::reachable_blocks(mir, [mir.entry()], &Default::default());

    let reports = reached.into_iter().filter_map(|id| {
        match mir.block(id).expect("reached MIR block exists").terminator().kind() {
            bray_ir::MirTerminatorKind::Return { report, .. } => Some(report),
            _ => None,
        }
    }).collect::<Vec<_>>();

    assert!(!reports.is_empty() && reports.iter().all(|report| report.is_none()), "{reports:?}");
}

#[test]
fn returned_values_preserve_imported_generic_dependencies() {
    let provider = compilation(
        r#"
            module api;

            func same<T>(pos value: &T) -> &T
            {
                return value;
            }
        "#,
    );

    for (body, valid) in [
        (
            r#"
                func caller(pos value: &bool) -> &bool
                {
                    return example.package.api.same(value);
                }
            "#,
            true,
        ),
        (
            r#"
                func caller() -> &bool
                {
                    let value: bool = true;

                    return example.package.api.same(&value);
                }
            "#,
            false,
        ),
    ] {
        let source = format!(
            r#"
                module app;

                using example.package.api.same;

                {body}
            "#
        );

        let consumer = execution_consumer(&provider, &source);

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .expect("fixture callable must complete storage-flow analysis");

        if valid {
            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn returned_assignments_and_errors_survive_interfaces() {
    let provider = compilation(
        r#"
            module api;

            struct Holder
            {
                mut value: &bool;
            }

            func replace(pos first: &bool, pos second: &bool) -> Holder
            {
                let mut result = Holder { value = first };
                result.value = second;

                return result;
            }

            struct OptionalHolder
            {
                value: (&bool)?;
            }

            func wrap(pos value: &bool) -> OptionalHolder
            {
                return { value = value };
            }

            func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
            {
                let value = try input;

                return Ok(value);
            }
        "#,
    );

    for (result, call) in [
        (
            "example.package.api.Holder",
            "example.package.api.replace(caller, argument)",
        ),
        (
            "example.package.api.OptionalHolder",
            "example.package.api.wrap(argument)",
        ),
        (
            "Result<bool, &bool>",
            "example.package.api.forward(Error(argument))",
        ),
    ] {
        for (argument, valid) in [("caller", true), ("&local", false)] {
            let call = call.replace("argument", argument);

            let source = format!(
                r#"
                    module app;

                    using example.package.api;

                    func caller(pos caller: &bool) -> {result}
                    {{
                        let local: bool = true;

                        return {call};
                    }}
                "#
            );

            let consumer = execution_consumer(&provider, &source);

            let flow = consumer
                .storage_flow(source_function_body_key(&consumer, "caller"))
                .expect("fixture callable must complete storage-flow analysis");

            if valid {
                assert!(
                    consumer.check_diagnostics().is_empty(),
                    "{:?}",
                    consumer.check_diagnostics()
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
fn returned_borrow_alias_assignments_survive_interfaces() {
    let provider = compilation(
        r#"
            module api;

            func same<T>(pos value: &mut T) -> &mut T
            {
                return value;
            }
        "#,
    );

    for (argument, valid) in [("caller", true), ("&local", false)] {
        let source = format!(
            r#"
                module app;

                using example.package.api;

                struct Holder
                {{
                    mut value: &bool;
                }}

                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {{
                    let owner = example.package.api.same(&mut result);
                    owner.value = second;

                    return result;
                }}

                func caller(pos caller: &bool) -> Holder
                {{
                    let local: bool = true;

                    return replace(Holder {{ value = caller }}, {argument});
                }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn implicit_call_reborrows_preserve_source_and_imported_parameter_modes() {
    let declaration = r#"
func touch<T>(pos value: &mut T)
{
}

func take<T>(pos value: T)
{
}

func same(pos value: &mut bool) -> &mut bool
{
    return value;
}
"#;

    let provider = compilation(&format!("module api;\n{declaration}"));

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    let moved = bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage;
    let conflict = bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow;

    for (body, expected) in [
        (
            r#"let local: &mut bool = caller;

    touch(local);
    touch(local);"#,
            None,
        ),
        (
            r#"let local: &mut bool = same(caller);

    touch(local);
    touch(local);"#,
            None,
        ),
        (
            r#"let local: &mut bool = caller;

    take(local);
    touch(local);"#,
            Some(moved),
        ),
        (
            r#"let local: &mut bool = caller;
    let escaped: &mut bool = same(local);

    touch(local);
    touch(escaped);"#,
            Some(conflict),
        ),
    ] {
        let body = format!(
            r#"
func check(pos caller: &mut bool)
{{
    {body}
}}
"#
        );

        let source = compilation(&format!("module app;\n{declaration}\n{body}"));

        let imported_body = body
            .replace("touch(", "example.package.api.touch(")
            .replace("take(", "example.package.api.take(")
            .replace("same(", "example.package.api.same(");

        let imported = execution_consumer(
            &provider,
            &format!("module app;\nusing example.package.api;\n{imported_body}"),
        );

        for consumer in [&source, &imported] {
            let diagnostics = consumer.check_diagnostics();

            if let Some(expected) = expected {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, expected);
            } else {
                assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");

                let key = source_function_body_key(consumer, "check");

                let lowered = consumer
                    .lowered_unit(key)
                    .expect("source or imported call reborrow lowering");

                assert!(
                    !lowered.diagnostics().has_errors(),
                    "{body}: {:?}",
                    lowered.diagnostics()
                );
            }
        }
    }
}

#[test]
fn returned_values_preserve_default_wrapper_dependencies_in_interfaces() {
    let provider = compilation(
        r#"
            module api;

            struct Holder
            {
                value: &bool;
            }

            func wrap(pos transient: &bool, pos anchor: &bool) -> Holder
            {
                return Holder
                {
                    value = anchor
                };
            }

            func choose(pos transient: &bool, pos anchor: &bool, value: Holder = wrap(transient, anchor)) -> Holder
            {
                return value;
            }
        "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (body, valid) in [
        (
            r#"
                func caller(pos anchor: &bool) -> example.package.api.Holder
                {
                    let transient: bool = true;

                    return example.package.api.choose(&transient, anchor);
                }
            "#,
            true,
        ),
        (
            r#"
                func caller() -> example.package.api.Holder
                {
                    let anchor: bool = true;

                    return example.package.api.choose(&anchor, &anchor);
                }
            "#,
            false,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                    module app;

                    using example.package.api;

                    {body}
                "#
            ),
        );

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .expect("fixture callable must complete storage-flow analysis");

        if valid {
            assert!(
                !consumer.check_diagnostics().has_errors(),
                "{:?}",
                consumer.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn storage_projection_guarantees_survive_generic_interfaces() {
    use bray_bound_tree::{
        BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
        BoundExpression,
    };

    for borrow in ["&", "&mut "] {
        let provider = compilation(
            &r#"
                module api;

                        func project<T>(pos value: BORROWbox T) -> BORROWT executes(pure, total)
                        {
                            return match value
                            {
                                case box(inner)
                                {
                                    yield BORROWinner;
                                }
                            };
                        }
            "#
            .replace("BORROW", borrow),
        );

        assert!(
            !provider.check_diagnostics().has_errors(),
            "{:?}",
            provider.check_diagnostics()
        );

        let consumer = execution_consumer(
            &provider,
            &r#"
                module app;
                        using example.package.api.project;

                        func root(pos value: BORROWbox bool) -> BORROWbool executes(pure, total)
                        {
                            return example.package.api.project<bool>(value);
                        }
            "#
            .replace("BORROW", borrow),
        );

        assert!(
            !consumer.check_diagnostics().has_errors(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let key = source_function_body_key(&consumer, "root");

        let storage = consumer
            .storage_plan(key.clone())
            .expect("fixture callable must have a storage plan");

        let unit = consumer
            .bound_unit(key.clone())
            .expect("fixture callable must bind successfully");

        let contracts = consumer
            .dependency_contracts(key)
            .expect("fixture `dependency_contracts` call must succeed");

        let (capability, input) = storage
            .value()
            .borrow_capability_entries()
            .find(|(_, capability)| capability.entry_binding().is_some())
            .expect("fixture lookup must find its expected value");

        let root = storage
            .value()
            .root_identity(input.access())
            .expect("fixture `root_identity` call must succeed");

        let call = unit
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Call(_)).then_some(id)
            })
            .expect("fixture selection must find its expected value");

        let contract = contracts
            .value()
            .expression(call)
            .and_then(|id| contracts.value().contract(id))
            .expect("fixture `and_then` call must succeed");

        assert!(
            contract.requirements().iter().any(|requirement| matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::BorrowCapability(actual),
                    kind: BoundDependencyRequirementKind::BorrowCapabilityActive(kind),
                } if *actual == capability && *kind == input.kind()
            )),
            "{contract:?}"
        );

        assert!(
            contract.requirements().iter().any(|requirement| matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(access),
                    kind: BoundDependencyRequirementKind::StorageAlive,
                } if storage.value().root_identity(*access) == Some(root)
            )),
            "{contract:?}"
        );

        let escaping = execution_consumer(
            &provider,
            &r#"
                module app;

                using example.package.api.project;

                func root() -> BORROWbool
                {
                    let mut value = box(true);

                    return example.package.api.project<bool>(BORROWvalue);
                }
            "#
            .replace("BORROW", borrow),
        );

        let flow = escaping
            .storage_flow(source_function_body_key(&escaping, "root"))
            .expect("fixture callable must complete storage-flow analysis");

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}

#[test]
fn exact_borrow_origin_survives_generic_export_and_import() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T>
        {
            reference: &T;
            address: DataPointer<T>;
        }
        func make<T>(pos input: &T, pos address: DataPointer<T>) -> Holder<T>
            {
                return
                {
                    reference = input,
                    address = address,
                };
            }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func forwarded(pos input: &bool, pos address: DataPointer<bool>) -> example.package.api.Holder<bool>
            {
                return example.package.api.make(input, address);
            }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let callable = crate::test_support::source_function(&consumer, "forwarded");

    let context = consumer
        .binding_context(&consumer.state.cancellation)
        .expect("fixture compilation must provide a binding context");

    let dependencies = bray_binder::SymbolQueryProvider::resolve_symbol_query(
        &context,
        bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
            callable.into(),
        ),
    )
    .expect("fixture symbol query must resolve");

    let template = consumer
        .semantic_value_store()
        .expect("fixture compilation must provide its semantic value store")
        .dependency_contract_template_data(*dependencies.value());

    assert_eq!(template.borrows().len(), 2, "{template:#?}");

    for (parameter, kind) in [
        (0, bray_symbols::DependencyReferenceKind::Borrow),
        (1, bray_symbols::DependencyReferenceKind::Address),
    ] {
        let borrows = template
            .borrows();

        let relation = borrows
            .iter()
            .find(|relation| relation.kind() == kind)
            .unwrap_or_else(|| panic!("imported origin must retain {kind:?}: {template:#?}"));

        assert_eq!(
            relation.entry().expect("input origin").subject_root(),
            bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                parameter
            ))
        );

        assert_eq!(
            relation.exit().subject_root(),
            bray_symbols::DependencySubjectRoot::Result
        );

        assert_eq!(relation.exit().projections().len(), 1);
    }
}

#[test]
fn inferred_unwind_behavior_survives_export_and_import() {
    let provider = compilation(
        r#"
        module api;

        func spin()
        {
            while true
            {
            }
        }

        func fail()
        {
            panic("failure");
        }

        func cancel() -> u32
        {
            let outcome: RunResult<unit> = Cancelled;

            try outcome;

            return 0;
        }

        func with_default(pos value: u32 = cancel())
        {
        }
    "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (callee, expected, cancellation) in [
        (
            "spin",
            false,
            bray_symbols::CurrentRunCancellation::NotEntered,
        ),
        (
            "fail",
            true,
            bray_symbols::CurrentRunCancellation::NotEntered,
        ),
        (
            "cancel",
            true,
            bray_symbols::CurrentRunCancellation::MayEnter,
        ),
        (
            "with_default",
            true,
            bray_symbols::CurrentRunCancellation::MayEnter,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                module app;

                using example.package.api;

                func root()
                {{
                    example.package.api.{callee}();
                }}
                "#
            ),
        );

        let key = source_function_body_key(&consumer, "root");

        let summary = consumer
            .unwind_behavior(key.clone(), &consumer.state.cancellation)
            .expect("fixture callable must resolve its unwind behavior");

        assert!(
            !summary.result().diagnostics().has_errors(),
            "{:?}",
            summary.result().diagnostics()
        );

        assert_eq!(summary.result().value().may_unwind(), expected, "{callee}");

        assert_eq!(
            summary.result().value().current_run_cancellation(),
            cancellation,
            "{callee}"
        );

        let lowered = consumer
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );
    }
}

#[test]
fn imported_generic_addresses_retain_authority_only_while_the_owner_lives() {
    let provider = compilation(
        r#"
        module api;
        func forward<T>(pos pointer: DataPointer<T>) -> DataPointer<T>
            executes(pure, total)
        { return pointer; }
    "#,
    );

    for (body, valid) in [
        (
            r#"
            let value: u8 = 1;
            let pointer = example.package.api.forward<u8>(core.memory.address_of<u8>(&value));
            return core.memory.read<u8>(pointer);
        "#,
            true,
        ),
        (
            r#"
            let mut pointer = core.memory.null<u8>();
            {
                let value: u8 = 1;
                pointer = example.package.api.forward<u8>(core.memory.address_of<u8>(&value));
            };
            return core.memory.read<u8>(pointer);
        "#,
            false,
        ),
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
            trusted module app;

            using core.memory;
            using example.package.api;

            trusted func caller() -> u8 uses(raw_memory) {{ {body} }}
        "#
            ),
        );

        let diagnostics = consumer.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{body}: {diagnostics:?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_reference_replacements_preserve_both_identity_and_lifetime() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T> { mut reference: &T; }
        func replace<T>(pos target: &mut Holder<T>, pos input: &T) { target.reference = input; }
        func forward<T>(pos first: &T, pos second: &T) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            replace(&mut result, second);
            return result;
        }
        func choose<T>(pos first: &T, pos second: &T, pos select: bool) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            if select { replace(&mut result, second); }
            return result;
        }
        func set_choice<T>(pos target: &mut Holder<T>, pos first: &T, pos second: &T, pos select: bool) {
            if select { target.reference = first; }
            else { target.reference = second; }
        }
        func forward_choice<T>(pos first: &T, pos second: &T, pos select: bool) -> Holder<T> {
            let mut result: Holder<T> = { reference = first };
            set_choice(&mut result, first, second, select);
            return result;
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func caller(pos first: &bool, pos second: &bool) -> example.package.api.Holder<bool> {
            let mut result: example.package.api.Holder<bool> = { reference = first };
            example.package.api.replace(&mut result, second);
            return result;
        }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let template =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    assert!(
        template
            .borrows()
            .iter()
            .any(|relation| relation.exit().subject_root()
                == bray_symbols::DependencySubjectRoot::Result
                && relation.entry().expect("input origin").subject_root()
                    == bray_symbols::DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(1)
                    )),
        "{template:#?}"
    );

    let escaping = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api;
        func bad(pos caller: &bool) -> example.package.api.Holder<bool> {
            let local = true;
            return example.package.api.forward(caller, &local);
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        escaping.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
    );

    for function in ["choose", "forward_choice"] {
        let caller = format!(
            r#"
            module app;
            using example.package.api;
            func bad(pos caller: &bool, pos select: bool) -> example.package.api.Holder<bool> {{
                let local = true;
                return example.package.api.{function}(caller, &local, select);
            }}
        "#
        );

        let escaping = execution_consumer(&provider, &caller);

        bray_testing::assert_goal_state_diagnostic_kind(
            escaping.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}
#[test]
fn imported_field_addresses_keep_the_projection_from_the_original_reference() {
    let provider = compilation(
        r#"
        module api;
        struct Holder<T> { reference: &T; }
        func address<T>(pos holder: &Holder<T>) -> &&T { return &holder.reference; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Holder;
        using example.package.api.address;
        func caller(pos holder: &example.package.api.Holder<bool>) -> &&bool { return example.package.api.address(holder); }
    "#,
    );

    let flow = consumer
        .storage_flow(source_function_body_key(&consumer, "caller"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

    let borrows = flow
        .value()
        .dependencies()
        .borrows();

    let references = borrows
        .iter()
        .filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .collect::<Vec<_>>();

    assert_eq!(references.len(), 1, "{references:?}");

    assert!(
        references[0]
            .entry()
            .expect("input origin")
            .projections()
            .is_empty()
    );

    assert!(matches!(
        references[0].target_projections(),
        [bray_symbols::DependencyProjection::ProductField(_)]
    ));
}

#[test]
fn imported_generic_protected_payload_addresses_retain_their_storage_projection() {
    let provider = compilation(
        r#"
        module api;

        using core.memory;
        func pointer<T>(pos storage: &Uninit<T>) -> DataPointer<T> {
            return core.memory.uninit_pointer<T>(storage);
        }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.pointer;
        func caller(pos storage: &Uninit<bool>) -> DataPointer<bool> {
            return example.package.api.pointer<bool>(storage);
        }
    "#,
    );

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    let relations = dependencies.borrows();

    assert_eq!(relations.len(), 1, "{dependencies:#?}");

    assert_eq!(
        relations[0].kind(),
        bray_symbols::DependencyReferenceKind::Address
    );

    assert_eq!(
        relations[0].target_projections(),
        &[bray_symbols::DependencyProjection::UninitPayload]
    );
}

#[test]
fn imported_nested_reference_layers_survive_generic_specialization() {
    let provider = compilation(
        r#"
        module api;
        struct Cell<T> { value: T; }
        func value<T>(pos outer: &&Cell<T>) -> &T { return &outer.value; }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Cell;
        using example.package.api.value;
        func forward(pos outer: &&example.package.api.Cell<bool>) -> &bool { return example.package.api.value<bool>(outer); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "forward");

    assert_eq!(dependencies.borrows().len(), 1, "{dependencies:#?}");

    assert_eq!(
        dependencies.borrows()[0].dereferences(),
        1,
        "{dependencies:#?}"
    );

    assert_eq!(
        dependencies.borrows()[0].target_projections().len(),
        1,
        "{dependencies:#?}"
    );
}

#[test]
fn imported_recursive_results_preserve_normal_completion_and_exact_origin() {
    let provider = compilation(
        r#"
        module api;
        func forward<T>(pos value: &T, pos again: bool) -> &T {
            if again { return forward<T>(value, false); }
            return value;
        }
        func forever<T>(pos value: &T) -> &T { return forever<T>(value); }
    "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.forward;
        using example.package.api.forever;
        func good(pos value: &bool) -> &bool { return example.package.api.forward<bool>(value, true); }
        func diverging(pos value: &bool) -> &bool { return example.package.api.forever<bool>(value); }
    "#,
    );

    assert!(
        !consumer.check_diagnostics().has_errors(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let good =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "good");

    assert!(good.normal_completion());
    assert_eq!(good.borrows().len(), 1, "{good:#?}");

    assert_eq!(
        good.borrows()[0]
            .entry()
            .expect("input origin")
            .subject_root(),
        bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
    );

    let diverging = crate::compilation::result_dependencies::source_function_dependencies(
        &consumer,
        "diverging",
    );

    assert!(!diverging.normal_completion());
    assert!(diverging.borrows().is_empty());
}

#[test]
fn imported_union_payload_dependencies_resolve_at_the_owning_boundary() {
    let provider = compilation(
        r#"
        module api;
        union Choice { Value(pos item: &bool); }
        func get(pos choice: Choice) -> &bool {
            match choice { case .Value(item) { return item; } }
        }
    "#,
    );

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (body, valid) in [
        (
            "func caller(pos value: &bool) -> &bool { return example.package.api.get(example.package.api.Choice.Value(value)); }",
            true,
        ),
        (
            "func caller() -> &bool { let value = true; return example.package.api.get(example.package.api.Choice.Value(&value)); }",
            false,
        ),
    ] {
        let source = format!(
            "module app; using example.package.api.Choice; using example.package.api.get; {body}"
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }
}

#[test]
fn generic_owner_forwarding_preserves_only_the_transferred_witness() {
    let forwarding = r#"
        module api;
        extern func fail() -> never;
        struct Box<T> { value: T; }
        struct Cell { mut flag: bool; }
        func scalar(pos value: bool) -> bool { return value; }
        func borrow<T>(pos value: &T) -> &T { return value; }
        func changed(pos value: Cell) -> Cell {
            let mut owner = value;
            owner.flag = false;
            return owner;
        }
        func identity<T>(pos value: T) -> T { return value; }
        func unwrap<T>(pos value: Box<T>) -> T { return value.value; }
        func present<T>(pos value: T?) -> T {
            match consume value {
                case ?item { return item; }
                case none { panic("absent"); }
            }
        }
        func success<T>(pos value: Result<T, bool>) -> T {
            match consume value {
                case Ok(item) { return item; }
                case Error(_) { fail(); }
            }
        }
        func choose<T>(pos first: T, pos second: T, pos choice: bool) -> T {
            if choice { return first; }
            return second;
        }
    "#;

    let provider = compilation(forwarding);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for name in ["scalar", "borrow"] {
        let dependencies =
            crate::compilation::result_dependencies::source_function_dependencies(&provider, name);

        assert!(
            dependencies.owned_result().is_empty(),
            "{name}: {dependencies:?}"
        );
    }

    let changed = crate::compilation::result_dependencies::source_function_dependencies(&provider, "changed");
    let transfers = changed.owned_result().guaranteed_transfers().into_iter().collect::<Vec<_>>();

    assert_eq!(transfers.len(), 1, "{changed:#?}");

    let values = provider.semantic_value_store().expect("provider semantic values");

    let input = values.intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(bray_symbols::SymbolOrdinal::new(0)))
        .expect("owned source is the first formal input");

    assert_eq!(transfers[0].input(), input, "{changed:#?}");
    assert!(transfers[0].output().is_empty(), "the same owner is returned at the root");
    assert!(transfers[0].preservation().preserves_value(), "unchanged root observations survive: {changed:#?}");

    let children = transfers[0].preservation().children();

    assert_eq!(children.len(), 1, "{changed:#?}");

    let (bray_symbols::ConstantProjectionKind::ProductField(flag), mask) = children.first_key_value().expect("changed flag override") else {
        panic!("changed owner retains its named field override: {changed:#?}");
    };

    let symbols = provider.symbol_graph().expect("provider symbols");

    assert_eq!(symbols.member_name((*flag).into()).map(|name| name.as_str()), Some("flag"));
    assert!(!mask.preserves_value(), "the written flag loses its old contents");

    for name in ["identity", "unwrap", "present", "success"] {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(&provider, name);

        assert!(!summary.owned_result().is_empty(), "{name} must publish its checked owner transfer: {summary:#?}");
    }

    for (body, failure) in [
        (
            "let owner = api.identity<Owner>(create()); observe(&owner);",
            None,
        ),
        (
            "let owner = api.identity<Owner>(api.identity<Owner>(create())); observe(&owner);",
            None,
        ),
        (
            "let wrapped: api.Box<Owner> = { value = create() }; let owner = api.unwrap<Owner>(wrapped); observe(&owner);",
            None,
        ),
        (
            "let owner = api.present<Owner>(create()); observe(&owner);",
            None,
        ),
        (
            "let wrapped: Result<Owner, bool> = Ok(create()); let owner = api.success<Owner>(wrapped); observe(&owner);",
            None,
        ),
        (
            "let owner = api.success<Owner>(create_result(true)); observe(&owner);",
            None,
        ),
        (
            "let owner = api.present<Owner>(create_optional(true)); observe(&owner);",
            None,
        ),
        (
            "let owner = api.identity<Owner>(Owner { marker = Marker {} }); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
        (
            "let owner = create(); let forged = api.identity<Owner>(Owner { marker = owner.marker }); observe(&forged);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let owner = api.unwrap<Owner>(create_box()); observe(&owner);",
            None,
        ),
        (
            "let owner = api.unwrap<Owner>(create_box_carrier()); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven),
        ),
        (
            "let owner = api.choose<Owner>(create(), Owner { marker = Marker {} }, false); observe(&owner);",
            Some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
        ),
    ] {
        let declarations = format!(
            r#"
            trusted module app;
            using api;
            struct Marker {{}}
            struct Owner {{ marker: Marker; destruct() {{}} }}
            trusted predicate live(value: &Marker);
            trusted func create() -> Owner ensures(trusted live(&result.marker)) {{
                return Owner {{ marker = Marker {{}} }};
            }}
            func create_result(pos succeeds: bool) -> Result<Owner, bool>
                ensures(!(result matches Ok(_)) || trusted live(result_marker(&result))) {{
                if succeeds {{ return Ok(create()); }}
                return Error(false);
            }}
            const func result_marker(pos value: &Result<Owner, bool>) -> &Marker
                requires(value matches Ok(_)) executes(pure, total) {{
                match value {{
                    case Ok(owner) {{ return &owner.marker; }}
                    case Error(_) {{ panic("absent"); }}
                }}
            }}
            func create_optional(pos succeeds: bool) -> Owner?
                ensures(!(result matches ?_) || trusted live(optional_marker(&result))) {{
                if succeeds {{ let owner = create(); return owner; }}
                return none;
            }}
            const func optional_marker(pos value: &(Owner?)) -> &Marker
                requires(value matches ?_) executes(pure, total) {{
                match value {{
                    case ?owner {{ return &owner.marker; }}
                    case none {{ panic("absent"); }}
                }}
            }}
            trusted func create_box() -> api.Box<Owner>
                ensures(trusted live(&result.value.marker)) {{
                return {{ value = Owner {{ marker = Marker {{}} }} }};
            }}
            trusted predicate boxed(value: &api.Box<Owner>);
            trusted func create_box_carrier() -> api.Box<Owner>
                ensures(trusted boxed(&result)) {{
                return {{ value = Owner {{ marker = Marker {{}} }} }};
            }}
            func observe(pos value: &Owner)
                requires(trusted live(&value.marker))
                ensures(trusted live(&value.marker)) executes(pure, total) {{}}
            func caller() {{ {body} }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &declarations
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([forwarding, &declarations])
            };

            let diagnostics = consumer.check_diagnostics();

            if let Some(failure) = failure {
                assert!(diagnostics.has_errors(), "imported={imported}, {body}: {diagnostics:#?}");
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, failure);
            } else {
                assert!(
                    !diagnostics.has_errors(),
                    "imported={imported}, {body}: {diagnostics:#?}"
                );
            }
        }
    }
}

#[test]
fn imported_task_owners_retain_dependencies_through_cleanup_and_consumption() {
    let provider = compilation(
        r#"
        module api;
        async func work(pos value: &u32)
        {
            if value == 2
            {
                panic("unexpected input");
            }
        }
        func pending(pos value: &u32) -> Future<unit> {
            return work(value);
        }
    "#,
    );

    for (body, valid) in [
        (
            "let owner = example.package.api.pending(&value).start(); value = 1;",
            false,
        ),
        (
            "_ = example.package.api.pending(&value).start(); value = 1;",
            false,
        ),
        (
            "let owner = example.package.api.pending(&value).start(); let moved = owner; value = 1;",
            false,
        ),
        (
            "let owner = example.package.api.pending(&value).start(); try await owner.join(); value = 1;",
            true,
        ),
        (
            "{ let owner = example.package.api.pending(&value).start(); } value = 1;",
            true,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api.pending;
            async func caller() {{
                let mut value: u32 = 0;
                {body}
            }}
        "#
        );

        let consumer = execution_consumer(&provider, &source);

        let flow = consumer
            .storage_flow(source_function_body_key(&consumer, "caller"))
            .expect("fixture callable must complete storage-flow analysis");

        if valid {
            assert!(
                !flow.diagnostics().has_errors(),
                "{body}: {:?}",
                flow.diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow,
            );
        }
    }
}

#[test]
fn terminal_destructor_checks_imported_generic_replacement_types() {
    let provider = compilation(
        r#"
        module api;

        func replace<T>(pos destination: &mut T, pos value: T)
        {
            destination = value;
        }
        "#,
    );

    let provider_diagnostics = provider.check_diagnostics();

    assert!(
        !provider_diagnostics.has_errors(),
        "{provider_diagnostics:?}"
    );

    let flow = provider
        .storage_flow(source_function_body_key(&provider, "replace"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .iter()
            .any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Replace(_),
                    ..
                }
            )),
        "{:?}",
        flow.value().dependencies().effects()
    );

    for (declaration, value, rejected) in [
        ("struct Child { finalize() {} }", "Child {}", true),
        ("struct Child { destruct() {} }", "Child {}", false),
    ] {
        let source = format!(
            r#"
            module app;
            using example.package.api.replace;

            {declaration}

            struct Owner
            {{
                mut child: Child;

                destruct()
                {{
                    example.package.api.replace(&mut self.child, {value});
                }}
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        if rejected {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingDestructorOwnershipIntroduction,
            );
        } else {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
fn imported_unknown_call_effects_do_not_claim_only_retirement() {
    let provider = compilation(
        r#"
        module api;

        extern func alter<T>(pos value: &mut T);

        func forward<T>(pos value: &mut T)
        {
            alter(value);
        }
        "#,
    );

    let flow = provider
        .storage_flow(source_function_body_key(&provider, "forward"))
        .expect("fixture callable must complete storage-flow analysis");

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .contains(&bray_symbols::DependencyStorageEffect::Ambient)
    );

    assert!(
        flow.value()
            .dependencies()
            .effects()
            .iter()
            .any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
            ))
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api.forward;

        struct Owner
        {
            mut value: i32;

            finalize() {}

            destruct()
            {
                example.package.api.forward(&mut self.value);
            }
        }
        "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        consumer.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingUnprovedDestructorEffects,
    );
}

#[test]
fn terminal_destructor_can_clear_an_owner_through_an_imported_generic_helper() {
    let provider = compilation(
        r#"
        module api;

        func clear<T>(pos destination: &mut T?)
        {
            destination = none;
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api.clear;

        struct Child
        {
            finalize() {}
        }

        struct Owner
        {
            mut child: Child?;

            destruct()
            {
                example.package.api.clear(&mut self.child);
            }
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn imported_escape_effects_distinguish_retained_arguments_from_opaque_execution() {
    let provider = compilation(
        r#"
        trusted module api;

        using core.memory;

        struct Marker
        {
        }

        struct Owner
        {
            marker: Marker;
            mut value: u32;
        }

        trusted predicate live(value: &Marker);

        extern trusted func opaque();

        extern trusted func publish(pos pointer: DataPointer<Owner>);

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return
            {
                marker = Marker
                {
                },
                value = 0
            };
        }

        trusted func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            opaque();

            let certify = trusted lambda(pos marker: &Marker)
                ensures(trusted live(marker))
                executes(pure, total) {};

            certify(&owner.marker);
        }

        trusted func mutate(pos owner: &mut Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
            uses(raw_memory, unchecked_init)
        {
            let pointer = core.memory.address_of_mut<u32>(&mut owner.value);

            trusted core.memory.write<u32>(pointer, 1);
        }

        trusted func expose(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            ensures(trusted live(&owner.marker))
        {
            publish(core.memory.address_of<Owner>(owner));

            let certify = trusted lambda(pos marker: &Marker)
                ensures(trusted live(marker))
                executes(pure, total) {};

            certify(&owner.marker);
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total) {}
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for (operation, valid) in [("inspect", true), ("mutate", true), ("expose", false)] {
        let borrow = if operation == "mutate" {
            "&mut owner"
        } else {
            "&owner"
        };

        let source = format!(
            r#"
            module app;
            using example.package.api.make;
            using example.package.api.{operation};
            using example.package.api.opaque;
            using example.package.api.observe;

            func caller()
            {{
                let mut owner = example.package.api.make();
                example.package.api.{operation}({borrow});
                example.package.api.opaque();
                example.package.api.observe(&owner);
            }}
            "#
        );

        let consumer = execution_consumer(&provider, &source);
        let diagnostics = consumer.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{operation}: {diagnostics:?}"
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
fn returned_address_fields_keep_their_origin_through_anonymous_factories() {
    let provider = compilation(
        r#"
        module api;

        using core.atomic;

        struct Owner
        {
            pointer: DataPointer<u32>;
        }

        func forward(pos owner: &Owner, pos counter: &core.atomic.Atomic<usize>) -> Owner
        {
            _ = core.atomic.fetch_add<usize, 0>(counter, 1);

            let make = lambda(pos pointer: DataPointer<u32>) -> Owner
            {
                return
                {
                    pointer = pointer
                };
            };

            return make(owner.pointer);
        }
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "forward");

    let borrows = dependencies
        .borrows();

    let returned = borrows
        .iter()
        .filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        })
        .collect::<Vec<_>>();

    assert_eq!(returned.len(), 1, "{dependencies:#?}");

    assert_eq!(
        returned[0].kind(),
        bray_symbols::DependencyReferenceKind::Address
    );
}

#[test]
fn returned_union_payload_addresses_remain_bounded_after_extraction() {
    let provider = compilation(
        r#"
        trusted module api;

        struct Marker
        {
        }

        struct Owner
        {
            marker: Marker;
            pointer: DataPointer<u32>;
        }

        trusted predicate live(marker: &Marker);

        extern func pointer() -> DataPointer<u32>;

        extern func touch(pos pointer: DataPointer<u32>);

        func make() -> Result<Owner, bool>
            ensures(!(result matches Ok(_)) || trusted live(result_marker(&result)))
        {
            let make_owner = trusted lambda(pos pointer: DataPointer<u32>) -> Owner
                ensures(trusted live(&result.marker))
            {
                return Owner
                {
                    marker = Marker
                    {
                    },
                    pointer = pointer
                };
            };

            return Ok(make_owner(pointer()));
        }

        const func result_marker(pos value: &Result<Owner, bool>) -> &Marker
            requires(value matches Ok(_))
            executes(pure, total)
        {
            match value
            {
                case Ok(owner) { return &owner.marker; }
                case Error(_) { panic("absent"); }
            }
        }

        func copy_pointer(pos owner: &Owner) -> DataPointer<u32>
        {
            return owner.pointer;
        }

        func observe(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total) {}
        "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;

        using example.package.api;

        func caller() -> Result<unit, bool>
        {
            let owner = try example.package.api.make();

            example.package.api.touch(example.package.api.copy_pointer(&owner));
            example.package.api.observe(&owner);
            return Ok(unit);
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

#[test]
fn propagation_preserves_the_selected_payload_reference_relation() {
    for (ty, input, result, count) in [
        ("DataPointer<u32>?", "input?", "pointer", 1),
        ("Wrapper?", "input?", "pointer", 1),
        ("Outer?", "input?", "pointer", 1),
        ("Result<Wrapper, bool>", "try input", "Ok(pointer)", 1),
        ("Result<Outer, Wrapper>", "try input", "Ok(pointer)", 2),
        (
            "Result<DataPointer<u32>, bool>",
            "try input",
            "Ok(pointer)",
            1,
        ),
        (
            "Result<DataPointer<u32>, DataPointer<u32>>",
            "try input",
            "Ok(pointer)",
            2,
        ),
    ] {
        for yield_region in [false, true] {
            let body = if yield_region {
                let yielded = if ty.ends_with('?') {
                    format!("yield {result};")
                } else {
                    format!("let value: {ty} = {result};\nyield value;")
                };

                format!(
                    r#"
                    let forwarded: {ty} =
                    {{
                        let pointer = {input};
                        {yielded}
                    }};
                    return forwarded;
                "#
                )
            } else {
                format!(
                    r#"
                    let pointer = {input};
                    return {result};
                "#
                )
            };

            let provider = compilation(&format!(
                r#"
                module api;

                struct Wrapper
                {{
                    pointer: DataPointer<u32>;
                }}

                struct Outer
                {{
                    inner: Wrapper;
                }}

                func forward(pos input: {ty}) -> {ty}
                {{
                    {body}
                }}
                "#
            ));

            let diagnostics = provider.check_diagnostics();

            assert!(
                !diagnostics.has_errors(),
                "{ty}, yield={yield_region}: {diagnostics:?}"
            );

            let dependencies =
                crate::compilation::result_dependencies::source_function_dependencies(
                    &provider, "forward",
                );

            let borrows = dependencies
                .borrows();

            let returned = borrows
                .iter()
                .filter(|relation| {
                    relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
                })
                .collect::<Vec<_>>();

            assert_eq!(
                returned.len(),
                count,
                "{ty}, yield={yield_region}: {dependencies:#?}"
            );

            for relation in returned {
                assert_eq!(
                    relation.kind(),
                    bray_symbols::DependencyReferenceKind::Address
                );

                assert_eq!(
                    relation.entry().expect("input origin").projections(),
                    relation.exit().projections()
                );
            }
        }
    }
}

#[test]
fn compound_assignment_preserves_operator_effects_at_terminal_and_imported_boundaries() {
    let declarations = r#"
        module api;

        struct Child
        {
            finalize() {}
        }

        struct Value
        {
            child: DataPointer<Child>;
            fail: bool;
        }

        extern func replace(pos child: DataPointer<Child>);

        impl ValueAdd = Value(Add<Value>)
        {
            type Output = Value;

            func add(pos rhs: &Value) -> Value
            {
                replace(self.child);

                if rhs.fail
                {
                    assert(false);
                }

                return Value
                {
                    child = rhs.child,
                    fail = false
                };
            }
        }

        struct Container
        {
            mut value: Value;
        }

        func update(pos container: &mut Container)
        {
            container.value += container.value;
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "update");

    assert!(
        dependencies
            .effects()
            .iter()
            .any(|effect| matches!(effect, bray_symbols::DependencyStorageEffect::Ambient)),
        "the operator's opaque effects must survive its scalar-only replacement: {dependencies:#?}"
    );

    for imported in [false, true] {
        for operation in [
            "self.container.value += self.container.value;",
            "update(&mut self.container);",
        ] {
            let owner = format!(
                r#"
                struct Owner
                {{
                    mut container: Container;

                    finalize() {{}}

                    destruct()
                    {{
                        {operation}
                    }}
                }}
            "#
            );

            let consumer = if imported {
                let owner = owner
                    .replace("Container", "example.package.api.Container")
                    .replace("update(", "example.package.api.update(");

                execution_consumer(
                    &provider,
                    &format!(
                        r#"
                    module app;
                    using example.package.api.Container;
                    using example.package.api.update;
                    using example.package.api.ValueAdd;

                    {owner}
                "#
                    ),
                )
            } else {
                compilation(&format!("{declarations}\n{owner}"))
            };

            let diagnostics = consumer.check_diagnostics();

            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingUnprovedDestructorEffects,
            );
        }
    }
}

#[test]
fn unresolved_assignment_retains_stored_reference_capture() {
    let declarations = r#"
        module api;

        struct Slot
        {
            mut pointer: DataPointer<bool>;
        }

        func choose(pos left: &mut Slot, pos right: &mut Slot, pos first: bool) -> &mut Slot
        {
            if first
            {
                return left;
            }
            return right;
        }

        func store(pos left: &mut Slot, pos right: &mut Slot, pos pointer: DataPointer<bool>, pos first: bool)
        {
            let destination = choose(left, right, first);
            destination.pointer = pointer;
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for imported in [false, true] {
        let body = r#"
            extern func opaque();

            func forward(pos left: &mut Slot, pos right: &mut Slot, pos pointer: DataPointer<bool>, pos first: bool)
            {
                store(left, right, pointer, first);
                opaque();
            }
        "#;

        let consumer = if imported {
            let body = body
                .replace("Slot", "example.package.api.Slot")
                .replace("store(", "example.package.api.store(");

            execution_consumer(
                &provider,
                &format!(
                    "module app; using example.package.api.Slot; using example.package.api.store; {body}"
                ),
            )
        } else {
            compilation(&format!("{declarations}\n{body}"))
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "forward",
        );

        assert!(
            dependencies.effects().iter().any(|effect| matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Indirect {
                    kind: bray_symbols::DependencyMutationKind::Escape,
                    ..
                }
            )),
            "stored references remain exposed across an opaque call; imported={imported}: {dependencies:#?}"
        );
    }
}

#[test]
fn copied_parameter_storage_does_not_escape_through_source_or_imported_calls() {
    let declarations = r#"
        module api;

        @copy
        struct Metadata
        {
            value: bool;
            namespace: [u64; 4];
        }

        extern func expose(pos value: &bool);

        func scalar(pos value: bool)
        {
            expose(&value);
        }

        func aggregate(pos value: Metadata)
        {
            expose(&value.value);
        }

        extern func accept(pos value: Metadata);

        extern func accept_fields(pos value: bool, pos namespace: [u64; 4]);

        func projected(pos value: &Metadata)
        {
            accept_fields(value.value, value.namespace);
        }

        func forwarded(pos value: Metadata)
        {
            accept(value);
        }

        func reference(pos value: &Metadata)
        {
            expose(&value.value);
        }
    "#;

    let provider = compilation(declarations);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:?}");

    for imported in [false, true] {
        for (name, argument) in [
            ("scalar", "value.value"),
            ("aggregate", "value"),
            ("forwarded", "value"),
            ("projected", "&value"),
            ("reference", "&value"),
        ] {
            let body = format!(
                r#"
                func caller(pos value: Metadata)
                {{
                    {name}({argument});
                }}
                "#
            );

            let consumer = if imported {
                let body = body
                    .replace("Metadata", "example.package.api.Metadata")
                    .replace(&format!("{name}("), &format!("example.package.api.{name}("));

                execution_consumer(
                    &provider,
                    &format!(
                        "module app; using example.package.api.Metadata; using example.package.api.{name}; {body}"
                    ),
                )
            } else {
                compilation(&format!("{declarations}\n{body}"))
            };

            let contracts = consumer
                .dependency_contracts(source_function_body_key(&consumer, "caller"))
                .expect("fixture `dependency_contracts` call must succeed");

            assert!(
                !contracts.diagnostics().has_errors(),
                "{:?}",
                contracts.diagnostics()
            );

            let calls = contracts.value().calls();

            let effects = calls
                .iter()
                .find_map(|(site, call)| matches!(site, bray_bound_tree::BoundExecutionSite::Node(_)).then_some(call))
                .expect("fixture iterator must provide its expected item")
                .effects();

            assert!(
                !effects.iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Direct { .. }
                )),
                "a copied parameter's inline storage does not belong to its caller: imported={imported}, {name}: {effects:?}"
            );

            assert_eq!(
                effects.iter().any(|effect| matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                )),
                name == "reference",
                "borrowed referent escapes must remain visible: imported={imported}, {name}: {effects:?}"
            );
        }
    }
}

#[test]
fn composite_conversion_effects_use_the_aggregate_input_paths() {
    use bray_symbols::{
        DependencyProjection, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal,
    };

    for (input, output, expected) in [
        (
            "(Source, Source)",
            "(Target, Target)",
            vec![
                DependencyProjection::TupleElement(SymbolOrdinal::new(0)),
                DependencyProjection::TupleElement(SymbolOrdinal::new(1)),
            ],
        ),
        (
            "Source?",
            "Target?",
            vec![DependencyProjection::NullableValue],
        ),
        ("[Source; 2]", "[Target; 2]", vec![]),
    ] {
        let source = r#"
        module api;
        struct Value
        {
            mut active: bool;
        }
        struct Source
        {
            value: &mut Value;
        }
        struct Target {}
        impl Conversion = Source(ConvertTo<Target>)
        {
            consume func convert() -> Target
            {
                let mut reference = self.value;
                reference.active = true;
                return Target {};
            }
        }
        func caller(pos value: Input) -> Output
        {
            return value as Output;
        }
        "#
        .replace("Input", input)
        .replace("Output", output);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");

        let consumer = execution_consumer(
            &application,
            &format!(
                "module app; using example.package.api.Source; using example.package.api.Target; using example.package.api.caller; func forward(pos value: {}) -> {} {{ return example.package.api.caller(value); }}",
                input.replace("Source", "example.package.api.Source"),
                output.replace("Target", "example.package.api.Target")
            ),
        );

        let imported_diagnostics = consumer.check_diagnostics();

        assert!(
            !imported_diagnostics.has_errors(),
            "{imported_diagnostics:#?}"
        );

        for (application, name) in [(&application, "caller"), (&consumer, "forward")] {
            let key = source_function_body_key(application, name);

            let flow = application
                .storage_flow(key)
                .expect("fixture callable must complete storage-flow analysis");

            let summary = flow.value().dependencies().effects();

            let summary_roots = summary
                .iter()
                .filter_map(|effect| match effect {
                    DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(
                summary_roots.len(),
                expected.len(),
                "{name}: {source}: {summary:#?}"
            );

            for reference in summary_roots {
                assert_eq!(
                    reference.subject_root(),
                    DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))
                );

                assert!(
                    expected.contains(
                        reference
                            .projections()
                            .first()
                            .expect("fixture sequence must contain its first item")
                    ),
                    "{summary:#?}"
                );
            }

            assert_eq!(
                summary
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::UnboundedMutation { kind: bray_symbols::DependencyMutationKind::Replace(_), .. })),
                expected.is_empty(),
                "{name}: {source}: {summary:#?}"
            );
        }

        let key = crate::test_support::source_function_body_key(&application, "caller");

        let contracts = application
            .dependency_contracts(key)
            .expect("fixture `dependency_contracts` call must succeed");

        let effects = contracts
            .value()
            .calls()
            .values()
            .flat_map(|call| call.effects())
            .collect::<Vec<_>>();

        let indirect = effects
            .iter()
            .filter_map(|effect| match effect {
                DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
                _ => None,
            })
            .collect::<Vec<_>>();

        let actual = indirect
            .iter()
            .map(|reference| {
                assert_eq!(reference.subject_root(), DependencySubjectRoot::Receiver);

                *reference
                    .projections()
                    .first()
                    .expect("composite reference retains its aggregate prefix")
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected, "{source}: {effects:#?}");

        if expected.is_empty() {
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::UnboundedMutation { kind: bray_symbols::DependencyMutationKind::Replace(_), .. })),
                "{effects:#?}"
            );
        }
    }
}

#[test]
fn custom_index_effects_preserve_owned_and_borrowed_input_origins() {
    use bray_symbols::{
        DependencyProjection, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal,
    };

    let provider = compilation(
        r#"
        module api;
        struct Value { mut active: bool; }
        struct Indexed { mut output: i32; mut visits: usize; target: &mut Value; }
        impl IndexAccess = Indexed(MutableElementIndex<i32>)
        {
            type Output = i32;
            mut func index(pos selector: &i32) -> &mut i32
            {
                self.visits = 1;
                self.target.active = true;
                return &mut self.output;
            }
        }
        func owned(pos mut indexed: Indexed) { _ = &mut indexed[0]; }
        func borrowed(pos indexed: &mut Indexed) { _ = &mut indexed[0]; }
    "#,
    );

    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.Indexed;
        using example.package.api.owned;
        using example.package.api.borrowed;
        func owned(pos indexed: example.package.api.Indexed) { example.package.api.owned(indexed); }
        func borrowed(pos indexed: &mut example.package.api.Indexed) { example.package.api.borrowed(indexed); }
    "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for application in [&provider, &consumer] {
        for name in ["owned", "borrowed"] {
            let flow = application
                .storage_flow(source_function_body_key(application, name))
                .expect("fixture callable must complete storage-flow analysis");

            let effects = flow.value().dependencies().effects();

            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, DependencyStorageEffect::Unknown)),
                "{name}: {effects:#?}"
            );

            assert!(effects.iter().any(|effect| matches!(effect,
                DependencyStorageEffect::Indirect { reference, .. }
                if reference.subject_root() == DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))
                    && matches!(reference.projections(), [DependencyProjection::ProductField(_)])
            )), "{name}: {effects:#?}");
        }
    }
}

#[test]
fn imported_calls_distinguish_normal_preservation_from_caught_failure_effects() {
    let api = r#"
        module api;

        struct Cell
        {
            mut value: bool;
        }

        func check(pos fail: bool)
        {
            if fail
            {
                panic("failed");
            }
        }

        func change(pos cell: &mut Cell)
        {
            cell.value = false;
        }

        func change_then_fail(pos cell: &mut Cell, pos fail: bool)
        {
            if fail
            {
                cell.value = false;
                panic("failed");
            }
        }

        func change_on_success(pos cell: &mut Cell, pos fail: bool)
        {
            if fail
            {
                panic("failed");
            }

            cell.value = false;
        }

        func stop(pos choose: bool, pos left: &mut Cell, pos right: &mut Cell)
        {
            if choose
            {
                left.value = false;
                panic("left");
            }

            right.value = false;
            panic("right");
        }

        func forward_stop(pos choose: bool, pos left: &mut Cell, pos right: &mut Cell)
        {
            stop(choose, left, right);
        }
    "#;

    let branches = (0..8).map(|code| format!(r#"
        if code == {code} {{ left.value = false; panic("left"); }}
    "#)).collect::<String>();

    let api = format!(r#"
        {api}
        func stop_many(pos code: i32, pos left: &mut Cell, pos right: &mut Cell)
        {{
            {branches}
            right.value = false;
            panic("right");
        }}
    "#);

    let source = r#"
        module app;
        using example.package.api;

        func forward(pos fail: bool)
        {
            example.package.api.check(fail);
        }

        func mutate(pos cell: &mut example.package.api.Cell)
        {
            example.package.api.change(cell);
        }

        func preserve(pos cell: &mut example.package.api.Cell, pos fail: bool)
        {
            example.package.api.change_then_fail(cell, fail);
        }

        func recover(pos cell: &mut example.package.api.Cell, pos fail: bool)
        {
            _ = catch
            {
                example.package.api.change_then_fail(cell, fail);
            };
        }

        func recover_after_write(pos cell: &mut example.package.api.Cell)
        {
            _ = catch
            {
                example.package.api.change_then_fail(cell, true);
            };
        }

        func recover_before_write(pos cell: &mut example.package.api.Cell)
        {
            _ = catch
            {
                example.package.api.change_on_success(cell, true);
            };
        }

        func recover_left(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
        {
            _ = catch { example.package.api.forward_stop(true, left, right); };
        }

        func recover_right(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
        {
            _ = catch { example.package.api.forward_stop(false, left, right); };
        }

        func recover_either(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell, pos choose: bool)
        {
            _ = catch { example.package.api.forward_stop(choose, left, right); };
        }

        func recover_last(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
        {
            _ = catch { example.package.api.stop_many(8, left, right); };
        }

        func recover_many(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell, pos code: i32)
        {
            _ = catch { example.package.api.stop_many(code, left, right); };
        }

        func require_true(pos value: bool) requires(value) {}

        func check_untouched(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
            requires(right.value)
        {
            let handled = catch { example.package.api.forward_stop(true, left, right); };
            require_true(right.value);
        }

        func check_replaced(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
            requires(right.value)
        {
            let handled = catch { example.package.api.forward_stop(false, left, right); };
            require_true(right.value);
        }

        func check_written(pos left: &mut example.package.api.Cell, pos right: &mut example.package.api.Cell)
        {
            _ = catch { example.package.api.forward_stop(false, left, right); };
            require_true(!right.value);
        }
    "#;

    let provider = compilation(&api);
    let many = crate::compilation::result_dependencies::source_function_dependencies(&provider, "stop_many");

    assert_eq!(many.failure_outcomes().expect("defined callable publishes its failure rows").len(), 9);

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, source)
        } else {
            compilation_from_sources([api.as_str(), &source.replace("example.package.api", "api")])
        };

        assert!(!consumer.check_diagnostics().has_errors(), "{:?}", consumer.check_diagnostics());

        for (name, preserved) in [
            ("forward", true),
            ("mutate", false),
            ("preserve", true),
            ("recover", false),
            ("recover_after_write", false),
        ] {
            let dependencies =
                crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            if matches!(name, "mutate" | "preserve" | "recover" | "recover_after_write") {
                assert!(!dependencies.effects().is_empty(), "{name}: {dependencies:?}");
            }

            assert_eq!(dependencies.preserves_storage_on_normal_return(), preserved, "{name}, imported={imported}: {dependencies:?}");
        }

        for (name, writes) in [
            ("recover_before_write", &[][..]),
            ("recover_after_write", &[0][..]),
            ("recover_left", &[0][..]),
            ("recover_right", &[1][..]),
            ("recover_either", &[0, 1][..]),
            ("recover_last", &[1][..]),
            ("recover_many", &[0, 1][..]),
        ] {
            let dependencies =
                crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            let replaced_inputs = dependencies.normal_effects().iter().filter_map(|effect| match effect {
                bray_symbols::DependencyStorageEffect::Indirect {
                    reference,
                    projections,
                    kind: bray_symbols::DependencyMutationKind::Replace(_),
                    ..
                } if matches!(&**projections, [bray_symbols::DependencyProjection::ProductField(_)]) => {
                    match reference.subject_root() {
                        bray_symbols::DependencySubjectRoot::Parameter(ordinal) => Some(ordinal),
                        _ => None,
                    }
                }
                _ => None,
            }).collect::<std::collections::BTreeSet<_>>();

            let expected = writes.iter().map(|&ordinal| bray_symbols::SymbolOrdinal::new(ordinal)).collect();

            assert_eq!(replaced_inputs, expected, "{name}, imported={imported}: {dependencies:?}");
        }

        let requirement = crate::test_support::source_function(&consumer, "require_true");

        for (name, proven) in [("check_untouched", true), ("check_replaced", false), ("check_written", true)] {
            let key = source_function_body_key(&consumer, name);
            let selections = consumer.semantic_selections(key.clone()).expect("caller has checked selections");
            let flow = consumer.storage_flow(key).expect("caller publishes its checked storage flow");

            let invocation = selections.value().entries().iter().find_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::Call(call) => match call.target() {
                    bray_bound_tree::BoundCallableTarget::Declaration(target)
                        if target.definition().callable_symbol() == requirement.into() => Some(entry.expression().into()),
                    _ => None,
                },
                _ => None,
            }).expect("caller reaches the chosen requirement check");

            assert_eq!(flow.value().preconditions_proven(invocation), proven, "{name}, imported={imported}");
        }
    }
}

#[test]
fn rejected_invocation_has_no_callee_effects_or_completion() {
    let api = r#"
        module api;

        struct Cell { value: bool; }

        func write(pos allowed: bool, pos cell: &mut Cell)
            requires(allowed)
        {
            cell.value = true;
        }
    "#;

    let provider = compilation(api);

    let caller = r#"
        module app;
        using example.package.api;

        func rejected(pos cell: &mut example.package.api.Cell)
        {
            example.package.api.write(false, cell);
        }

        func admitted(pos cell: &mut example.package.api.Cell)
        {
            example.package.api.write(true, cell);
        }
    "#;

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, caller)
        } else {
            compilation_from_sources([api, &caller.replace("example.package.api", "api")])
        };

        for (name, admitted) in [("rejected", false), ("admitted", true)] {
            let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            assert_eq!(summary.normal_completion(), admitted, "{name}, imported={imported}");
            assert_eq!(summary.effects().is_empty(), !admitted, "{name}, imported={imported}");

            assert_eq!(
                summary.failure_outcomes().expect("source caller publishes failure rows").iter()
                    .any(|row| row.kind == Some(bray_symbols::DependencyFailureKind::Panic)),
                !admitted,
                "{name}, imported={imported}",
            );
        }
    }
}

#[test]
fn total_destructor_has_no_failure_continuation() {
    let api = r#"
        module api;

        struct TotalGuard { destruct() executes(total) {} }
        struct PanickingGuard { destruct() { panic("cleanup"); } }
        struct Pair { keep: PanickingGuard; discard: TotalGuard; }
    "#;

    let source = r#"
        module app;
        using example.package.api;

        func release_total(pos guard: example.package.api.TotalGuard) {}
        func release_panicking(pos guard: example.package.api.PanickingGuard) {}
        func release_pair(pos pair: example.package.api.Pair) {}

        func transfer(pos keep: example.package.api.PanickingGuard, pos discard: example.package.api.TotalGuard)
            -> example.package.api.PanickingGuard
        {
            return keep;
        }

        func transfer_part(pos pair: example.package.api.Pair) -> example.package.api.PanickingGuard
        {
            return pair.keep;
        }

        func recover_total() -> Result<i32, PanicReport>
        {
            return catch { let guard = example.package.api.TotalGuard {}; yield 7; };
        }

        func recover_panicking() -> Result<i32, PanicReport>
        {
            return catch { let guard = example.package.api.PanickingGuard {}; yield 7; };
        }

        func recover_transfer() -> Result<example.package.api.PanickingGuard, PanicReport>
        {
            return catch
            {
                let guard = example.package.api.PanickingGuard {};
                yield guard;
            };
        }
    "#;

    let provider = compilation(api);

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, source)
        } else {
            compilation_from_sources([api, &source.replace("example.package.api", "api")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "imported={imported}: {diagnostics:?}");

        for name in ["release_total", "transfer", "transfer_part"] {
            let total = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            assert!(total.normal_completion());
            assert_eq!(total.failure_outcomes(), Some(&[][..]), "{name}, imported={imported}");
        }

        for name in ["release_panicking", "release_pair"] {
            let panicking = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            assert!(panicking.failure_outcomes().into_iter().flatten().any(|row|
                row.covers(bray_symbols::DependencyFailureKind::Panic) && row.report.is_some()
            ), "{name}, imported={imported}: {panicking:?}");
        }

        let variants = consumer.available_compiler_known_symbols()
            .result_representation().expect("fixture has the Result representation");

        for (name, may_panic) in [("recover_total", false), ("recover_transfer", false), ("recover_panicking", true)] {
            let key = source_function_body_key(&consumer, name);
            let result = consumer.lowered_unit(key).expect("catch around cleanup lowers");

            let mir = result.value().as_ref().and_then(bray_lowering::LoweredUnit::mir)
                .expect("catch around cleanup produces MIR");

            let reached = bray_ir::reachable_blocks(mir, [mir.entry()], &Default::default());

            let constructed = reached.iter().filter_map(|id| mir.block(*id))
                .flat_map(|block| block.operations()).filter_map(|id| mir.operation(*id))
                .filter_map(|operation| match operation.kind() {
                    bray_ir::MirOperationKind::Construct(construction) => match construction.target() {
                        bray_bound_tree::ConstructionTarget::UnionVariant(variant) => Some(variant),
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();

            if may_panic {
                assert!(constructed.contains(&variants.error_variant()), "{name}, imported={imported}");
            } else {
                assert_eq!(constructed, [variants.success_variant()], "{name}, imported={imported}");
            }
        }
    }
}

#[test]
fn cleanup_failure_retains_the_abandoned_return_report() {
    use bray_symbols::{DependencyFailureKind, DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal};

    let compilation = compilation(r#"
        module app;
        struct Incident { value: &bool; }
        struct Resource
        {
            value: &bool;
            enter() -> &bool { return self.value; }
            exit(pos lease: &bool) -> Result<unit, Incident>
            {
                return Error(Incident { value = lease });
            }
        }
        struct Returned
        {
            value: &bool;
            destruct()
            {
                let resource = Resource { value = self.value };
                _ = with _ = resource { panic("abandoned result"); };
            }
        }
        struct Guard { destruct() { panic("lexical cleanup"); } }
        struct Broken
        {
            enter() -> bool { return true; }
            exit(pos lease: bool) { panic("scoped cleanup"); }
        }
        struct Items {}
        struct PendingCursor
        {
            mut pending: bool;
            destruct() { panic("cursor cleanup"); }
        }
        impl &Items(Iterable)
        {
            type Element = bool;
            type Cursor = PendingCursor;
            consume func iterate() -> PendingCursor { return PendingCursor { pending = true }; }
        }
        impl PendingCursor(Iterator)
        {
            type Element = bool;
            mut func next() -> bool?
            {
                if !self.pending { return none; }
                self.pending = false;
                return true;
            }
        }
        func lexical(pos dependency: &bool) -> Returned
        {
            let guard = Guard {};
            return Returned { value = dependency };
        }
        func scoped(pos dependency: &bool) -> Returned
        {
            let resource = Broken {};
            with _ = resource { return Returned { value = dependency }; }
        }
        func propagated(pos dependency: &bool) -> Result<unit, Returned>
        {
            let resource = Broken {};
            with _ = resource
            {
                let failed: Result<unit, Returned> = Error(Returned { value = dependency });
                try failed;
            }
            panic("unreachable");
        }
        func iterated(pos dependency: &bool) -> Returned
        {
            let items = Items {};
            return for _ in items
            {
                break Returned { value = dependency };
            }
            else { break Returned { value = dependency }; };
        }
        func after_abandonment(pos left: &bool, pos right: &bool) -> (&bool, Result<bool, PanicReport>)
        {
            let outcome: Result<bool, PanicReport> = catch
            {
                let resource = Broken {};
                with _ = resource { return (left, Ok(false)); }
                yield true;
            };
            return (right, outcome);
        }
        func delivered(pos dependency: &bool) -> Returned
        {
            return Returned { value = dependency };
        }
    "#);

    let expected = DependencyRequirement::direct(
        DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0))),
        DependencyRequirementKind::ValueDependencies,
    );

    for name in ["lexical", "scoped", "propagated", "iterated", "after_abandonment", "delivered"] {
        let key = crate::test_support::source_function_body_key(&compilation, name);
        let lowered = compilation.lowered_unit(key).expect("return reaches lowering");

        assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "{name}: {lowered:#?}");

        let summary = crate::compilation::result_dependencies::source_function_dependencies(&compilation, name);
        let failures = summary.failure_outcomes().expect("body publishes its reached failures");

        if name == "delivered" || name == "after_abandonment" {
            assert!(summary.normal_completion() && failures.is_empty(), "{name}: {summary:#?}");

            let selected = SymbolOrdinal::new(u32::from(name == "after_abandonment"));

            assert!(summary.requirements().iter().any(|requirement| matches!(requirement,
                DependencyRequirement::Direct { subject, .. }
                    if subject.subject_root() == DependencySubjectRoot::Parameter(selected))), "{name}: {summary:#?}");

            assert!(summary.requirements().iter().all(|requirement| !matches!(requirement,
                DependencyRequirement::Direct { subject, .. }
                    if matches!(subject.subject_root(), DependencySubjectRoot::Parameter(parameter) if parameter != selected))),
                "{name}: {summary:#?}");
        } else {
            assert!(failures.iter().any(|row| {
                row.kind == Some(DependencyFailureKind::Panic)
                    && row.report.as_deref().is_some_and(|report| report.contains(&expected))
            }), "{name}: {failures:#?}");
        }
    }
}

#[test]
fn forwarded_panics_keep_only_the_selected_cleanup_report_dependencies() {
    use bray_symbols::{DependencyFailureKind, DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal};

    let api = r#"
        module api;

        struct Incident { value: &bool; }

        struct Resource
        {
            value: &bool;
            enter() -> &bool { return self.value; }
            exit(pos lease: &bool) -> Result<unit, Incident>
            {
                return Error(Incident { value = lease });
            }
        }

        Executionfunc produce(pos choose: bool, pos left: &bool, pos right: &bool)
        {
            if choose
            {
                let resource = Resource { value = left };
                with _ = resource { panic("left"); }
            }

            let resource = Resource { value = right };
            with _ = resource { panic("right"); }
        }

        Executionfunc forward(pos choose: bool, pos left: &bool, pos right: &bool)
        {
            Awaitproduce(choose, left, right);
        }
    "#;

    let source = r#"
        module app;
        using example.package.api;

        Executionfunc choose_left(pos left: &bool, pos right: &bool)
        {
            Awaitexample.package.api.forward(true, left, right);
        }

        Executionfunc choose_right(pos left: &bool, pos right: &bool)
        {
            Awaitexample.package.api.forward(false, left, right);
        }
    "#;

    for (execution, awaiting) in [("", ""), ("async ", "await ")] {
        let api = api.replace("Execution", execution).replace("Await", awaiting);
        let source = source.replace("Execution", execution).replace("Await", awaiting);
        let provider = compilation(&api);

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([&api, &source.replace("example.package.api", "api")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{execution}imported={imported}: {diagnostics:?}");

            for (name, ordinal) in [("choose_left", 0), ("choose_right", 1)] {
                let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

                let expected = DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(ordinal))),
                    DependencyRequirementKind::ValueDependencies,
                );

                assert!(!summary.normal_completion(), "{name}, {execution}imported={imported}: {summary:?}");

                let failures = summary.failure_outcomes().expect("checked forwarding publishes its failures");
                let panics = failures.iter().filter(|row| row.kind == Some(DependencyFailureKind::Panic));

                if execution.is_empty() {
                    assert_eq!(failures.len(), 2);
                    assert_eq!(panics.clone().count(), failures.len());
                    assert!(panics.clone().all(|row| row.effects.as_deref() == Some(&[][..])));
                }

                let reports = panics.map(|row| row.report.as_deref()).collect::<std::collections::BTreeSet<_>>();

                assert_eq!(reports, [Some(&[][..]), Some(&[expected][..])].into(),
                    "{name}, {execution}imported={imported}: {failures:?}");
            }
        }
    }
}

#[test]
fn awaited_abnormal_prefixes_follow_their_actual_exit_kind() {
    use bray_symbols::{DependencyFailureKind as Failure, DependencyMutationKind, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal};

    let api = r#"
        module api;

        struct Cell { mut value: bool; }

        async func terminate(pos panicked: &mut Cell, pos cancelled: &mut Cell, pos cancel: bool)
        {
            if cancel
            {
                cancelled.value = false;
                let outcome: RunResult<unit> = Cancelled;

                try outcome;
                return;
            }

            panicked.value = false;
            panic("reached the panic path");
        }
    "#;

    let source = r#"
        module app;

        using example.package.api;

        async func forward(pos panicked: &mut example.package.api.Cell, pos cancelled: &mut example.package.api.Cell, pos cancel: bool)
        {
            await example.package.api.terminate(panicked, cancelled, cancel);
        }
    "#;

    let provider = compilation(api);

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, source)
        } else {
            compilation_from_sources([api, &source.replace("example.package.api", "api")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "imported={imported}: {diagnostics:?}");

        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(&consumer, "forward");

        assert!(!dependencies.normal_completion(), "imported={imported}: {dependencies:?}");

        let failures = dependencies.failure_outcomes().expect("a checked body publishes its reached failure rows");

        for (kind, input) in [(Failure::Panic, 0), (Failure::Cancellation, 1)] {
            let mut written = std::collections::BTreeSet::new();

            for row in failures.iter().filter(|row| row.kind == Some(kind)) {
                assert_eq!(row.report.is_some(), kind == Failure::Panic, "imported={imported}: {row:?}");

                for effect in row.effects.as_deref().expect("a reached exit retains its performed effects") {
                    if let DependencyStorageEffect::Indirect { reference, kind: DependencyMutationKind::Replace(_), .. } = effect {
                        written.insert(reference.subject_root());
                    }
                }
            }

            assert_eq!(written, [DependencySubjectRoot::Parameter(SymbolOrdinal::new(input))].into(),
                "imported={imported}, {kind:?}: {failures:?}");
        }
    }
}

#[test]
fn task_observation_keeps_each_result_with_its_performed_writes() {
    use bray_symbols::{DependencyMutationKind, DependencyStorageEffect, DependencySubjectRoot, SymbolOrdinal};
    use std::collections::{BTreeMap, BTreeSet};

    let api = r#"
        module api;

        struct Cell { mut value: bool; }

        async func run(pos first: &mut Cell, pos second: &mut Cell, pos panicked: &mut Cell, pos cancelled: &mut Cell, pos decision: u32) -> u32
        {
            if decision == 0
            {
                first.value = false;
                return 7;
            }
            if decision == 1
            {
                second.value = false;
                return 11;
            }
            if decision == 2
            {
                panicked.value = false;
                panic("child panicked");
            }

            cancelled.value = false;
            let outcome: RunResult<unit> = Cancelled;

            try outcome;
            return 13;
        }
    "#;

    let provider = compilation(api);

    for method in ["join", "cancel"] {
        let source = format!(r#"
            module app;
            using example.package.api;

            async func observe(
                pos first: &mut example.package.api.Cell, pos second: &mut example.package.api.Cell,
                pos panicked: &mut example.package.api.Cell, pos cancelled: &mut example.package.api.Cell, pos decision: u32
            ) -> RunResult<u32>
            {{
                let task = example.package.api.run(first, second, panicked, cancelled, decision).start();

                return await task.{method}();
            }}
        "#);

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([api, &source.replace("example.package.api", "api")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert!(diagnostics.is_empty(), "{method}, imported={imported}: {diagnostics:?}");

            let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, "observe");

            let representation = consumer.available_compiler_known_symbols().run_result_representation()
                .expect("the fixture has the checked RunResult representation");

            let mut writes_by_tag = BTreeMap::<_, BTreeSet<_>>::new();

            for row in summary.captured_values() {
                let result = row.bindings.iter().find(|(subject, _)| {
                    subject.subject_root() == DependencySubjectRoot::Result && subject.projections().is_empty()
                }).map(|(_, value)| value).expect("each normal row publishes its task observation");

                let mut tags = result.variants();
                let tag = *tags.next().expect("each row retains its actual tag");

                assert!(tags.next().is_none(), "{result:?}");

                let writes = row.effects.iter().filter_map(|effect| match effect {
                    DependencyStorageEffect::Indirect { reference, kind: DependencyMutationKind::Replace(_), .. } =>
                        Some(reference.subject_root()),
                    _ => None,
                }).collect::<BTreeSet<_>>();

                assert!(writes.len() <= 1, "{method}, imported={imported}: {row:?}");

                writes_by_tag.entry(tag).or_default().extend(writes);
            }

            let parameter = |index| DependencySubjectRoot::Parameter(SymbolOrdinal::new(index));

            assert_eq!(writes_by_tag, BTreeMap::from([
                (representation.completed_variant(), [parameter(0), parameter(1)].into()),
                (representation.panicked_variant(), [parameter(2)].into()),
                (representation.cancelled_variant(), [parameter(3)].into()),
            ]), "{method}, imported={imported}: {summary:?}");
        }
    }
}

#[test]
fn imported_conditional_null_pointer_bounds_never_become_exact_identity() {
    let provider = compilation(
        r#"
        module api;
        using core.memory;

        func pointer<T>(pos value: &T, pos present: bool) -> DataPointer<T>
        {
            if present { return core.memory.address_of<T>(value); }
            return core.memory.null<T>();
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using core.memory;
        using example.package.api.pointer;

        func caller(pos value: &u32, pos present: bool) -> DataPointer<u32>
        {
            return example.package.api.pointer<u32>(value, present);
        }
        "#,
    );

    let diagnostics = consumer.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let dependencies =
        crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

    assert_eq!(dependencies.borrows().len(), 2, "{dependencies:#?}");

    assert_eq!(dependencies.borrows().iter().filter(|relation| relation.is_empty_reference()).count(), 1,
        "{dependencies:#?}");

    let borrows = dependencies.borrows();
    let input = borrows.iter().find(|relation| relation.entry().is_some()).expect("retained input origin");

    assert!(!input.is_exact(), "{dependencies:#?}");
}

#[test]
fn slice_arguments_bound_opaque_effects_without_exposing_other_inputs() {
    let source = r#"
        module api;
        extern func opaque(pos value: &mut [u8]);

        func update(pos preserved: &mut bool, pos value: &mut [u8])
        {
            opaque(value);
        }
    "#;

    let provider = compilation(source);

    let caller = r#"
        module app;
        using example.package.api;

        func caller(pos preserved: &mut bool)
        {
            let mut buffer: bytes<1> = [0];
            example.package.api.update(preserved, &mut buffer[..]);
        }
    "#;

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, caller)
        } else {
            compilation_from_sources([source, &caller.replace("example.package.", "")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:#?}"
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "caller",
        );

        assert!(
            summary.effects().iter().all(|effect| !matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Escape
                    | bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                    | bray_symbols::DependencyStorageEffect::Direct {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
            )),
            "imported={imported}: {summary:#?}"
        );
    }
}

#[test]
fn recursive_nullable_pointer_bounds_include_every_normal_origin() {
    let provider = compilation(
        r#"
        module api;
        using core.memory;

        func choose(pos first: &u32, pos second: &u32, pos depth: u32, pos present: bool) -> DataPointer<u32>
        {
            if depth == 0
            {
                if present { return core.memory.address_of<u32>(first); }
                return core.memory.null<u32>();
            }
            return choose(second, first, depth - 1, present);
        }
        "#,
    );

    let consumer = execution_consumer(
        &provider,
        r#"
        module app;
        using example.package.api.choose;

        func caller(pos first: &u32, pos second: &u32, pos depth: u32, pos present: bool) -> DataPointer<u32>
        {
            return example.package.api.choose(first, second, depth, present);
        }
        "#,
    );

    for (application, name) in [(&provider, "choose"), (&consumer, "caller")] {
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{name}: {diagnostics:#?}");

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            application,
            name,
        );

        assert_eq!(summary.borrows().len(), 3, "{name}: {summary:#?}");

        assert_eq!(summary.borrows().iter().filter(|relation| relation.is_empty_reference()).count(), 1,
            "{name}: the explicit empty alternative survives recursion and import: {summary:#?}");

        assert!(
            summary
                .borrows()
                .iter()
                .all(|relation| !relation.is_exact()),
            "{name}: {summary:#?}"
        );

        let origins = summary
            .borrows()
            .iter()
            .filter_map(|relation| relation.entry().map(|entry| entry.subject_root()))
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            origins,
            std::collections::BTreeSet::from([
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0)),
                bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(1)),
            ])
        );
    }
}

#[test]
fn copied_handles_do_not_publish_their_borrowed_containers() {
    let source = r#"
        trusted module api;
        using core.target;
        using core.memory;

        struct Owner
        {
            handle: usize;
            mut complete: bool;
        }

        extern trusted func opaque(pos pointer: DataPointer<u8>);

        trusted func from_handle(pos handle: usize) -> DataPointer<u8>
            uses(intrinsic)
        {
            return core.target.from_exposed_address<u8>(address = handle);
        }

        func update(pos owner: &mut Owner)
        {
            opaque(from_handle(owner.handle));
            owner.complete = true;
        }
    "#;

    let provider = compilation(source);

    let caller = r#"
        module app;
        using example.package.api;

        func caller(pos owner: &mut example.package.api.Owner)
        {
            example.package.api.update(owner);
        }
    "#;

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, caller)
        } else {
            compilation_from_sources([source, &caller.replace("example.package.", "")])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:#?}"
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &consumer, "caller",
        );

        assert!(
            summary.effects().iter().all(|effect| !matches!(
                effect,
                bray_symbols::DependencyStorageEffect::Escape
                    | bray_symbols::DependencyStorageEffect::Indirect {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
                    | bray_symbols::DependencyStorageEffect::Direct {
                        kind: bray_symbols::DependencyMutationKind::Escape,
                        ..
                    }
            )),
            "imported={imported}: {summary:#?}"
        );
    }
}

#[test]
fn owned_computations_retain_checked_effects_without_granting_authority() {
    let source = r#"
        trusted module api;
        using core.memory;

        struct Marker {}
        struct Owner { marker: Marker; }

        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func publish(pos pointer: DataPointer<Owner>);

        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }

        async func work(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
            opaque();
        }

        async func escaping(pos owner: &Owner)
            requires(trusted live(&owner.marker))
        {
            publish(core.memory.address_of<Owner>(owner));
        }

        struct Guard
        {
            owner: &Owner;

            destruct()
            {
                publish(core.memory.address_of<Owner>(self.owner));
            }
        }

        async func retained_result(pos owner: &Owner) -> Guard
        {
            return { owner = owner };
        }

        async func retained_input(pos guard: Guard) -> Guard
        {
            return guard;
        }

        async func pause() {}

        async func retained_suspended_result(pos owner: &Owner) -> Guard
        {
            let guard = Guard { owner = owner };
            await pause();
            return guard;
        }

        func leaked(pos owner: &Owner) -> &Owner
        {
            publish(core.memory.address_of<Owner>(owner));
            return owner;
        }

        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    for (body, valid) in [
        (
            "let held = api.leaked(&owner); api.opaque(); api.inspect(held);",
            false,
        ),
        (
            "let held = &owner; api.leaked(held); api.opaque(); api.inspect(held);",
            false,
        ),
        (
            "let guard = await api.retained_suspended_result(&owner);",
            true,
        ),
        (
            "let task = api.retained_suspended_result(&owner).start(); let outcome = await task.join();",
            false,
        ),
        (
            "let task = api.retained_suspended_result(&owner).start(); let outcome = await task.cancel();",
            false,
        ),
        (
            "let task = api.work(&owner).start(); try await task.join();",
            true,
        ),
        (
            "let task = api.work(&owner).start(); let moved = task; try await moved.join();",
            true,
        ),
        (
            "let task = api.work(&owner).start(); await task.cancel();",
            true,
        ),
        (
            "{ let task = api.work(&owner).start(); await api.work(&owner); }",
            true,
        ),
        (
            "let task = api.work(&owner).start(); await api.work(&owner); try await task.join();",
            true,
        ),
        ("{ let future = api.work(&owner); }", true),
        ("api.work(&owner);", true),
        ("api.retained_result(&owner);", true),
        ("{ let task = api.retained_result(&owner).start(); }", false),
        (
            "let guard = api.Guard { owner = &owner }; { let task = api.retained_input(guard).start(); }",
            false,
        ),
        (
            "let task = api.escaping(&owner).start(); try await task.join();",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            module app;
            using api;

            async func caller()
            {{
                let owner = api.make();
                {body}
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {body}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn independent_pointer_origins_survive_calls_without_publishing_other_inputs() {
    let source = r#"
        trusted module api;
        using core.memory;
        struct Marker {}
        struct Owner { marker: Marker; }
        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func external() -> DataPointer<Owner>;
        extern trusted func publish(pos pointer: DataPointer<Owner>);
        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }
        func independent(pos anchor: &Owner) -> DataPointer<Owner>
        {
            return external();
        }
        func direct(pos owner: &Owner) -> DataPointer<Owner>
        {
            return core.memory.address_of<Owner>(owner);
        }
        func mixed(pos owner: &Owner, pos choose_input: bool) -> DataPointer<Owner>
        {
            if choose_input { return direct(owner); }
            return independent(owner);
        }
        async func independent_async(pos anchor: &Owner) -> DataPointer<Owner>
        {
            return independent(anchor);
        }
        async func mixed_async(pos owner: &Owner, pos choose_input: bool) -> DataPointer<Owner>
        {
            return mixed(owner, choose_input);
        }
        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    for (pointer, valid) in [
        ("api.independent(&owner)", true),
        ("api.direct(&owner)", false),
        ("api.mixed(&owner, choose_input)", false),
        ("await api.independent_async(&owner)", true),
        ("await api.mixed_async(&owner, choose_input)", false),
    ] {
        let caller = format!(
            r#"
            module app;
            using api;
            async func caller(pos choose_input: bool)
            {{
                let owner = api.make();
                api.publish({pointer});
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {pointer}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    let contract =
        crate::compilation::result_dependencies::source_function_dependencies(&provider, "mixed");

    assert!(
        contract
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_none()),
        "{contract:#?}"
    );

    assert!(
        contract
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_some()),
        "{contract:#?}"
    );

    assert!(
        contract
            .borrows()
            .iter()
            .all(|relation| !relation.is_exact()),
        "{contract:#?}"
    );
}

#[test]
fn completed_composite_values_drop_only_the_obsolete_frame_capture() {
    let source = r#"
        trusted module api;
        using core.memory;
        struct Marker {}
        struct Owner { marker: Marker; }
        struct View { pointer: DataPointer<Owner>; }
        struct DeferredView { pointer: DataPointer<Owner>; computation: Future<View>; }
        trusted predicate live(value: &Marker);
        extern trusted func opaque();
        extern trusted func external() -> DataPointer<Owner>;
        extern trusted func publish(pos value: View);
        extern trusted func publish_deferred(pos value: DeferredView);
        trusted func make() -> Owner
            ensures(trusted live(&result.marker))
        {
            return { marker = Marker {} };
        }
        func independent(pos anchor: &mut Owner) -> View
        {
            return { pointer = external() };
        }
        func direct(pos owner: &mut Owner) -> View
        {
            return { pointer = core.memory.address_of_mut<Owner>(owner) };
        }
        async func independent_async(pos anchor: &mut Owner) -> View
        {
            return independent(anchor);
        }
        async func direct_async(pos owner: &mut Owner) -> View
        {
            return direct(owner);
        }
        func deferred(pos owner: &mut Owner) -> Future<View>
        {
            return direct_async(owner);
        }

        func inspect(pos owner: &Owner)
            requires(trusted live(&owner.marker))
            executes(pure, total)
        {
        }
    "#;

    let provider = compilation(source);

    let completion = crate::compilation::result_dependencies::source_function_dependencies(
        &provider,
        "independent_async",
    );

    assert!(
        completion
            .borrows()
            .iter()
            .any(|relation| relation.entry().is_none()),
        "the body publishes the independent nested result origin: {completion:#?}"
    );

    for (body, valid) in [
        ("api.publish(api.independent(&mut owner));", true),
        ("api.publish(api.direct(&mut owner));", false),
        (
            "api.publish(await api.independent_async(&mut owner));",
            true,
        ),
        ("api.publish(await api.direct_async(&mut owner));", false),
        (
            "let pending = api.independent_async(&mut owner); let moved = pending; api.publish(await moved);",
            true,
        ),
        (
            "let pending = api.direct_async(&mut owner); let moved = pending; api.publish(await moved);",
            false,
        ),
        (
            "api.publish_deferred(api.DeferredView { pointer = api.external(), computation = api.deferred(&mut owner) });",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            module app;
            using api;
            async func caller()
            {{
                let mut owner = api.make();
                {body}
                api.opaque();
                api.inspect(&owner);
            }}
        "#
        );

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(
                    &provider,
                    &caller
                        .replace("using api;", "using example.package.api;")
                        .replace("api.", "example.package.api."),
                )
            } else {
                compilation_from_sources([source, &caller])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "imported={imported}, {body}: {diagnostics:#?}, completion={completion:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn indexed_protected_writes_preserve_siblings_without_initializing_other_elements() {
    let api = r#"
        trusted module api;
        using core.memory;

        trusted func put<T>(pos storage: &mut [Uninit<T>; 2], pos index: usize, pos value: T)
            requires(index < 2)
        {
            _ = core.memory.uninit_write<T>(&mut storage[index], value);
        }
    "#;

    let provider = compilation(api);

    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    let flow = provider.storage_flow(source_function_body_key(&provider, "put")).unwrap();

    assert!(!flow.value().is_recovered(), "a checked indexed write retains its finite contract");

    for imported in [false, true] {
        for read_other in [false, true] {
            let prefix = if imported { "example.package.api" } else { "api" };

            let read_index = if read_other { "1 - index" } else { "index" };

            let source = format!(r#"
                trusted module app;
                using core.memory;
                using {prefix}.put;
                struct Marker {{}}
                struct Owner {{ marker: Marker; mut slots: [Uninit<u8>; 2]; }}
                trusted predicate live(marker: &Marker);
                func observe(pos marker: &Marker) requires(trusted live(marker)) executes(pure, total) {{}}
                trusted func caller(pos owner: &mut Owner, pos index: usize)
                    requires(index < 2, trusted live(&owner.marker))
                    uses(unchecked_init)
                {{
                    {prefix}.put<u8>(&mut owner.slots, index, 7);
                    observe(&owner.marker);
                    _ = core.memory.move_initialized<u8>(&mut owner.slots[{read_index}]);
                }}
            "#);

            let consumer = if imported {
                execution_consumer(&provider, &source)
            } else {
                compilation_from_sources([api, &source])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), !read_other, "imported={imported}, read_other={read_other}: {diagnostics:#?}");

            if read_other {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);

                let source_id = source_function_body_key(&consumer, "caller").source().syntax().source_id();

                assert!(diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                        && crate::test_support::diagnostic_at_call(diagnostic, source_id, &source,
                            "core.memory.move_initialized<u8>(&mut owner.slots[1 - index])")
                }), "{diagnostics:#?}");
            }
        }
    }
}

#[test]
fn exclusive_inputs_preserve_guarantees_through_protected_storage_writes() {
    let api = r#"
        trusted module api;
        using core.memory;

        struct Cell
        {
            mut value: bool;
        }

        trusted func overwrite(pos storage: &mut Uninit<u8>, pos independent: &mut Cell)
            requires(independent.value)
            ensures(independent.value)
        {
            _ = core.memory.uninit_write<u8>(storage, 0);
        }
    "#;

    let provider = compilation(api);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for imported in [false, true] {
        let prefix = if imported { "example.package.api" } else { "api" };

        let source = format!(r#"
            trusted module app;
            using core.memory;
            using {prefix};

            trusted func caller(pos storage: &mut Uninit<u8>, pos independent: &mut {prefix}.Cell)
                requires(independent.value)
                ensures(independent.value)
            {{
                {prefix}.overwrite(storage, independent);
            }}
        "#);

        let consumer = if imported {
            execution_consumer(&provider, &source)
        } else {
            compilation_from_sources([api, &source])
        };

        let diagnostics = consumer.check_diagnostics();

        assert!(!diagnostics.has_errors(), "imported={imported}: {diagnostics:#?}");
    }
}

#[test]
fn copied_handle_fields_do_not_retain_returned_view_loans() {
    let source = r#"
        module api;

        struct Handle
        {
            pointer: DataPointer<u32>;
        }

        @copy
        struct Link
        {
            pointer: DataPointer<u32>;
        }

        struct LinkOwner
        {
            link: Link;
        }

        func link_view(pos owner: &LinkOwner) -> &LinkOwner
        {
            return owner;
        }

        func copy_link_from_view(pos owner: &LinkOwner) -> Link
        {
            return link_view(owner).link;
        }

        struct Execution
        {
            handle: Handle;
        }

        struct Service
        {
            handle: Handle;
            execution: Execution;
        }

        struct View
        {
            handle: &Handle;
        }

        func reference(pos owner: &Execution) -> &Handle
        {
            return &owner.handle;
        }

        func retain(pos owner: &Handle) -> Handle
        {
            return
            {
                pointer = owner.pointer
            };
        }

        func borrow(pos owner: &Handle) -> View
        {
            return
            {
                handle = owner
            };
        }

        func assemble(pos execution: Execution) -> Service
        {
            let handle = retain(reference(&execution));

            return
            {
                handle = handle,
                execution = execution
            };
        }
    "#;

    let provider = compilation(source);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (result, body, valid) in [
        (
            "Service",
            r#"
                let handle = api.retain(api.reference(&execution));

                return
                {
                    handle = handle,
                    execution = execution
                };
            "#,
            true,
        ),
        ("Service", "return api.assemble(execution);", true),
        ("Link", "return api.copy_link_from_view(&links);", true),
        ("View", "return api.borrow(api.reference(&execution));", false),
        (
            "Handle",
            r#"
                let value: u32 = 7;

                let local: api.Execution =
                {
                    handle =
                    {
                        pointer = core.memory.address_of<u32>(&value)
                    }
                };

                return api.retain(api.reference(&local));
            "#,
            // Raw addresses carry no access authority or borrow lifetime.
            true,
        ),
    ] {
        let caller = format!(
            r#"
            module app;
            using example.package.api;
            using core.memory;

            func caller(pos execution: api.Execution, pos links: api.LinkOwner) -> api.{result}
            {{
                {body}
            }}
            "#
        ).replace("api.", "example.package.api.");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([source, &caller.replace("example.package.", "")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{result}, imported={imported}: {diagnostics:#?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn resource_acquisitions_survive_pointee_writes_but_not_resource_replacement() {
    let source = r#"
        trusted module api;

        using core.memory;

        struct Acquisition
        {
        }

        struct Resources
        {
            acquisition: Acquisition;
            pointer: DataPointer<u32>;
        }

        struct Registration
        {
            mut resources: Resources;
            mut next: usize;
        }

        trusted predicate acquired(owner: &Acquisition);

        predicate owned(owner: &Resources) = acquired(&owner.acquisition);

        trusted predicate contents_valid(owner: &Resources);

        trusted func update(pos owner: &mut Registration)
            requires(trusted owned(&owner.resources))
            ensures(trusted owned(&owner.resources))
            uses(raw_memory, unchecked_init)
        {
            let pointer = owner.resources.pointer;

            let permit = trusted lambda(pos owner: &Resources, pos pointer: DataPointer<u32>)
                requires(trusted owned(owner))
                ensures(
                    trusted core.memory.valid_write<u32>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u32>(pointer = pointer)
                )
                executes(pure, total) {};

            trusted permit(&owner.resources, pointer);
            trusted core.memory.write<u32>(pointer, 7);
            owner.next = 1;
        }

        func replace(pos owner: &mut Registration, pos replacement: Resources)
        {
            owner.resources = replacement;
        }

        func publish(pos owner: Registration)
            requires(trusted owned(&owner.resources))
        {
        }

        func publish_contents(pos owner: Registration)
            requires(trusted contents_valid(&owner.resources))
        {
        }
    "#;

    let provider = compilation(source);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (operation, check, valid) in [
        ("trusted api.update(&mut owner)", "publish", true),
        ("trusted api.update(&mut owner)", "publish_contents", false),
        ("api.replace(&mut owner, replacement)", "publish", false),
    ] {
        let caller = format!(r#"
            trusted module app;
            using example.package.api;

            trusted func caller(pos mut owner: api.Registration, pos replacement: api.Resources)
                requires(trusted api.owned(&owner.resources), trusted api.contents_valid(&owner.resources))
            {{
                {operation};
                api.{check}(owner);
            }}
        "#).replace("api.", "example.package.api.");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([source, &caller.replace("example.package.", "")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{operation}, {check}, imported={imported}: {diagnostics:#?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn disjunctive_variant_requirements_exclude_incompatible_match_arms() {
    let source = r#"
        module api;

        union Choice
        {
            First(pos value: u32);
            Second(pos value: u32);
            Third;
        }

        const func read(pos owner: &Choice) -> u32
            requires(owner matches Choice.First(_) || owner matches Choice.Second(_))
        {
            return match owner
            {
                case Choice.First(value) { yield value; }
                case Choice.Second(value) { yield value; }
                case Choice.Third { panic("unreachable variant"); }
            };
        }
    "#;

    let provider = compilation(source);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (variant, valid) in [("First(7)", true), ("Second(9)", true), ("Third", false)] {
        let caller = format!(r#"
            module app;
            using example.package.api;

            const func caller() -> u32
            {{
                let owner = api.Choice.{variant};
                return api.read(&owner);
            }}
        "#).replace("api.", "example.package.api.");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([source, &caller.replace("example.package.", "")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{variant}, imported={imported}: {diagnostics:#?}");

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
fn entry_union_observations_preserve_sibling_owner_values() {
    let source = r#"
        trusted module api;

        struct Acquisition
        {
        }

        struct Owner
        {
            acquisition: Acquisition;
            pointer: DataPointer<u32>;
        }

        union Target
        {
            First(pos owner: Owner);
            Second(pos owner: Owner);
        }

        struct Resources
        {
            target: Target;
            other: Owner;
        }

        trusted predicate acquired(owner: &Acquisition);

        trusted predicate authorized(owner: &Acquisition);

        predicate retained(owner: &Owner) = acquired(&owner.acquisition);

        predicate permitted(owner: &Owner) = authorized(&owner.acquisition);

        const func first(pos target: &Target) -> &Owner
            requires(target matches Target.First(_))
        {
            return match target
            {
                case Target.First(owner) { yield &owner; }
                case Target.Second(_) { panic("missing first owner"); }
            };
        }

        const func second(pos target: &Target) -> &Owner
            requires(target matches Target.Second(_))
        {
            return match target
            {
                case Target.Second(owner) { yield &owner; }
                case Target.First(_) { panic("missing second owner"); }
            };
        }

        predicate owned(owner: &Resources) = retained(&owner.other) && permitted(&owner.other) &&
            ((owner.target matches Target.First(_) && retained(first(&owner.target)) && permitted(first(&owner.target))) ||
             (owner.target matches Target.Second(_) && retained(second(&owner.target)) && permitted(second(&owner.target))));

        func inspect(pos owner: &Owner)
            requires(trusted retained(owner), trusted permitted(owner))
        {
        }

        func borrowed(pos owner: &Resources)
            requires(trusted owned(owner))
        {
            match owner.target
            {
                case Target.First(payload) { inspect(&payload); }
                case Target.Second(payload) { inspect(&payload); }
            }
        }

        func split(pos owner: Resources) -> Owner
            requires(trusted owned(&owner))
            ensures(trusted acquired(&result.acquisition))
        {
            let other = owner.other;

            match owner.target
            {
                case Target.First(payload) { inspect(&payload); }
                case Target.Second(payload) { inspect(&payload); }
            }

            inspect(&other);

            return other;
        }

        func forward(pos owner: Owner) -> Owner
            requires(trusted acquired(&owner.acquisition))
            ensures(trusted acquired(&result.acquisition))
        {
            return owner;
        }

        func project(pos owner: Resources) -> Owner
            requires(trusted owned(&owner))
            ensures(trusted acquired(&result.acquisition))
        {
            return owner.other;
        }
    "#;

    let provider = compilation(source);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (operation, valid) in [("split(owner)", true), ("project(owner)", true), ("project(replacement)", false)] {
        let caller = format!(r#"
            module app;
            using example.package.api;

            func caller(pos owner: api.Resources, pos replacement: api.Resources) -> api.Owner
                requires(trusted api.owned(&owner))
                ensures(trusted api.acquired(&result.acquisition))
            {{
                return api.{operation};
            }}
        "#).replace("api.", "example.package.api.");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([source, &caller.replace("example.package.", "")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{operation}, imported={imported}: {diagnostics:#?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn conditional_component_guarantees_survive_complete_union_consumption() {
    let source = r#"
        trusted module api;

        struct Acquisition
        {
        }

        struct Owner
        {
            acquisition: Acquisition;
            pointer: DataPointer<u32>;
        }

        union Target
        {
            First(pos owner: Owner);
            Second(pos owner: Owner, pos callback: Owner);
            Third(pos owner: Owner);
        }

        struct Registration
        {
            resources: Resources;
            mut next: usize;
        }

        struct Resources
        {
            target: Target;
            other: Owner;
        }

        trusted predicate acquired(owner: &Acquisition);

        trusted predicate ready();

        trusted predicate authorized(owner: &Acquisition);

        predicate retained(owner: &Owner) = acquired(&owner.acquisition);

        predicate permitted(owner: &Owner) = authorized(&owner.acquisition);

        const func first(pos target: &Target) -> &Owner
            requires(target matches Target.First(_))
        {
            return match target
            {
                case Target.First(owner) { yield &owner; }
                case Target.Second(_, _) | Target.Third(_) { panic("missing first owner"); }
            };
        }

        const func second(pos target: &Target) -> &Owner
            requires(target matches Target.Second(_, _))
        {
            return match target
            {
                case Target.Second(owner, _) { yield &owner; }
                case Target.First(_) | Target.Third(_) { panic("missing second owner"); }
            };
        }

        predicate owned(owner: &Resources) = retained(&owner.other) && permitted(&owner.other) &&
            (
                (owner.target matches Target.First(_) && retained(first(&owner.target)) && permitted(first(&owner.target))) ||
                (owner.target matches Target.Second(_, _) && retained(second(&owner.target)) && permitted(second(&owner.target)) &&
                 retained(callback_owner(&owner.target)) && permitted(callback_owner(&owner.target))) ||
                (owner.target matches Target.Third(_) && retained(third(&owner.target)) && permitted(third(&owner.target)))
            );

        func inspect(pos owner: &Owner)
            requires(trusted retained(owner), trusted permitted(owner))
        {
        }

        func borrowed(pos owner: &Resources)
            requires(trusted owned(owner))
        {
            match owner.target
            {
                case Target.First(payload) { inspect(&payload); }
                case Target.Second(payload, _) { inspect(&payload); }
                case Target.Third(payload) { inspect(&payload); }
            }
        }

        func split(pos owner: Registration) -> Owner
            requires(trusted owned(&owner.resources), trusted ready())
            ensures(trusted acquired(&result.acquisition))
        {
            let other = owner.resources.other;

            match consume owner.resources.target
            {
                case Target.First(payload) { retire(payload); }
                case Target.Second(payload, callback) { retire(callback); retire(payload); }
                case Target.Third(payload) { retire(payload); }
            }

            inspect(&other);
            return other;
        }

        func retire(pos owner: Owner)
            requires(trusted retained(&owner), trusted permitted(&owner), trusted ready())
        {
        }

        const func third(pos target: &Target) -> &Owner
            requires(target matches Target.Third(_))
        {
            return match target
            {
                case Target.Third(owner) { yield &owner; }
                case Target.First(_) | Target.Second(_, _) { panic("missing third owner"); }
            };
        }

        const func callback_owner(pos target: &Target) -> &Owner
            requires(target matches Target.Second(_, _))
        {
            return match target
            {
                case Target.Second(_, callback) { yield &callback; }
                case Target.First(_) | Target.Third(_) { panic("missing callback"); }
            };
        }
    "#;

    let provider = compilation(source);
    let diagnostics = provider.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    for (argument, valid) in [("owner", true), ("replacement", false)] {
        let caller = format!(r#"
            module app;
            using example.package.api;

            func caller(pos owner: api.Registration, pos replacement: api.Registration) -> api.Owner
                requires(trusted api.owned(&owner.resources), trusted api.ready())
                ensures(trusted api.acquired(&result.acquisition))
            {{
                return api.split({argument});
            }}
        "#).replace("api.", "example.package.api.");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &caller)
            } else {
                compilation_from_sources([source, &caller.replace("example.package.", "")])
            };

            let diagnostics = consumer.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{argument}, imported={imported}: {diagnostics:#?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }
}

#[test]
fn escape_summaries_preserve_input_roots_before_alias_expansion() {
    let mut failures = Vec::new();

    for (extra, wrapped, choice, expected) in [("", false, false, 1), ("expose(core.memory.address_of<Value>(owner.value));", false, false, 3), ("", true, false, 1), ("", false, true, 2)] {
        let source = r#"
            trusted module api;

            using core.memory;
            using core.target;

            struct Value
            {
                pointer: DataPointer<u8>;
            }

            struct Frame
            {
                value: &Value;
                pointer: DataPointer<Value>;
                first: Value;
                second: Value;
            }

            func choose(pos owner: &Frame, pos first: bool) -> &Value
            {
                if first
                {
                    return &owner.first;
                }

                return &owner.second;
            }

            union Packet
            {
                Selected(pos pointer: DataPointer<Value>);
                Other(pos pointer: DataPointer<Value>);
            }

            struct Wrapped
            {
                packet: Packet;
            }

            func wrap(pos unrelated: &Frame, pos pointer: DataPointer<Value>) -> Wrapped
            {
                return
                {
                    packet = Packet.Selected(pointer)
                };
            }

            @link(name = "c")
            @abi(c)
            @symbol(name = "expose")
            extern trusted func expose(pos pointer: DataPointer<Value>)
                uses(foreign_call);

            trusted func forward(pos owner: &mut Frame, pos first: bool)
                uses(CAPABILITY)
            {
                _ = owner.value;
                PUBLISH_VALUE
                ESCAPE_BORROW
            }
        "#.replace("ESCAPE_BORROW", extra).replace("CAPABILITY", if wrapped || choice { "intrinsic" } else { "foreign_call" }).replace("PUBLISH_VALUE", if choice {
            "let selected = choose(&owner, first);\n                _ = core.target.expose_address<u8>(selected.pointer);"
        } else if wrapped {
            "let wrapped = wrap(&owner, owner.pointer);\n                _ = core.target.expose_address<Wrapped>(core.memory.address_of<Wrapped>(&wrapped));"
        } else { "expose(owner.pointer);" });

        let application = super::fixtures::compilation_from_sources_for_product_with_platform_services_and_worker_budget(
            [source.as_str()], bray_symbols::ProductKind::Library, [], crate::WorkerBudget::default(), None,
            [bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new("c").expect("native link name"),
                bray_symbols::NativeLinkKind::Dynamic,
            )],
        );

        let consumer = execution_consumer(&application, r"
            module app;

            using example.package.api;

            func relay(pos owner: &mut example.package.api.Frame, pos first: bool)
            {
                example.package.api.forward(owner, first);
            }
        ");

        for (application, name, surface) in [(&application, "forward", "source"), (&consumer, "relay", "imported")] {
            let diagnostics = application.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{surface}: {}", super::fixtures::diagnostic_messages(diagnostics));

            let flow = application.storage_flow(source_function_body_key(application, name))
                .expect("escape summary composes");

            let effects = flow.value().dependencies().effects();

            let roots = effects.iter().filter(|effect| matches!(effect,
                bray_symbols::DependencyStorageEffect::Direct { kind: bray_symbols::DependencyMutationKind::Escape, .. }
                | bray_symbols::DependencyStorageEffect::Indirect { kind: bray_symbols::DependencyMutationKind::Escape, .. })).count();

            if choice && effects.iter().any(|effect| !matches!(effect,
                bray_symbols::DependencyStorageEffect::Indirect { reference, projections, kind: bray_symbols::DependencyMutationKind::Escape, .. }
                if reference.projections().len() == 2 && projections.is_empty())) {
                failures.push(format!("{surface}: borrowed choices must publish their pointer fields, got {effects:#?}"));
            }

            if roots != expected { failures.push(format!("{surface} {extra} wrapped={wrapped}: expected {expected} roots, got {effects:#?}")); }
            assert!(!effects.contains(&bray_symbols::DependencyStorageEffect::Escape), "{surface}: known escape roots remain structured");
        }

        if wrapped {
            let dependencies = crate::compilation::result_dependencies::source_function_dependencies(&application, "wrap");

            assert_eq!(dependencies.borrows().iter().filter(|relation| relation.is_no_payload()).count(), 1,
                "the inactive union field must retain its shape through portable publication: {dependencies:#?}");

            assert!(!dependencies.borrows().iter().any(|relation| relation.is_empty_reference()),
                "inactive fields must not become live null addresses: {dependencies:#?}");
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
