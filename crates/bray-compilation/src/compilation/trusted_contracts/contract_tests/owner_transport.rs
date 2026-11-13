use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{source_function_dependencies};
use crate::test_support::{compilation, source_function_body_key};
use super::fixtures::{production_sync_compilation};

#[test]
fn ownership_composition_accepts_complete_transport() {
    let source = include_str!("../../../../../../xtask/fixtures/native-execution/ownership-transitions.bray");
    let compilation = compilation(source);
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn ownership_composition_preserves_trusted_result_transport() {
    let source = include_str!("../../../../../../xtask/fixtures/native-execution/ownership-transitions.bray")
        .replace("owner = acquire()", "owner = trusted acquire()");
    let compilation = compilation(&source);
    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn ownership_composition_rejects_fabricated_authority() {
    let source = include_str!("../../../../../../xtask/fixtures/native-execution/ownership-transitions.bray")
        .replace("owner = acquire()", r#"owner = Owner
        {
            marker = Marker
            {
            },
            value = 7
        }"#);

    let compilation = compilation(&source);
    let diagnostics = compilation.check_diagnostics();

    bray_testing::assert_goal_state_diagnostic_kind(
        diagnostics,
        DiagnosticKind::CheckingTrustedObligationNotProven,
    );
}

#[test]
fn extracted_owned_fields_outlive_their_replaced_storage() {
    let mut failures = Vec::new();

    for deferred in [false, true] {
        for route in ["known", "unknown", "overwrite", "loop", "mutated"] {
            let asynchronous = if deferred { "async " } else { "" };
            let take = |name: &str, choice: &str| {
                let call = format!("trusted take_owned_report(&mut reports, {choice})");
                if deferred { format!("let pending_{name} = {call};\nlet mut {name} = await pending_{name};") }
                else { format!("let mut {name} = {call};") }
            };
            let first = take("previous", "false");
            let body = match route {
                "known" | "unknown" => {
                    let incoming = take("incoming", if route == "known" { "true" } else { "choice" });
                    format!("{first}\n{incoming}\nconsume_report(incoming);\nconsume_report(previous);")
                }
                "overwrite" => format!("{first}\nreports.left = replacement;\nconsume_report(previous);"),
                "mutated" => format!("{first}\ntrusted change_report(&mut previous);\nconsume_report(previous);"),
                "loop" => {
                    let current = take("current", "choice");
                    format!(r#"
                        let mut previous: ScalarReport? = none;
                        let mut turn: u32 = 0;

                        while turn < 2
                        {{
                            {current}
                            match consume previous
                            {{
                                case ?retained
                                {{
                                    consume_report(retained);
                                }}
                                case none
                                {{
                                }}
                            }}

                            previous = current;
                            turn += 1;
                        }}

                        match consume previous
                        {{
                            case ?retained
                            {{
                                consume_report(retained);
                            }}
                            case none
                            {{
                            }}
                        }}
                    "#)
                }
                _ => unreachable!(),
            };
            let source = format!(r#"
                trusted module app;

                using std.memory;

                struct Marker
                {{
                }}

                trusted predicate live(owner: &Marker);

                struct ScalarReport
                {{
                    mut value: usize;
                }}

                struct ReportProgress
                {{
                    mut state: u32;
                    mut report: ScalarReport;
                }}

                struct ScalarReports
                {{
                    anchor: Marker;
                    mut left: ReportProgress;
                    mut right: ReportProgress;
                }}

                trusted predicate report_valid(owner: &ScalarReport);

                trusted func fresh_report() -> ScalarReport
                    ensures(trusted report_valid(owner = &result))
                    executes(pure, total)
                {{
                    return
                    {{
                        value = 0
                    }};
                }}

                func consume_report(pos report: ScalarReport)
                    requires(trusted report_valid(owner = &report))
                {{
                }}

                predicate reports_owned(owner: &ScalarReports) = live(owner = &owner.anchor);

                trusted {asynchronous}func take_owned_report(pos reports: &mut ScalarReports, pos right: bool) -> ScalarReport
                    requires(trusted reports_owned(owner = &reports))
                    ensures(trusted reports_owned(owner = &reports), trusted report_valid(owner = &result))
                {{
                    if right
                    {{
                        return std.memory.replace_value<ScalarReport>(&mut reports.right.report, trusted fresh_report());
                    }}

                    return std.memory.replace_value<ScalarReport>(&mut reports.left.report, trusted fresh_report());
                }}

                extern trusted func make_reports() -> ScalarReports
                    ensures(trusted reports_owned(owner = &result));


                extern trusted func change_report(pos report: &mut ScalarReport);

                trusted {asynchronous}func caller(pos marker: &Marker, pos choice: bool, pos replacement: ReportProgress)
                    requires(trusted live(owner = marker))
                    ensures(trusted live(owner = marker))
                {{
                    let mut reports = trusted make_reports();
                    {body}
                }}
            "#);
            let application = crate::test_support::production_standard_library_compilation(
                crate::SelectedTarget::baseline(), &source);
            let published = source_function_dependencies(&application, "take_owned_report");
            assert!(!published.owned_result().is_empty(), "the fixture must publish checked extraction lineage: {published:?}");
            let checked = application.lowered_unit(source_function_body_key(&application, "caller"))
                .expect("field extraction caller reaches lowering");
            let valid = route != "mutated";
            if !checked.diagnostics().has_errors() != valid {
                failures.push(format!("deferred={deferred} route={route}: {:#?}", checked.diagnostics()));
            }
            if !valid && checked.diagnostics().has_errors() {
                bray_testing::assert_goal_state_diagnostic_kind(checked.diagnostics(), DiagnosticKind::CheckingTrustedObligationNotProven);
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn transparent_predicate_getters_preserve_the_contained_owner() {
    for (body, valid) in [
        (
            "match consume owner { case ?value { release_owner(value); } case none {} }",
            true,
        ),
        ("release_owner(Owner { acquisition = Marker {} });", false),
    ] {
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
            trusted predicate acquired(owner: &Marker);
            const func present_owner(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_) executes(pure, total)
            {{
                return match owner
                {{
                    case ?value
                    {{
                        yield &value;
                    }}
                    case none
                    {{
                        panic("missing owner");
                    }}
                }};
            }}
            predicate optional_owner(owner: &(Owner?)) = owner matches none || acquired(owner = &present_owner(owner).acquisition);
            func release_owner(pos owner: Owner)
                requires(trusted acquired(owner = &owner.acquisition))
            {{
            }}
            func forward(pos owner: Owner?)
                requires(trusted optional_owner(owner = &owner))
            {{
                {body}
            }}
            "#
        );

        let application = compilation(&source);

        assert!(
            !application.syntax_tree_result().diagnostics().has_errors(),
            "{source}: {:?}",
            application.syntax_tree_result().diagnostics()
        );

        let checked = application
            .lowered_unit(source_function_body_key(&application, "forward"))
            .expect("predicate getter retains its checked reference context");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:#?}"
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn physical_entry_origin_does_not_erase_a_local_lifetime_anchor() {
    for (inside, valid) in [(true, true), (false, false)] {
        let observe = "observe(pointer);";

        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            using std.memory;
            struct Anchor
            {{
            }}
            trusted func anchored(pos owner: &Anchor, pos payload: &u32) -> &u32
                uses(raw_memory, unchecked_alias)
            {{
                return trusted std.memory.borrow_from<u32, Anchor>(owner, std.memory.address_of<u32>(payload));
            }}
            trusted func address_view(pos owner: &u32) -> DataPointer<u32>
                ensures(trusted core.memory.valid_read<u32>(pointer = result, count = 1))
                executes(pure, total)
            {{
                return std.memory.address_of<u32>(owner);
            }}
            func observe(pos pointer: DataPointer<u32>)
                requires(trusted core.memory.valid_read<u32>(pointer = pointer, count = 1))
                executes(pure, total) {{}}
            func production_probe(pos payload: &u32)
            {{
                let mut pointer = std.memory.null<u32>();
                {{
                    let anchor: Anchor = Anchor {{}};
                    let borrowed = anchored(&anchor, payload);
                    pointer = address_view(borrowed);
                    {}
                }};
                {}
            }}
        "#,
            if inside { observe } else { "" },
            if inside { "" } else { observe }
        );

        let application = production_sync_compilation(&source);

        let checked = application
            .lowered_unit(source_function_body_key(&application, "production_probe"))
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
#[ignore = "tier:proof"]
fn mutable_slice_foreign_output_keeps_its_extent_authority() {
    let application = production_sync_compilation(
        r#"
        trusted module app;

        using core.memory;
        using std.memory;
        @abi(c)
        trusted func sink(destination: DataPointer<u8>, length: u64, transferred: DataPointer<u64>)
            requires(
                length == 0 || trusted core.memory.valid_write<u8>(pointer = destination, count = length as usize),
                trusted core.memory.valid_write<u64>(pointer = transferred, count = 1),
                trusted core.memory.aligned_for<u64>(pointer = transferred),
            ) { release(); }
        struct Lock
        {
            trusted destruct()
            {
                release();
            }
        }
        extern trusted func acquire() -> Result<Lock, u32>;
        extern trusted func release();
        trusted func caller(pos destination: &mut [u8]) -> Result<unit, u32>
        {
            let lock = try acquire();
            let capacity: usize = destination.length();
            let pointer = internal std.memory.slice_pointer_mut(destination, count = capacity);
            let mut transferred: u64 = 0;
            let transferred_pointer = std.memory.address_of_mut<u64>(&mut transferred);
            sink(destination = pointer, length = capacity as u64, transferred = transferred_pointer);
            return Ok(unit);
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

#[test]
fn trusted_implementation_does_not_export_undeclared_authority() {
    let application = compilation(
        r#"
        trusted module app;
        struct Marker
        {
        }
        struct Owner
        {
            marker: Marker;
        }
        trusted predicate held(marker: &Marker);
        trusted func acquire() -> Owner
            ensures(trusted held(&result.marker))
        {
            return
            {
                marker = Marker {}
            };
        }
        trusted func hidden() -> Owner
        {
            return acquire();
        }
        func observe(pos value: &Owner)
            requires(trusted held(&value.marker))
        {
        }
        func caller()
        {
            let value = hidden();
            observe(&value);
        }
    "#,
    );

    let checked = application
        .lowered_unit(source_function_body_key(&application, "caller"))
        .expect("fixture callable must lower successfully");

    assert!(!checked.diagnostics().has_errors(), "{:?}", checked.diagnostics());
}

#[test]
fn consuming_copyable_receivers_move_authority_and_retire_the_source() {
    use bray_diagnostics::DiagnosticKind;

    for (operation, failure) in [
        ("let choice: u32? = none; trusted ticket.redeem(choice);", None),
        ("let choice: u32 = 1; trusted ticket.redeem(choice);", None),
        ("let choice: u32? = none; trusted ticket.redeem(choice); observe(&ticket);", Some(DiagnosticKind::CheckingUseOfMovedStorage)),
        ("copy_ticket(ticket);", Some(DiagnosticKind::CheckingTrustedWitnessTransferNotProven)),
        ("let choice: u32 = 1; accept(choice);", None),
        ("let nested: Holder<u32> = { value = 1 }; require_nested(nested);", Some(DiagnosticKind::CheckingNoApplicableCandidate)),
    ] {
        let source = r#"
            trusted module app;

            @copy
            struct Marker
            {
            }

            trusted predicate live(marker: &Marker);

            @copy
            struct Ticket
            {
                marker: Marker;

                trusted consume func redeem(pos choice: u32?)
                    requires(trusted live(&self.marker))
                {
                }
            }

            struct Holder<T>
            {
                value: T;
            }

            func accept<T>(pos value: T?)
            {
            }

            func require_nested<T>(pos value: Holder<(T?)>)
            {
            }

            trusted func issue() -> Ticket
                ensures(trusted live(&result.marker))
            {
                return
                {
                    marker = Marker
                    {
                    }
                };
            }

            func observe(pos ticket: &Ticket)
                requires(trusted live(&ticket.marker))
            {
            }

            func copy_ticket(pos ticket: Ticket)
                requires(trusted live(&ticket.marker))
            {
            }

            func probe()
            {
                let ticket = trusted issue();

                OPERATION
            }
        "#.replace("OPERATION", operation);
        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(diagnostics.has_errors(), failure.is_some(), "{operation}: {diagnostics:#?}");

        if let Some(failure) = failure {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, failure);
        }
    }
}

#[test]
fn returned_tuple_owner_keeps_its_predicate_after_scalar_field_reads() {
    let application = compilation(
        r#"
        trusted module app;

        using std.memory;
        struct Owner
        {
            mut first: u32;
            mut second: u32;
            mut third: usize;
            mut fourth: usize;
            mut pointer: DataPointer<u8>;
        }
        trusted predicate live(owner: &Owner);
        trusted func make() -> (u32, Owner)
            ensures(trusted live(&result.1))
        {
            return (
                1,
                Owner
                {
                    first = 2, second = 3, third = 0, fourth = 0, pointer = std.memory.null<u8>()
                }
            );
        }
        func consume_owner(pos owner: Owner)
            requires(trusted live(&owner))
        {
        }
        func caller()
        {
            loop
            {
                let (state, owner) = make();
                if state == 1
                {
                    let fields = (owner.first, owner.second, owner.third, owner.fourth);
                    consume_owner(owner);
                }
                else
                {
                    return;
                }
            }
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

#[test]
fn owned_return_does_not_preserve_changed_boolean_contents() {
    for (mutation, valid) in [("", true), ("flags.ready = false;", false)] {
        let source = format!(
            r#"
            module app;
            struct Flags
            {{
                mut ready: bool;
            }}
            func clear_flags(pos mut flags: Flags) -> Flags
                executes(total)
            {{
                {mutation}
                return flags;
            }}
            func require_old_ready()
                executes(total)
            {{
                let flags: Flags = {{ ready = true }};
                let changed_flags = clear_flags(flags);
                assert(changed_flags.ready);
            }}
            "#,
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn consuming_transitions_introduce_child_owners_without_detaching_parent_authority() {
    for (result, field, guarantee, valid) in [
        ("Child", "child", "trusted child_owned(&result.marker)", true),
        ("Marker", "marker", "trusted parent_owned(&result)", false),
        ("Marker", "marker", "!flag || trusted parent_owned(&result)", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            using core.memory;
            struct Marker
            {{
            }}
            struct ChildMarker
            {{
            }}
            struct Child
            {{
                marker: ChildMarker;
                value: u32;
            }}
            struct Parent
            {{
                marker: Marker;
                mut child: Child;
            }}
            trusted predicate parent_owned(marker: &Marker);
            trusted predicate child_owned(marker: &ChildMarker);
            trusted func make() -> Parent
                ensures(trusted parent_owned(&result.marker)) executes(pure, total)
            {{
                return
                {{
                    marker = Marker {{}}, child =
                    {{
                        marker = ChildMarker {{}}, value = 1
                    }}
                }};
            }}
            trusted func extract(pos owner: &mut Parent) -> Child
                requires(trusted parent_owned(&owner.marker))
                ensures(trusted parent_owned(&owner.marker), trusted child_owned(&result.marker))
                uses(raw_memory, unchecked_init)
            {{
                let pointer = core.memory.address_of_mut<Child>(&mut owner.child);
                let previous = trusted core.memory.read<Child>(pointer);
                trusted core.memory.write<Child>(pointer, {{ marker = ChildMarker {{}}, value = 0 }});
                return previous;
            }}
            func observe_parent(pos owner: &Parent)
                requires(trusted parent_owned(&owner.marker)) executes(pure, total) {{}}
            func local_transition() -> Child
                ensures(trusted child_owned(&result.marker))
            {{
                let mut owner = make();
                let child = extract(&mut owner);
                observe_parent(&owner);
                return child;
            }}
            trusted func take(pos owner: Parent, pos flag: bool) -> {result}
                requires(trusted parent_owned(&owner.marker))
                ensures({guarantee})
                uses(raw_memory)
            {{
                return trusted core.memory.read<{result}>(core.memory.address_of<{result}>(&owner.{field}));
            }}
            func caller(pos owner: Parent, pos flag: bool) -> {result}
                requires(trusted parent_owned(&owner.marker))
            {{
                return take(owner, flag);
            }}
            "#
        );

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            );
        }
    }
}

#[test]
fn structural_storage_preserves_whole_moved_and_fresh_owner_guarantees() {
    for body in [
        "let value = owner(); let holder: Holder = { owner = value }; observe(&holder.owner);",
        "let holder: Holder = { owner = owner() }; observe(&holder.owner);",
        "let holder: Holder = { owner = { let value = owner(); yield value; } }; observe(&holder.owner);",
        "let nested: Nested = { holder = { owner = owner() } }; observe(&nested.holder.owner);",
        "let holder: Holder = { owner = owner() }; let moved = holder; observe(&moved.owner);",
        "let holder: Holder = { owner = owner() }; let moved = holder.owner; observe(&moved);",
        "let mut holder: Holder = { owner = { epoch = 0 } }; holder.owner = owner(); observe(&holder.owner);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
            }}
            struct Holder
            {{
                mut owner: Owner;
            }}
            struct Nested
            {{
                holder: Holder;
            }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner
                ensures(trusted live(&result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func observe(pos value: &Owner)
                requires(trusted live(value))
            {{
            }}
            func caller()
            {{
                {body}
            }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn destructured_storage_preserves_whole_owner_guarantees() {
    for body in [
        "let number: u8 = 0; let (value, copied) = (owner(), number); observe(&value);",
        "let [value]: [Owner; 1] = [owner()]; observe(&value);",
        "let holder = Holder.Owned(owner()); match consume holder { case .Owned(value) { observe(&value); } }",
        "let value: Owner = owner(); let holder: Owner? = value; match consume holder { case ?present { observe(&present); } case none {} }",
        "let value = (owner(), owner()); observe(&value.0);",
        "let value = (owner(), owner()); observe(&value.1);",
        "let value: [Owner; 1] = [owner()]; observe(&value[0]);",
        "let number: u8 = 0; let (_, value) = (number, owner()); observe(&value);",
        "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&first);",
        "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&last);",
        "let value = { let number: u8 = 0; let pair = (owner(), number); let (value, copied) = pair; yield value; }; observe(&value);",
        "let value = { let values: [Owner; 1] = [owner()]; let [value] = values; yield value; }; observe(&value);",
    ] {
        let compilation = compilation(&format!(
            r#"
            trusted module app;
            struct Owner
            {{
                epoch: u64;
            }}
            union Holder
            {{
                Owned(pos value: Owner);
            }}
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner
                executes(pure, total)
                ensures(trusted live(&result))
            {{
                return
                {{
                    epoch = 1
                }};
            }}
            func observe(pos value: &Owner)
                executes(pure, total)
                requires(trusted live(value))
            {{
            }}
            func caller()
            {{
                {body}
            }}
        "#
        ));

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );
    }
}
