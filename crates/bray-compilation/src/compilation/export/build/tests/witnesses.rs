use super::fixtures::{compilation, execution_consumer};

const NESTED_ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

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

    func forward_owned_inferred(pos value: Receipt?, pos state: &mut State) -> (u32, Receipt?)
        requires(trusted owned(owner = &value))
    {
        state.value += 1;
        return (0, value);
    }

    func forward_nullable_inferred(pos value: Receipt?, pos state: &mut State) -> Receipt?
        requires(trusted owned(owner = &value))
    {
        state.value += 1;
        return value;
    }

    func create_owned_tuple_inferred(pos state: &mut State) -> (u32, Receipt?)
    {
        state.value += 1;
        return (0, make_owned());
    }

    func forward_address_inferred(pos value: Address, pos state: &mut State) -> (u32, Address)
        requires(
            trusted held(owner = &value.marker),
            trusted core.memory.valid_read<u32>(pointer = value.pointer, count = 1)
        )
    {
        state.value += 1;
        return (0, value);
    }

    func select_inactive_result(
        pos prior: Result<Receipt?, usize>,
        pos existing: bool,
        pos repeat: bool
    ) -> Result<Receipt?, usize>
    {
        return if existing
        {
            yield Ok(none);
        }
        else if repeat && prior matches Error(_)
        {
            yield prior;
        }
        else
        {
            yield Ok(make());
        };
    }

    func select_active_result(
        pos prior: Result<Receipt?, usize>,
        pos existing: bool,
        pos repeat: bool
    ) -> Result<Receipt?, usize>
    {
        return if existing
        {
            yield Ok(make());
        }
        else if repeat && prior matches Ok(_)
        {
            yield prior;
        }
        else
        {
            yield Error(1);
        };
    }

    func select_absent_choice(pos prior: Choice?, pos existing: bool, pos repeat: bool) -> Choice?
    {
        return if existing
        {
            yield Choice.Existing(make());
        }
        else if repeat && prior matches none
        {
            yield prior;
        }
        else
        {
            yield Choice.Backing;
        };
    }

    func select_present_choice(pos prior: Choice?, pos existing: bool, pos repeat: bool) -> Choice?
    {
        return if existing
        {
            yield Choice.Existing(make());
        }
        else if repeat && prior matches ?_
        {
            yield prior;
        }
        else
        {
            yield Choice.Backing;
        };
    }
    ";

const NESTED_SHAPES: &[(&str, &str, [&str; 3], &str)] = &[
    (
        "nullable_choice",
        "api.Choice?",
        [
            "api.Choice.Existing(api.make())",
            "api.Choice.Backing",
            "none",
        ],
        r"
        match consume selected
        {
            case ?choice
            {
                match consume choice
                {
                    case api.Choice.Existing(value) { api.observe(&value); }
                    case api.Choice.Backing {}
                }
            }
            case none {}
        }
    ",
    ),
    (
        "result_nullable",
        "Result<api.Receipt?, usize>",
        ["Ok(api.make())", "Ok(none)", "Error(1)"],
        r"
        match consume selected
        {
            case Ok(optional)
            {
                match consume optional
                {
                    case ?value { api.observe(&value); }
                    case none {}
                }
            }
            case Error(_) {}
        }
    ",
    ),
    (
        "result_nullable_choice",
        "Result<api.Choice?, usize>",
        [
            "Ok(api.Choice.Backing)",
            "Error(1)",
            "Ok(api.Choice.Existing(api.make()))",
        ],
        r"
        match consume selected
        {
            case Ok(optional)
            {
                match consume optional
                {
                    case ?choice
                    {
                        match consume choice
                        {
                            case api.Choice.Existing(value) { api.observe(&value); }
                            case api.Choice.Backing {}
                        }
                    }
                    case none {}
                }
            }
            case Error(_) {}
        }
    ",
    ),
];

fn nested_values(values: [&str; 3], valid: bool) -> [String; 3] {
    values.map(|value| {
        if valid {
            value.to_owned()
        } else {
            value.replace("api.make()", "api.Receipt { marker = api.Marker {} }")
        }
    })
}

fn api_source() -> String {
    let mut source = API.to_owned();

    for &(kind, ty, values, _) in NESTED_SHAPES {
        for (index, order) in NESTED_ORDERS.into_iter().enumerate() {
            for valid in [true, false] {
                let values = nested_values(values, valid).map(|value| value.replace("api.", ""));
                let selection = nested_selection(order, values.each_ref().map(String::as_str));
                let ty = ty.replace("api.", "");

                source.push_str(&format!(
                    "func choose_nested_{kind}_{index}_{valid}(pos existing: bool, pos repeat: bool) -> {ty} {{ return {selection}; }}\n"
                ));
            }
        }
    }

    source
}

