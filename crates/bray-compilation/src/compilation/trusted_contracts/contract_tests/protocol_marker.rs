use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation, source_function_body_key};

#[test]
fn complete_protocol_owners_survive_unrelated_calls_until_their_storage_is_exposed() {
    for (fields, operation) in [
        ("tag: usize; pointer: DataPointer<u8>;", "opaque();"),
        ("storage: Uninit<[u64; 16]>; finish: Native;", "opaque();"),
        ("first: &Uninit<u32>; second: &Uninit<u32>;", ""),
    ] {
        let exposures: &[(&str, bool)] = if operation.is_empty() {
            &[("", true)]
        } else {
            &[("", true), ("_ = trusted publish(&owner);", false)]
        };

        for &(publish, valid) in exposures {
            let source = format!(
                r#"
                trusted module app;
                using core.memory;
                using core.target;
                callable Native = @abi(c) trusted func();
                struct Owner
                {{
                    {fields}
                }}
                trusted predicate live(owner: &Owner);
                trusted predicate checked(owner: &Owner);
                extern trusted func inspect(pos owner: &Owner)
                    requires(trusted live(owner)) ensures(trusted checked(owner)) executes(pure, total);
                func observe(pos owner: &Owner)
                    requires(trusted checked(owner))
                    executes(pure, total)
                {{
                }}
                extern func opaque()
                    executes(total);
                extern func release_owner(pos owner: Owner)
                    requires(trusted live(owner = &owner))
                    executes(total);
                trusted func publish(pos owner: &Owner) -> usize
                    uses(intrinsic)
                {{
                    return trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(owner));
                }}
                func caller(pos owner: Owner)
                    requires(trusted live(owner = &owner))
                {{
                    {publish}
                    {operation}
                    inspect(&owner);
                    observe(&owner);
                    release_owner(owner);
                }}
            "#
            );

            let application = compilation(&source);

            assert!(
                !application.syntax_tree_result().diagnostics().has_errors(),
                "{source}"
            );

            let checked = application
                .lowered_unit(source_function_body_key(&application, "caller"))
                .expect("complete protocol owners retain their checked storage context");

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {checked:#?}"
            );
        }
    }
}

#[test]
fn shared_protocol_markers_survive_native_exposure_and_loop_merges() {
    for (parameter, subject, binding) in [
        ("pos owner: &Owner", "owner", ""),
        ("pos original: Owner", "original", "let owner = &original;"),
    ] {
        for (use_owner, valid) in [
            ("observe(&owner.acquisition);", true),
            ("let fabricated: Marker = {}; observe(&fabricated);", false),
            ("observe_payload(owner.pointer);", false),
        ] {
            let source = format!(
                r#"
                trusted module app;
                using core.memory;
                using core.target;
                struct Marker
                {{
                }}
                struct Owner
                {{
                    acquisition: Marker;
                    identity: u64;
                    pointer: DataPointer<u64>;
                }}
                trusted predicate held(owner: &Marker);
                trusted predicate scalar_held(owner: &u64);
                trusted predicate payload_held(pointer: DataPointer<u64>);
                extern func opaque(pos pointer: DataPointer<u64>)
                    executes(total);
                func observe(pos marker: &Marker)
                    requires(trusted held(owner = marker))
                    executes(pure, total)
                {{
                }}
                func observe_scalar(pos value: &u64)
                    requires(trusted scalar_held(owner = value))
                    executes(pure, total)
                {{
                }}
                func observe_payload(pos pointer: DataPointer<u64>)
                    requires(trusted payload_held(pointer = pointer))
                    executes(pure, total)
                {{
                }}
                func advance(pos value: &mut u32)
                {{
                    value += 1;
                }}
                trusted func publish(pos owner: &Owner) -> usize
                    uses(intrinsic)
                {{
                    return trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(owner));
                }}
                func caller({parameter}, pos count: u32)
                    requires(
                        trusted held(owner = &{subject}.acquisition),
                        trusted scalar_held(owner = &{subject}.identity),
                        trusted payload_held(pointer = {subject}.pointer)
                    )
                {{
                    {binding}
                    _ = trusted publish(owner);
                    let mut index: u32 = 0;
                    while index < count
                    {{
                        if index & 1 == 0 {{ opaque(owner.pointer); }}
                        {use_owner}
                        observe_scalar(&owner.identity);
                        advance(&mut index);
                    }}
                    observe(&owner.acquisition);
                    observe_scalar(&owner.identity);
                }}
                "#
            );

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:#?}"
            );
        }
    }
}

