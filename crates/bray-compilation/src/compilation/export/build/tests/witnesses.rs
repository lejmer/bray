use super::fixtures::{compilation, execution_consumer};

const API: &str = r"
    trusted module api;

    using core.memory;

    struct Marker
    {
    }

    struct Receipt
    {
        mut marker: Marker;
    }

    struct State
    {
        mut value: u32;
    }

    union Choice
    {
        Existing(pos value: Receipt);
        Backing;
    }

    trusted predicate held(owner: &Marker);

    trusted func make() -> Receipt
        ensures(trusted held(owner = &result.marker))
    {
        return
        {
            marker = Marker
            {
            }
        };
    }

    func observe(pos value: &Receipt)
        requires(trusted held(owner = &value.marker)) {}

    func choose(pos existing: bool) -> Choice
    {
        return if existing
        {
            yield Choice.Existing(make());
        }
        else
        {
            yield Choice.Backing;
        };
    }

    func choose_nullable(pos existing: bool) -> Receipt?
    {
        return if existing
        {
            let value: Receipt? = make();

            yield value;
        }
        else
        {
            yield none;
        };
    }

    trusted predicate owned(owner: &(Receipt?));

    trusted func make_owned() -> Receipt?
        ensures(trusted owned(owner = &result))
    {
        let value: Receipt? = make();

        return value;
    }

    func observe_owned(pos value: &(Receipt?))
        requires(trusted owned(owner = value)) {}

    func choose_owned(pos existing: bool) -> Receipt?
    {
        return if existing
        {
            yield make_owned();
        }
        else
        {
            yield none;
        };
    }

    trusted func forward_owned(pos value: Receipt?, pos state: &mut State) -> (u32, Receipt?)
        requires(trusted owned(owner = &value))
        ensures(trusted owned(owner = &result.1))
    {
        state.value += 1;
        return (0, value);
    }

    func observe_tuple(pos value: &(u32, Receipt?))
        requires(trusted owned(owner = &value.1)) {}

    struct Address
    {
        marker: Marker;
        pointer: DataPointer<u32>;
    }

    union AddressChoice
    {
        Existing(pos value: Address);
        Backing;
    }

    trusted func make_address(pos payload: &u32) -> Address
        ensures(
            trusted held(owner = &result.marker),
            trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
        )
    {
        return
        {
            marker = Marker
            {
            },
            pointer = core.memory.address_of<u32>(payload)
        };
    }

    func observe_address_marker(pos value: &Address)
        requires(trusted held(owner = &value.marker)) {}

    func observe_address(pos value: &Address)
        requires(trusted core.memory.valid_read<u32>(pointer = value.pointer, count = 1)) {}

    func choose_address(pos payload: &u32, pos present: bool) -> AddressChoice
    {
        return if present
        {
            yield AddressChoice.Existing(make_address(payload));
        }
        else
        {
            yield AddressChoice.Backing;
        };
    }

    func choose_nullable_implicit(pos existing: bool) -> Receipt?
    {
        return if existing
        {
            yield make();
        }
        else
        {
            yield none;
        };
    }

    func choose_nullable_break(pos existing: bool) -> Receipt?
    {
        return loop
        {
            if existing
            {
                break make();
            }

            break none;
        };
    }
    ";