fn nested_selection(order: [usize; 3], values: [&str; 3]) -> String {
    let [first, second, third] = order;

    format!(
        "if existing {{ yield {}; }} else if repeat {{ yield {}; }} else {{ yield {}; }}",
        values[first], values[second], values[third]
    )
}

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
    (
        "owned_tuple_inferred",
        r"
        let value =
        {
            let mut state: api.State =
            {
                value = 0
            };

            yield api.forward_owned_inferred(api.make_owned(), &mut state);
        };

        api.observe_tuple(&value);
    ",
        true,
    ),
    (
        "owned_tuple_inferred_live",
        r"
        let mut state: api.State =
        {
            value = 0
        };

        let value = api.forward_owned_inferred(api.make_owned(), &mut state);

        api.observe_tuple(&value);
    ",
        true,
    ),
    (
        "owned_nullable_inferred",
        r"
        let value =
        {
            let mut state: api.State =
            {
                value = 0
            };

            yield api.forward_nullable_inferred(api.make_owned(), &mut state);
        };

        api.observe_owned(&value);
    ",
        true,
    ),
    (
        "new_owned_tuple_inferred",
        r"
        let value =
        {
            let mut state: api.State =
            {
                value = 0
            };

            yield api.create_owned_tuple_inferred(&mut state);
        };

        api.observe_tuple(&value);
    ",
        true,
    ),
    (
        "tuple_inferred_observe_address_marker",
        r"
        let value =
        {
            let payload: u32 = 7;

            yield
            {
                let mut state: api.State =
                {
                    value = 0
                };

                yield api.forward_address_inferred(api.make_address(&payload), &mut state);
            };
        };

        api.observe_address_marker(&value.1);
    ",
        true,
    ),
    (
        "tuple_inferred_observe_address",
        r"
        let value =
        {
            let payload: u32 = 7;

            yield
            {
                let mut state: api.State =
                {
                    value = 0
                };

                yield api.forward_address_inferred(api.make_address(&payload), &mut state);
            };
        };

        api.observe_address(&value.1);
    ",
        false,
    ),
    (
        "tuple_inferred_address_live",
        r"
        let payload: u32 = 7;

        let value =
        {
            let mut state: api.State =
            {
                value = 0
            };

            yield api.forward_address_inferred(api.make_address(&payload), &mut state);
        };

        api.observe_address(&value.1);
    ",
        true,
    ),
];

#[test]
fn witness_ownership_survives_source_control_flow() {
    let api = api_source();

    for (name, body, valid) in cases() {
        let body = body.replace("api.", "");

        let source = compilation(&format!(
            "{api}\nfunc caller(pos existing: bool, pos repeat: bool) {{ {body} }}"
        ));

        assert_witness_result(&name, &source, valid);
    }
}

#[test]
fn witness_ownership_survives_imported_control_flow() {
    let provider = compilation(&api_source());

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:?}",
        provider.check_diagnostics()
    );

    for (name, body, valid) in cases() {
        let body = body.replace("api.", "example.package.api.");

        let consumer = execution_consumer(
            &provider,
            &format!(
                "trusted module app; using core.memory; using example.package.api; func caller(pos existing: bool, pos repeat: bool) {{ {body} }}"
            ),
        );

        assert_witness_result(&name, &consumer, valid);
    }
}

fn cases() -> impl Iterator<Item = (String, String, bool)> {
    let scale = [
        ("unrelated_scalars_valid", 70, false, true),
        ("unrelated_scalars_forged", 70, false, false),
        ("unrelated_witnesses_valid", 130, true, true),
        ("unrelated_witnesses_forged", 130, true, false),
    ]
    .into_iter()
    .map(|(name, count, witnesses, valid)| {
        let prefix = (0..count)
            .map(|index| {
                if witnesses {
                    format!("let unrelated_{index} = api.make();\n")
                } else {
                    format!("let unrelated_{index}: u32 = {index};\n")
                }
            })
            .collect::<String>();

        let base = if valid { "direct" } else { "forged" };

        let body = CASES
            .iter()
            .find(|(name, _, _)| *name == base)
            .expect("scale fixtures reuse the existing valid and forged branch cases")
            .1;

        (name.to_owned(), format!("{prefix}{body}"), valid)
    });

    let nested = NESTED_SHAPES
        .iter()
        .flat_map(|&(kind, ty, values, matching)| {
            NESTED_ORDERS.into_iter().flat_map(move |order| {
                [true, false].into_iter().map(move |valid| {
                    let values = nested_values(values, valid);
                    let selection = nested_selection(order, values.each_ref().map(String::as_str));

                    let [first, second, third] = order;

                    let body = format!("let selected: {ty} = {selection}; {matching}");

                    (
                        format!("{kind}_{first}_{second}_{third}_{valid}"),
                        body,
                        valid,
                    )
                })
            })
        });

    let provider = NESTED_SHAPES.iter().flat_map(|&(kind, _, _, matching)| {
        NESTED_ORDERS.into_iter().enumerate().flat_map(move |(index, _)| {
            [true, false].into_iter().map(move |valid| {
                (
                    format!("{kind}_provider_{index}_{valid}"),
                    format!("let selected = api.choose_nested_{kind}_{index}_{valid}(existing, repeat); {matching}"),
                    valid,
                )
            })
        })
    });

    let guarded = [
        ("guarded_error_input", "let selected = api.select_inactive_result(Error(1), existing, repeat);", 1, true),
        ("guarded_error_forged", "let selected = api.select_active_result(Ok(api.Receipt { marker = api.Marker {} }), existing, repeat);", 1, false),
        ("guarded_none_input", "let selected = api.select_absent_choice(none, existing, repeat);", 0, true),
        ("guarded_present_forged", "let selected = api.select_present_choice(api.Choice.Existing(api.Receipt { marker = api.Marker {} }), existing, repeat);", 0, false),
    ]
    .into_iter()
    .map(|(name, body, shape, valid)| {
        (name.to_owned(), format!("{body}{}", NESTED_SHAPES[shape].3), valid)
    });

    CASES
        .iter()
        .map(|&(name, body, valid)| (name.to_owned(), body.to_owned(), valid))
        .chain(scale)
        .chain(nested)
        .chain(provider)
        .chain(guarded)
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