#[test]
fn mutable_protocol_markers_survive_only_marker_free_writes() {
    for (operation, valid) in [
        ("owner.payload += 1;", true),
        ("advance(&mut owner.payload);", true),
        ("trusted core.memory.write<u32>(pointer, 1);", true),
        ("replace(owner);", false),
        ("owner.acquisition = Marker {};", false),
    ] {
        let raw = operation.contains("core.memory.write");

        let memory_requirements = if raw {
            "trusted core.memory.valid_write<u32>(pointer = pointer, count = 1), trusted core.memory.aligned_for<u32>(pointer = pointer),"
        } else {
            ""
        };

        for publication in ["", "_ = trusted core.target.expose_address<Owner>(core.memory.address_of<Owner>(&owner));"] {
        let capabilities = match (raw, publication.is_empty()) {
            (true, false) => "uses(intrinsic, raw_memory, unchecked_init)",
            (true, true) => "uses(raw_memory, unchecked_init)",
            (false, false) => "uses(intrinsic)",
            (false, true) => "",
        };

        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            using core.target;
            struct Marker
            {{
            }}
            struct Owner
            {{
                mut acquisition: Marker;
                mut payload: u32;
            }}
            trusted predicate held(owner: &Marker);
            extern func replace(pos owner: &mut Owner);
            func advance(pos value: &mut u32)
            {{
                value += 1;
            }}
            func observe(pos marker: &Marker)
                requires(trusted held(owner = marker))
            {{
            }}
            trusted func caller(pos owner: &mut Owner, pos pointer: DataPointer<u32>)
                requires(
                    trusted held(owner = &owner.acquisition),
                    {memory_requirements}
                ) {capabilities}
            {{
                {publication}
                {operation}
                observe(&owner.acquisition);
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(
            !diagnostics.has_errors(),
            valid,
            "{source}: {diagnostics:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                &diagnostics,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
        }
    }
}

#[test]
fn guarded_owner_authority_uses_entry_values_after_mutation_and_opaque_calls() {
    for (initial, valid) in [(true, true), (false, false)] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker
            {{
            }}
            struct Owner
            {{
                acquisition: Marker;
            }}
            trusted predicate live(owner: &Marker);
            trusted func acquire(pos enabled: bool) -> Owner
                when(enabled) {{ ensures(trusted live(owner = &result.acquisition)) }}
                executes(pure, total)
            {{
                return
                {{
                    acquisition = Marker {{}}
                }};
            }}
            extern func opaque()
                executes(total);
            func release_owner(pos owner: Owner)
                requires(trusted live(owner = &owner.acquisition))
                executes(pure, total)
            {{
            }}
            func caller()
            {{
                let mut enabled = {initial};
                let owner = acquire(enabled);
                enabled = !enabled;
                opaque();
                release_owner(owner);
            }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}"
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("owner guards retain their actual entry values");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
fn shared_marker_borrows_preserve_the_original_owner_without_mutable_authority() {
    for (parameter, argument, valid) in [
        ("&Owner", "&owner", true),
        ("&mut Owner", "&mut owner", false),
        ("&Marker", "&owner.marker", true),
    ] {
        let source = format!(
            r#"
            trusted module app;
            struct Marker
            {{
            }}
            trusted predicate live(value: &Marker);
            struct Owner
            {{
                marker: Marker;
                trusted destruct() requires(trusted live(&self.marker)) executes(total)
                {{
                }}
            }}
            trusted func create() -> Owner
                ensures(trusted live(&result.marker))
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}
            func invoke(pos owner: {parameter}, pos callback: func() -> unit)
            {{
                callback();
            }}
            func caller(pos callback: func() -> unit)
            {{
                let mut owner = create();
                invoke({argument}, callback);
            }}
        "#
        );

        let application = compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "caller"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            checked.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn shared_component_borrow_preserves_only_its_marker_observations() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker
        {
        }
        struct Owner
        {
            instance: Marker;
        }
        struct Pair
        {
            first: Owner;
            second: Owner;
        }
        trusted predicate live(marker: &Marker);
        trusted func make() -> Pair
            ensures(trusted live(&result.first.instance))
        {
            return
            {
                first =
                {
                    instance = Marker {}
                }, second =
                {
                    instance = Marker {}
                }
            };
        }
        extern trusted func opaque();
        func observe(pos owner: &Owner)
            requires(trusted live(&owner.instance))
        {
        }
        func caller()
        {
            let pair = make();
            let borrowed = &pair.first;
            opaque();
            observe(borrowed);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}