const CASES: &[(&str, &str, bool)] = &[
    (
        "direct",
        r"
        let selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Backing;
        };

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        true,
    ),
    (
        "forged",
        r"
        let selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Existing(
                api.Receipt
                {
                    marker = api.Marker
                    {
                    }
                }
            );
        };

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        false,
    ),
    (
        "both_owned",
        r"
        let selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Existing(api.make());
        };

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        true,
    ),
    (
        "assigned",
        r"
        let mut selected: api.Choice = api.Choice.Backing;

        if existing
        {
            selected = api.Choice.Existing(api.make());
        }

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        true,
    ),
    (
        "nullable",
        r"
        let selected: api.Receipt? = if existing
        {
            let value: api.Receipt? = api.make();

            yield value;
        }
        else
        {
            yield none;
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "result",
        r"
        let selected: Result<api.Receipt, usize> = if existing
        {
            yield Ok(api.make());
        }
        else
        {
            yield Error(1);
        };

        match consume selected
        {
            case Ok(value) { api.observe(&value); }
            case Error(_) {}
        }
    ",
        true,
    ),
    (
        "nested",
        r"
        let selected: Result<api.Choice, usize> = if existing
        {
            yield Ok(api.Choice.Existing(api.make()));
        }
        else
        {
            yield Ok(api.Choice.Backing);
        };

        match consume selected
        {
            case Ok(choice)
            {
                match consume choice
                {
                    case api.Choice.Existing(value) { api.observe(&value); }
                    case api.Choice.Backing {}
                }
            }
            case Error(_) {}
        }
    ",
        true,
    ),
    (
        "forwarded",
        r"
        let selected = api.choose(existing);

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        true,
    ),
    (
        "forwarded_nullable",
        r"
        let selected = api.choose_nullable(existing);

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "optional_claim",
        r"
        let value = api.choose_owned(existing);

        if value matches ?_
        {
            api.observe_owned(&value);
        }
    ",
        true,
    ),
    (
        "optional_forged",
        r"
        let value = api.choose_owned(existing);

        api.observe_owned(&value);
    ",
        false,
    ),
    (
        "owned_tuple",
        r"
        let value =
        {
            let mut state: api.State =
            {
                value = 0
            };

            yield api.forward_owned(api.make_owned(), &mut state);
        };

        api.observe_tuple(&value);
    ",
        true,
    ),
    (
        "mutation",
        r"
        let mut value = api.make();

        value.marker = api.Marker
        {
        };

        api.observe(&value);
    ",
        false,
    ),
    (
        "loop",
        r"
        loop
        {
            let selected: api.Choice = if existing
            {
                yield api.Choice.Existing(api.make());
            }
            else
            {
                yield api.Choice.Backing;
            };

            match consume selected
            {
                case api.Choice.Existing(value) { api.observe(&value); }
                case api.Choice.Backing {}
            }

            if existing
            {
                return;
            }
        }
    ",
        true,
    ),
    (
        "observe_address_marker",
        r"
        let selected: api.AddressChoice =
        {
            let payload: u32 = 7;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&payload));
            }
            else
            {
                yield api.AddressChoice.Backing;
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address_marker(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        true,
    ),
    (
        "observe_address",
        r"
        let selected: api.AddressChoice =
        {
            let payload: u32 = 7;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&payload));
            }
            else
            {
                yield api.AddressChoice.Backing;
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        false,
    ),
    (
        "loop_carried",
        r"
        let mut selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Backing;
        };

        let mut again = repeat;

        loop
        {
            match &selected
            {
                case api.Choice.Existing(value) { api.observe(&value); }
                case api.Choice.Backing {}
            }

            if !again
            {
                return;
            }

            selected = if existing
            {
                yield api.Choice.Existing(api.make());
            }
            else
            {
                yield api.Choice.Backing;
            };

            again = false;
        }
    ",
        true,
    ),
    (
        "loop_exit",
        r"
        let mut selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Backing;
        };

        let mut again = repeat;

        while again
        {
            selected = if existing
            {
                yield api.Choice.Existing(api.make());
            }
            else
            {
                yield api.Choice.Backing;
            };

            again = false;
        }

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        true,
    ),
    (
        "loop_forged",
        r"
        let mut selected: api.Choice = if existing
        {
            yield api.Choice.Existing(api.make());
        }
        else
        {
            yield api.Choice.Backing;
        };

        let mut again = repeat;

        while again
        {
            selected = if existing
            {
                yield api.Choice.Existing(api.make());
            }
            else
            {
                yield api.Choice.Existing(
                    api.Receipt
                    {
                        marker = api.Marker
                        {
                        }
                    }
                );
            };

            again = false;
        }

        match consume selected
        {
            case api.Choice.Existing(value) { api.observe(&value); }
            case api.Choice.Backing {}
        }
    ",
        false,
    ),
    (
        "loop_nullable",
        r"
        let mut selected: api.Receipt? = if existing
        {
            let value: api.Receipt? = api.make();

            yield value;
        }
        else
        {
            yield none;
        };

        let mut again = repeat;

        while again
        {
            selected = if existing
            {
                let value: api.Receipt? = api.make();

                yield value;
            }
            else
            {
                yield none;
            };

            again = false;
        }

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "mixed_observe_address_marker_stable_first",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.choose_address(&stable, repeat);
            }
            else
            {
                yield api.AddressChoice.Existing(api.make_address(&short));
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address_marker(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        true,
    ),
    (
        "mixed_observe_address_stable_first",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.choose_address(&stable, repeat);
            }
            else
            {
                yield api.AddressChoice.Existing(api.make_address(&short));
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        false,
    ),
    (
        "mixed_observe_address_marker_stable_second",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&short));
            }
            else
            {
                yield api.choose_address(&stable, repeat);
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address_marker(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        true,
    ),
    (
        "mixed_observe_address_stable_second",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&short));
            }
            else
            {
                yield api.choose_address(&stable, repeat);
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        false,
    ),
    (
        "mixed_observe_address_marker_short_first",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.choose_address(&short, repeat);
            }
            else
            {
                yield api.AddressChoice.Existing(api.make_address(&stable));
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address_marker(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        true,
    ),
    (
        "mixed_observe_address_short_first",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.choose_address(&short, repeat);
            }
            else
            {
                yield api.AddressChoice.Existing(api.make_address(&stable));
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        false,
    ),
    (
        "mixed_observe_address_marker_short_second",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&stable));
            }
            else
            {
                yield api.choose_address(&short, repeat);
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address_marker(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        true,
    ),
    (
        "mixed_observe_address_short_second",
        r"
        let stable: u32 = 1;

        let selected: api.AddressChoice =
        {
            let short: u32 = 2;

            yield if existing
            {
                yield api.AddressChoice.Existing(api.make_address(&stable));
            }
            else
            {
                yield api.choose_address(&short, repeat);
            };
        };

        match consume selected
        {
            case api.AddressChoice.Existing(value) { api.observe_address(&value); }
            case api.AddressChoice.Backing {}
        }
    ",
        false,
    ),
    (
        "implicit_yield",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.make();
        }
        else
        {
            yield none;
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "implicit_block_yield",
        r"
        let selected: api.Receipt? =
        {
            yield api.make();
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "implicit_break",
        r"
        let selected: api.Receipt? = loop
        {
            if existing
            {
                break api.make();
            }

            break none;
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "implicit_match_repack",
        r"
        let choice = api.choose(existing);

        let selected: api.Receipt? = match consume choice
        {
            case api.Choice.Existing(value)
            {
                api.observe(&value);
                yield value;
            }
            case api.Choice.Backing { yield none; }
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "forwarded_implicit_yield",
        r"
        let selected = api.choose_nullable_implicit(existing);

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "forwarded_implicit_break",
        r"
        let selected = api.choose_nullable_break(existing);

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "implicit_forged",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.make();
        }
        else
        {
            yield api.Receipt
            {
                marker = api.Marker
                {
                }
            };
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        false,
    ),
    (
        "nullable_mixed",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.choose_nullable(repeat);
        }
        else
        {
            yield api.make();
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_mixed_reverse",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.make();
        }
        else
        {
            yield api.choose_nullable(repeat);
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_mixed_explicit",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.choose_nullable(repeat);
        }
        else
        {
            let value: api.Receipt? = api.make();

            yield value;
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_mixed_inline",
        r"
        let selected: api.Receipt? = if existing
        {
            yield if repeat
            {
                yield api.make();
            }
            else
            {
                yield none;
            };
        }
        else
        {
            yield api.make();
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_mixed_assignment",
        r"
        let mut selected = api.choose_nullable(repeat);

        if existing
        {
            selected = api.make();
        }

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_both_conditional",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.choose_nullable(repeat);
        }
        else
        {
            yield api.choose_nullable(existing);
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        true,
    ),
    (
        "nullable_mixed_forged",
        r"
        let selected: api.Receipt? = if existing
        {
            yield api.choose_nullable(repeat);
        }
        else
        {
            yield api.Receipt
            {
                marker = api.Marker
                {
                }
            };
        };

        match consume selected
        {
            case ?value { api.observe(&value); }
            case none {}
        }
    ",
        false,
    ),
];

#[test]
fn witness_ownership_survives_source_control_flow() {
    for (name, body, valid) in CASES {
        let body = body.replace("api.", "");

        let source = compilation(&format!(
            "{API}\nfunc caller(pos existing: bool, pos repeat: bool) {{ {body} }}"
        ));

        assert_witness_result(name, &source, *valid);
    }
}

#[test]
fn witness_ownership_survives_imported_control_flow() {
    let provider = compilation(API);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (name, body, valid) in CASES {
        let body = body.replace("api.", "example.package.api.");

        let consumer = execution_consumer(
            &provider,
            &format!(
                "trusted module app; using core.memory; using example.package.api; func caller(pos existing: bool, pos repeat: bool) {{ {body} }}"
            ),
        );

        assert_witness_result(name, &consumer, *valid);
    }
}

fn assert_witness_result(name: &str, compilation: &crate::Compilation, valid: bool) {
    let diagnostics = compilation.check_diagnostics();

    assert_eq!(!diagnostics.has_errors(), valid, "{name}: {diagnostics:?}");

    if !valid {
        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}
