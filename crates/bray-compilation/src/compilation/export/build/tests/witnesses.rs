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
    (
        "unrelated_scalars_valid",
        r"
        let unrelated_0: u32 = 0;
        let unrelated_1: u32 = 1;
        let unrelated_2: u32 = 2;
        let unrelated_3: u32 = 3;
        let unrelated_4: u32 = 4;
        let unrelated_5: u32 = 5;
        let unrelated_6: u32 = 6;
        let unrelated_7: u32 = 7;
        let unrelated_8: u32 = 8;
        let unrelated_9: u32 = 9;
        let unrelated_10: u32 = 10;
        let unrelated_11: u32 = 11;
        let unrelated_12: u32 = 12;
        let unrelated_13: u32 = 13;
        let unrelated_14: u32 = 14;
        let unrelated_15: u32 = 15;
        let unrelated_16: u32 = 16;
        let unrelated_17: u32 = 17;
        let unrelated_18: u32 = 18;
        let unrelated_19: u32 = 19;
        let unrelated_20: u32 = 20;
        let unrelated_21: u32 = 21;
        let unrelated_22: u32 = 22;
        let unrelated_23: u32 = 23;
        let unrelated_24: u32 = 24;
        let unrelated_25: u32 = 25;
        let unrelated_26: u32 = 26;
        let unrelated_27: u32 = 27;
        let unrelated_28: u32 = 28;
        let unrelated_29: u32 = 29;
        let unrelated_30: u32 = 30;
        let unrelated_31: u32 = 31;
        let unrelated_32: u32 = 32;
        let unrelated_33: u32 = 33;
        let unrelated_34: u32 = 34;
        let unrelated_35: u32 = 35;
        let unrelated_36: u32 = 36;
        let unrelated_37: u32 = 37;
        let unrelated_38: u32 = 38;
        let unrelated_39: u32 = 39;
        let unrelated_40: u32 = 40;
        let unrelated_41: u32 = 41;
        let unrelated_42: u32 = 42;
        let unrelated_43: u32 = 43;
        let unrelated_44: u32 = 44;
        let unrelated_45: u32 = 45;
        let unrelated_46: u32 = 46;
        let unrelated_47: u32 = 47;
        let unrelated_48: u32 = 48;
        let unrelated_49: u32 = 49;
        let unrelated_50: u32 = 50;
        let unrelated_51: u32 = 51;
        let unrelated_52: u32 = 52;
        let unrelated_53: u32 = 53;
        let unrelated_54: u32 = 54;
        let unrelated_55: u32 = 55;
        let unrelated_56: u32 = 56;
        let unrelated_57: u32 = 57;
        let unrelated_58: u32 = 58;
        let unrelated_59: u32 = 59;
        let unrelated_60: u32 = 60;
        let unrelated_61: u32 = 61;
        let unrelated_62: u32 = 62;
        let unrelated_63: u32 = 63;
        let unrelated_64: u32 = 64;
        let unrelated_65: u32 = 65;
        let unrelated_66: u32 = 66;
        let unrelated_67: u32 = 67;
        let unrelated_68: u32 = 68;
        let unrelated_69: u32 = 69;

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
        "unrelated_scalars_forged",
        r"
        let unrelated_0: u32 = 0;
        let unrelated_1: u32 = 1;
        let unrelated_2: u32 = 2;
        let unrelated_3: u32 = 3;
        let unrelated_4: u32 = 4;
        let unrelated_5: u32 = 5;
        let unrelated_6: u32 = 6;
        let unrelated_7: u32 = 7;
        let unrelated_8: u32 = 8;
        let unrelated_9: u32 = 9;
        let unrelated_10: u32 = 10;
        let unrelated_11: u32 = 11;
        let unrelated_12: u32 = 12;
        let unrelated_13: u32 = 13;
        let unrelated_14: u32 = 14;
        let unrelated_15: u32 = 15;
        let unrelated_16: u32 = 16;
        let unrelated_17: u32 = 17;
        let unrelated_18: u32 = 18;
        let unrelated_19: u32 = 19;
        let unrelated_20: u32 = 20;
        let unrelated_21: u32 = 21;
        let unrelated_22: u32 = 22;
        let unrelated_23: u32 = 23;
        let unrelated_24: u32 = 24;
        let unrelated_25: u32 = 25;
        let unrelated_26: u32 = 26;
        let unrelated_27: u32 = 27;
        let unrelated_28: u32 = 28;
        let unrelated_29: u32 = 29;
        let unrelated_30: u32 = 30;
        let unrelated_31: u32 = 31;
        let unrelated_32: u32 = 32;
        let unrelated_33: u32 = 33;
        let unrelated_34: u32 = 34;
        let unrelated_35: u32 = 35;
        let unrelated_36: u32 = 36;
        let unrelated_37: u32 = 37;
        let unrelated_38: u32 = 38;
        let unrelated_39: u32 = 39;
        let unrelated_40: u32 = 40;
        let unrelated_41: u32 = 41;
        let unrelated_42: u32 = 42;
        let unrelated_43: u32 = 43;
        let unrelated_44: u32 = 44;
        let unrelated_45: u32 = 45;
        let unrelated_46: u32 = 46;
        let unrelated_47: u32 = 47;
        let unrelated_48: u32 = 48;
        let unrelated_49: u32 = 49;
        let unrelated_50: u32 = 50;
        let unrelated_51: u32 = 51;
        let unrelated_52: u32 = 52;
        let unrelated_53: u32 = 53;
        let unrelated_54: u32 = 54;
        let unrelated_55: u32 = 55;
        let unrelated_56: u32 = 56;
        let unrelated_57: u32 = 57;
        let unrelated_58: u32 = 58;
        let unrelated_59: u32 = 59;
        let unrelated_60: u32 = 60;
        let unrelated_61: u32 = 61;
        let unrelated_62: u32 = 62;
        let unrelated_63: u32 = 63;
        let unrelated_64: u32 = 64;
        let unrelated_65: u32 = 65;
        let unrelated_66: u32 = 66;
        let unrelated_67: u32 = 67;
        let unrelated_68: u32 = 68;
        let unrelated_69: u32 = 69;

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
        "unrelated_witnesses_valid",
        r"
        let unrelated_0 = api.make();
        let unrelated_1 = api.make();
        let unrelated_2 = api.make();
        let unrelated_3 = api.make();
        let unrelated_4 = api.make();
        let unrelated_5 = api.make();
        let unrelated_6 = api.make();
        let unrelated_7 = api.make();
        let unrelated_8 = api.make();
        let unrelated_9 = api.make();
        let unrelated_10 = api.make();
        let unrelated_11 = api.make();
        let unrelated_12 = api.make();
        let unrelated_13 = api.make();
        let unrelated_14 = api.make();
        let unrelated_15 = api.make();
        let unrelated_16 = api.make();
        let unrelated_17 = api.make();
        let unrelated_18 = api.make();
        let unrelated_19 = api.make();
        let unrelated_20 = api.make();
        let unrelated_21 = api.make();
        let unrelated_22 = api.make();
        let unrelated_23 = api.make();
        let unrelated_24 = api.make();
        let unrelated_25 = api.make();
        let unrelated_26 = api.make();
        let unrelated_27 = api.make();
        let unrelated_28 = api.make();
        let unrelated_29 = api.make();
        let unrelated_30 = api.make();
        let unrelated_31 = api.make();
        let unrelated_32 = api.make();
        let unrelated_33 = api.make();
        let unrelated_34 = api.make();
        let unrelated_35 = api.make();
        let unrelated_36 = api.make();
        let unrelated_37 = api.make();
        let unrelated_38 = api.make();
        let unrelated_39 = api.make();
        let unrelated_40 = api.make();
        let unrelated_41 = api.make();
        let unrelated_42 = api.make();
        let unrelated_43 = api.make();
        let unrelated_44 = api.make();
        let unrelated_45 = api.make();
        let unrelated_46 = api.make();
        let unrelated_47 = api.make();
        let unrelated_48 = api.make();
        let unrelated_49 = api.make();
        let unrelated_50 = api.make();
        let unrelated_51 = api.make();
        let unrelated_52 = api.make();
        let unrelated_53 = api.make();
        let unrelated_54 = api.make();
        let unrelated_55 = api.make();
        let unrelated_56 = api.make();
        let unrelated_57 = api.make();
        let unrelated_58 = api.make();
        let unrelated_59 = api.make();
        let unrelated_60 = api.make();
        let unrelated_61 = api.make();
        let unrelated_62 = api.make();
        let unrelated_63 = api.make();
        let unrelated_64 = api.make();
        let unrelated_65 = api.make();
        let unrelated_66 = api.make();
        let unrelated_67 = api.make();
        let unrelated_68 = api.make();
        let unrelated_69 = api.make();
        let unrelated_70 = api.make();
        let unrelated_71 = api.make();
        let unrelated_72 = api.make();
        let unrelated_73 = api.make();
        let unrelated_74 = api.make();
        let unrelated_75 = api.make();
        let unrelated_76 = api.make();
        let unrelated_77 = api.make();
        let unrelated_78 = api.make();
        let unrelated_79 = api.make();
        let unrelated_80 = api.make();
        let unrelated_81 = api.make();
        let unrelated_82 = api.make();
        let unrelated_83 = api.make();
        let unrelated_84 = api.make();
        let unrelated_85 = api.make();
        let unrelated_86 = api.make();
        let unrelated_87 = api.make();
        let unrelated_88 = api.make();
        let unrelated_89 = api.make();
        let unrelated_90 = api.make();
        let unrelated_91 = api.make();
        let unrelated_92 = api.make();
        let unrelated_93 = api.make();
        let unrelated_94 = api.make();
        let unrelated_95 = api.make();
        let unrelated_96 = api.make();
        let unrelated_97 = api.make();
        let unrelated_98 = api.make();
        let unrelated_99 = api.make();
        let unrelated_100 = api.make();
        let unrelated_101 = api.make();
        let unrelated_102 = api.make();
        let unrelated_103 = api.make();
        let unrelated_104 = api.make();
        let unrelated_105 = api.make();
        let unrelated_106 = api.make();
        let unrelated_107 = api.make();
        let unrelated_108 = api.make();
        let unrelated_109 = api.make();
        let unrelated_110 = api.make();
        let unrelated_111 = api.make();
        let unrelated_112 = api.make();
        let unrelated_113 = api.make();
        let unrelated_114 = api.make();
        let unrelated_115 = api.make();
        let unrelated_116 = api.make();
        let unrelated_117 = api.make();
        let unrelated_118 = api.make();
        let unrelated_119 = api.make();
        let unrelated_120 = api.make();
        let unrelated_121 = api.make();
        let unrelated_122 = api.make();
        let unrelated_123 = api.make();
        let unrelated_124 = api.make();
        let unrelated_125 = api.make();
        let unrelated_126 = api.make();
        let unrelated_127 = api.make();
        let unrelated_128 = api.make();
        let unrelated_129 = api.make();

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
        "unrelated_witnesses_forged",
        r"
        let unrelated_0 = api.make();
        let unrelated_1 = api.make();
        let unrelated_2 = api.make();
        let unrelated_3 = api.make();
        let unrelated_4 = api.make();
        let unrelated_5 = api.make();
        let unrelated_6 = api.make();
        let unrelated_7 = api.make();
        let unrelated_8 = api.make();
        let unrelated_9 = api.make();
        let unrelated_10 = api.make();
        let unrelated_11 = api.make();
        let unrelated_12 = api.make();
        let unrelated_13 = api.make();
        let unrelated_14 = api.make();
        let unrelated_15 = api.make();
        let unrelated_16 = api.make();
        let unrelated_17 = api.make();
        let unrelated_18 = api.make();
        let unrelated_19 = api.make();
        let unrelated_20 = api.make();
        let unrelated_21 = api.make();
        let unrelated_22 = api.make();
        let unrelated_23 = api.make();
        let unrelated_24 = api.make();
        let unrelated_25 = api.make();
        let unrelated_26 = api.make();
        let unrelated_27 = api.make();
        let unrelated_28 = api.make();
        let unrelated_29 = api.make();
        let unrelated_30 = api.make();
        let unrelated_31 = api.make();
        let unrelated_32 = api.make();
        let unrelated_33 = api.make();
        let unrelated_34 = api.make();
        let unrelated_35 = api.make();
        let unrelated_36 = api.make();
        let unrelated_37 = api.make();
        let unrelated_38 = api.make();
        let unrelated_39 = api.make();
        let unrelated_40 = api.make();
        let unrelated_41 = api.make();
        let unrelated_42 = api.make();
        let unrelated_43 = api.make();
        let unrelated_44 = api.make();
        let unrelated_45 = api.make();
        let unrelated_46 = api.make();
        let unrelated_47 = api.make();
        let unrelated_48 = api.make();
        let unrelated_49 = api.make();
        let unrelated_50 = api.make();
        let unrelated_51 = api.make();
        let unrelated_52 = api.make();
        let unrelated_53 = api.make();
        let unrelated_54 = api.make();
        let unrelated_55 = api.make();
        let unrelated_56 = api.make();
        let unrelated_57 = api.make();
        let unrelated_58 = api.make();
        let unrelated_59 = api.make();
        let unrelated_60 = api.make();
        let unrelated_61 = api.make();
        let unrelated_62 = api.make();
        let unrelated_63 = api.make();
        let unrelated_64 = api.make();
        let unrelated_65 = api.make();
        let unrelated_66 = api.make();
        let unrelated_67 = api.make();
        let unrelated_68 = api.make();
        let unrelated_69 = api.make();
        let unrelated_70 = api.make();
        let unrelated_71 = api.make();
        let unrelated_72 = api.make();
        let unrelated_73 = api.make();
        let unrelated_74 = api.make();
        let unrelated_75 = api.make();
        let unrelated_76 = api.make();
        let unrelated_77 = api.make();
        let unrelated_78 = api.make();
        let unrelated_79 = api.make();
        let unrelated_80 = api.make();
        let unrelated_81 = api.make();
        let unrelated_82 = api.make();
        let unrelated_83 = api.make();
        let unrelated_84 = api.make();
        let unrelated_85 = api.make();
        let unrelated_86 = api.make();
        let unrelated_87 = api.make();
        let unrelated_88 = api.make();
        let unrelated_89 = api.make();
        let unrelated_90 = api.make();
        let unrelated_91 = api.make();
        let unrelated_92 = api.make();
        let unrelated_93 = api.make();
        let unrelated_94 = api.make();
        let unrelated_95 = api.make();
        let unrelated_96 = api.make();
        let unrelated_97 = api.make();
        let unrelated_98 = api.make();
        let unrelated_99 = api.make();
        let unrelated_100 = api.make();
        let unrelated_101 = api.make();
        let unrelated_102 = api.make();
        let unrelated_103 = api.make();
        let unrelated_104 = api.make();
        let unrelated_105 = api.make();
        let unrelated_106 = api.make();
        let unrelated_107 = api.make();
        let unrelated_108 = api.make();
        let unrelated_109 = api.make();
        let unrelated_110 = api.make();
        let unrelated_111 = api.make();
        let unrelated_112 = api.make();
        let unrelated_113 = api.make();
        let unrelated_114 = api.make();
        let unrelated_115 = api.make();
        let unrelated_116 = api.make();
        let unrelated_117 = api.make();
        let unrelated_118 = api.make();
        let unrelated_119 = api.make();
        let unrelated_120 = api.make();
        let unrelated_121 = api.make();
        let unrelated_122 = api.make();
        let unrelated_123 = api.make();
        let unrelated_124 = api.make();
        let unrelated_125 = api.make();
        let unrelated_126 = api.make();
        let unrelated_127 = api.make();
        let unrelated_128 = api.make();
        let unrelated_129 = api.make();

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
