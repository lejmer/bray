use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

const API: &str = r"
    trusted module api;

    using core.memory;

    struct Marker
    {
    }

    struct Receipt
    {
        marker: Marker;
    }

    struct State
    {
        mut count: u32;
    }

    func reference(pos value: &u32) {}

    struct Box<T>
    {
        value: T;
    }

    union Choice<T>
    {
        Value(pos value: T);
        Empty;
    }

    trusted predicate held(owner: &(Receipt?));

    trusted func make() -> Receipt?
        ensures(trusted held(owner = &result))
    {
        let value: Receipt? = Receipt
        {
            marker = Marker
            {
            }
        };

        return value;
    }

    func observe(pos value: &(Receipt?))
        requires(trusted held(owner = value)) {}

    @copy
    struct Stamp
    {
        epoch: u64;
    }

    trusted predicate stamped(owner: &Stamp);

    trusted func stamp() -> Stamp
        ensures(trusted stamped(owner = &result))
    {
        return
        {
            epoch = 1
        };
    }

    func observe_stamp(pos value: &Stamp)
        requires(trusted stamped(owner = value)) {}

    func extract<T>(pos value: Box<T>) -> (u32, T)
    {
        return (0, value.value);
    }

    func pair<T>(pos left: T, pos right: T) -> (T, T)
    {
        return (left, right);
    }

    func extract_optional<T>(pos value: Box<T>, pos take: bool) -> T?
        ensures(take || result matches none)
    {
        if !take
        {
            return none;
        }

        return value.value;
    }

    async func deferred_extract_optional<T>(pos value: Box<T>, pos take: bool) -> T?
        ensures(take || result matches none)
    {
        await pause();

        if !take
        {
            return none;
        }

        return value.value;
    }

    func extract_choice<T>(pos value: Box<T>, pos take: bool) -> Choice<T>
        ensures(take || result matches .Empty)
    {
        if !take
        {
            return .Empty;
        }

        return .Value(value.value);
    }

    async func deferred_extract_choice<T>(pos value: Box<T>, pos take: bool) -> Choice<T>
        ensures(take || result matches .Empty)
    {
        await pause();

        if !take
        {
            return .Empty;
        }

        return .Value(value.value);
    }

    async func deferred_extract<T>(pos value: Box<T>) -> T
    {
        return value.value;
    }

    async func deferred_tuple<T>(pos value: T) -> (u32, T)
    {
        return (0, value);
    }

    func tuple<T>(pos value: T) -> (u32, T)
    {
        return (0, value);
    }

    func product<T>(pos value: T) -> Box<T>
    {
        return
        {
            value = value
        };
    }

    func variant<T>(pos value: T) -> Choice<T>
    {
        return .Value(value);
    }

    func nullable<T>(pos value: T) -> T?
    {
        return value;
    }

    func array<T>(pos value: T) -> [T; 1]
    {
        return[value];
    }

    func nested<T>(pos value: T) -> Box<(u32, T?)>
    {
        let optional: T? = value;
        let wrapped: (u32, T?) = (0, optional);

        return
        {
            value = wrapped
        };
    }

    func select_variant<T>(pos value: T, pos take: bool) -> Choice<T>
    {
        return if take
        {
            yield .Value(value);
        }
        else
        {
            yield .Empty;
        };
    }

    func select_nullable<T>(pos value: T, pos take: bool) -> T?
    {
        return if take
        {
            yield value;
        }
        else
        {
            yield none;
        };
    }

    func select_exits<T>(pos value: T, pos take: bool) -> T?
    {
        if take
        {
            return value;
        }

        return none;
    }

    func select_reversed<T>(pos value: T, pos take: bool) -> T?
    {
        if take
        {
            return none;
        }

        return value;
    }

    func select_optional<T>(pos value: T?, pos take: bool) -> T?
    {
        if take
        {
            return value;
        }

        return none;
    }

    func select_optional_join<T>(pos value: T?, pos take: bool) -> T?
    {
        return if take
        {
            yield value;
        }
        else
        {
            yield none;
        };
    }

    func select_nested<T>(pos value: T, pos take: bool) -> Box<Choice<T>>
    {
        return
        {
            value = select_variant<T>(value, take)
        };
    }

    async func select_deferred<T>(pos value: T, pos take: bool) -> Choice<T>
    {
        return if take
        {
            yield .Value(value);
        }
        else
        {
            yield .Empty;
        };
    }

    trusted func absent() -> Receipt?
        ensures(trusted held(owner = &result))
    {
        return none;
    }

    trusted func make_marker() -> Marker
        ensures(trusted marked(owner = &result))
    {
        return Marker
        {
        };
    }

    trusted func marker_box() -> Box<Marker>
        ensures(trusted marked(owner = &result.value))
    {
        return
        {
            value = Marker
            {
            }
        };
    }

    func inspect_marker(pos value: &Marker)
        requires(trusted marked(owner = value)) {}

    func factory() -> [Receipt?; 2]
    {
        return[make(), make()];
    }

    func marker_factory() -> [Marker; 2]
    {
        return[make_marker(), make_marker()];
    }

    func conditional_factory(pos take: bool) -> [Marker?; 2]
    {
        let value: Marker? = if take
        {
            yield make_marker();
        }
        else
        {
            yield none;
        };

        let other: Marker? = make_marker();

        return[value, other];
    }

    struct Resource
    {
        mut marker: Marker;

        destruct()
        {
            self.marker = Marker
            {
            };
        }
    }

    trusted func resource() -> Resource
        ensures(trusted marked(owner = &result.marker))
    {
        return
        {
            marker = Marker
            {
            }
        };
    }

    func inspect_resource(pos value: &Resource)
        requires(trusted marked(owner = &value.marker)) {}

    struct ExternalResource
    {
        state: &mut Dependency;

        destruct()
        {
            self.state.marker = Marker
            {
            };
        }
    }

    func cleanup_referent(pos state: &mut Dependency)
    {
        let owner: ExternalResource =
        {
            state = state
        };
    }

    func select_lambda<T>(pos value: T, pos take: bool) -> Choice<T>
    {
        let choose = lambda(pos item: T, pos enabled: bool) -> Choice<T>
        {
            return if enabled
            {
                yield .Value(item);
            }
            else
            {
                yield .Empty;
            };
        };

        return choose(value, take);
    }

    func discard<T>(pos value: T) {}

    async func deferred_discard<T>(pos value: T) {}

    func observe_borrow(pos value: &u32) {}

    async func pause() {}

    async func select_await<T>(pos value: T, pos take: bool, pos state: &mut State) -> Choice<T>
    {
        await pause();
        state.count += 1;

        return if take
        {
            yield .Value(value);
        }
        else
        {
            yield .Empty;
        };
    }

    struct Permit
    {
        id: u32;
    }

    struct Sink
    {
        construct(pos value: Permit) -> Self
        {
            return Sink
            {
            };
        }
    }

    trusted predicate permitted(id: u32);

    trusted func permission() -> Permit
        ensures(trusted permitted(id = result.id))
    {
        return
        {
            id = 1
        };
    }

    func inspect_permission(pos id: u32)
        requires(trusted permitted(id = id)) {}

    func with_reference<T>(pos value: T, pos other: &u32) -> (T, &u32)
    {
        return (value, other);
    }

    func mutate<T>(pos value: T, pos state: &mut State) -> (u32, T)
    {
        state.count += 1;
        return (state.count, value);
    }

    func branches<T>(pos value: T, pos choice: bool) -> (u32, T)
    {
        if choice
        {
            return (1, value);
        }

        return (2, value);
    }

    struct Dependency
    {
        mut marker: Marker;
    }

    struct RelatedOwner
    {
        marker: Marker;
    }

    trusted predicate related(owner: &Marker, state: &Marker);

    trusted func make_related(pos state: &Dependency) -> RelatedOwner
        ensures(trusted marked(owner = &result.marker), trusted related(owner = &result.marker, state = &state.marker))
    {
        return
        {
            marker = Marker
            {
            }
        };
    }

    func replace(pos value: RelatedOwner, pos state: &mut Dependency) -> (u32, RelatedOwner)
    {
        state.marker = Marker
        {
        };

        return (0, value);
    }

    func keep(pos value: RelatedOwner, pos state: &mut Dependency) -> (u32, RelatedOwner)
    {
        return (0, value);
    }

    func observe_independent(pos value: &RelatedOwner)
        requires(trusted marked(owner = &value.marker)) {}

    func observe_related(pos value: &RelatedOwner, pos state: &Dependency)
        requires(trusted related(owner = &value.marker, state = &state.marker)) {}

    struct Address
    {
        marker: Marker;
        pointer: DataPointer<u32>;
    }

    trusted predicate marked(owner: &Marker);

    trusted func address(pos value: &u32) -> Address
        ensures(
            trusted marked(owner = &result.marker),
            trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
        )
    {
        return
        {
            marker = Marker
            {
            },
            pointer = core.memory.address_of<u32>(value)
        };
    }

    func observe_marker(pos value: &Address)
        requires(trusted marked(owner = &value.marker)) {}

    func observe_address(pos value: &Address)
        requires(trusted core.memory.valid_read<u32>(pointer = value.pointer, count = 1)) {}
";

fn check_owned_transfers(imported: bool) {
    let provider = compilation(API);

    assert!(
        !provider.check_diagnostics().has_errors(),
        "{:#?}",
        provider.check_diagnostics()
    );

    let cases = [
        (
            "deferred",
            "let result = await api.deferred_tuple<(api.Receipt?)>(INPUT); api.observe(&result.1);",
        ),
        (
            "tuple",
            "let result = api.tuple<(api.Receipt?)>(INPUT); api.observe(&result.1);",
        ),
        (
            "product",
            "let result = api.product<(api.Receipt?)>(INPUT); api.observe(&result.value);",
        ),
        (
            "union",
            "let result = api.variant<(api.Receipt?)>(INPUT); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "nullable",
            "let result = api.nullable<(api.Receipt?)>(INPUT); match consume result { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "array",
            "let result = api.array<(api.Receipt?)>(INPUT); let [value] = result; api.observe(&value);",
        ),
        (
            "nested",
            "let result = api.nested<(api.Receipt?)>(INPUT); match consume result.value.1 { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "conditional_union",
            "let result = api.select_variant<(api.Receipt?)>(INPUT, choice); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "conditional_nullable",
            "let result = api.select_nullable<(api.Receipt?)>(INPUT, choice); match consume result { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "conditional_exits",
            "let result = api.select_exits<(api.Receipt?)>(INPUT, choice); match consume result { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "conditional_reversed",
            "let result = api.select_reversed<(api.Receipt?)>(INPUT, choice); match consume result { case ?value { api.observe(&value); } case none {} }",
        ),
        (
            "conditional_nested",
            "let result = api.select_nested<(api.Receipt?)>(INPUT, choice); match consume result.value { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "conditional_deferred",
            "let result = await api.select_deferred<(api.Receipt?)>(INPUT, choice); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "conditional_lambda",
            "let result = api.select_lambda<(api.Receipt?)>(INPUT, choice); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "suspending_conditional",
            "let result = { let mut state: api.State = { count = 0 }; yield await api.select_await<(api.Receipt?)>(INPUT, choice, &mut state); }; match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
        ),
        (
            "reference",
            "let other: u32 = 3; let result = api.with_reference<(api.Receipt?)>(INPUT, &other); api.observe(&result.0); api.reference(result.1);",
        ),
        (
            "mutation",
            "let result = { let mut state: api.State = { count = 0 }; yield api.mutate<(api.Receipt?)>(INPUT, &mut state); }; api.observe(&result.1);",
        ),
        (
            "branches",
            "let result = api.branches<(api.Receipt?)>(INPUT, choice); api.observe(&result.1);",
        ),
        (
            "composition",
            "let result = api.product<(u32, api.Receipt?)>(api.tuple<(api.Receipt?)>(INPUT)); api.observe(&result.value.1);",
        ),
    ];

    let mut failures = Vec::new();

    for (name, body) in cases {
        for valid in [true, false] {
            let body = body.replace("INPUT", if valid { "api.make()" } else { "none" });

            if let Some(failure) = check(&provider, imported, name, &body, valid) {
                failures.push(failure);
            }
        }
    }

    for (name, body, valid) in [
        ("nullable_payload_join", "let value: api.Marker? = if choice { yield api.make_marker(); } else { yield none; }; match consume value { case ?item { api.inspect_marker(&item); } case none {} }", true),
        ("nullable_payload_fabricated", "let value: api.Marker? = if choice { yield api.make_marker(); } else { yield api.Marker {}; }; match consume value { case ?item { api.inspect_marker(&item); } case none {} }", false),
        ("array_factory", "let [left, right] = api.factory(); api.observe(&left); api.observe(&right);", true),
        ("array_payload_factory", "let [left, right] = api.marker_factory(); api.inspect_marker(&left); api.inspect_marker(&right);", true),
        ("array_conditional_factory", "let [left, right] = api.conditional_factory(choice); match consume left { case ?item { api.inspect_marker(&item); } case none {} } match consume right { case ?item { api.inspect_marker(&item); } case none {} }", true),
        ("conditional_absent_input", "let result = api.select_variant<(api.Receipt?)>(api.absent(), choice); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }", true),
        ("conditional_optional_exits", "let result = api.select_optional<api.Receipt>(api.make(), choice); match consume result { case ?value { let wrapped: api.Receipt? = value; api.observe(&wrapped); } case none {} }", true),
        ("conditional_optional_join", "let result = api.select_optional_join<api.Receipt>(api.make(), choice); match consume result { case ?value { let wrapped: api.Receipt? = value; api.observe(&wrapped); } case none {} }", true),
        ("conditional_copy", "let original = api.stamp(); let result = api.select_variant<api.Stamp>(original, choice); match consume result { case .Value(value) { api.observe_stamp(&value); } case .Empty {} }", false),
        ("conditional_live_address", "let payload: u32 = 4; let result = api.select_variant<api.Address>(api.address(&payload), choice); match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", true),
        ("conditional_expired_address", "let result = { let payload: u32 = 4; yield api.select_variant<api.Address>(api.address(&payload), choice); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", false),
        ("conditional_independent_marker", "let result = { let payload: u32 = 4; yield api.select_variant<api.Address>(api.address(&payload), choice); }; match consume result { case .Value(value) { api.observe_marker(&value); } case .Empty {} }", true),
        ("conditional_authority_before", "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); api.inspect_permission(id);", false),
        ("conditional_authority_active", "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }", true),
        ("conditional_authority_inactive", "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }", false),
        ("independent_permission", "let permit = api.permission(); let other = api.permission(); let id = other.id; let result = api.select_variant<api.Permit>(permit, choice); api.inspect_permission(id);", true),
        ("conditional_consuming_receiver", "let result = api.select_variant<api.Resource>(api.resource(), choice); match consume result { case .Value(value) { api.inspect_resource(&value); } case .Empty {} }", true),
        ("conditional_consuming_receiver_forged", "let result = api.select_variant<api.Resource>(api.Resource { marker = api.Marker {} }, choice); match consume result { case .Value(value) { api.inspect_resource(&value); } case .Empty {} }", false),
        ("consuming_receiver_external_mutation", "let mut state: api.Dependency = { marker = api.Marker {} }; let owner = api.make_related(&state); api.cleanup_referent(&mut state); api.observe_related(&owner, &state);", false),
        ("consuming_receiver_independent_claim", "let mut state: api.Dependency = { marker = api.Marker {} }; let owner = api.make_related(&state); api.cleanup_referent(&mut state); api.observe_independent(&owner);", true),
        ("consumed_permission", "let permit = api.permission(); let id = permit.id; api.discard<api.Permit>(permit); api.inspect_permission(id);", false),
        ("deferred_consumed_permission", "let permit = api.permission(); let id = permit.id; await api.deferred_discard<api.Permit>(permit); api.inspect_permission(id);", false),
        ("copied_original_authority", "let original = api.stamp(); api.discard<api.Stamp>(original); api.observe_stamp(&original);", false),
        ("consumed_independent_permission", "let permit = api.permission(); let other = api.permission(); let id = other.id; api.discard<api.Permit>(permit); api.inspect_permission(id);", true),
        ("deferred_authority_active", "let permit = api.permission(); let id = permit.id; let result = await api.select_deferred<api.Permit>(permit, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }", true),
        ("deferred_authority_inactive", "let permit = api.permission(); let id = permit.id; let result = await api.select_deferred<api.Permit>(permit, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }", false),
        ("deferred_live_capture", "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); api.inspect_permission(id);", true),
        ("deferred_dropped_capture", "let id = { let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); yield id; }; api.inspect_permission(id);", false),
        ("deferred_dropped_nested_capture", "let id = { let permit = api.permission(); let id = permit.id; let pending = (0, api.deferred_discard<api.Permit>(permit)); yield id; }; api.inspect_permission(id);", false),
        ("conditional_future_active", "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }", true),
        ("conditional_future_inactive", "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }", false),
        ("conditional_future_dropped", "let id = { let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); yield id; }; api.inspect_permission(id);", false),
        ("extract_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract<api.Marker>(wrapped); api.inspect_marker(&result.1);", true),
        ("extract_partial_input", "let result = api.extract<api.Marker>(api.marker_box()); api.inspect_marker(&result.1);", false),
        ("deferred_partial_input_inactive", "let pending = api.deferred_extract<api.Marker>(api.marker_box());", true),
        ("deferred_partial_input_awaited", "let result = await api.deferred_extract<api.Marker>(api.marker_box()); api.inspect_marker(&result);", false),
        ("deferred_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract<api.Marker>(wrapped); api.inspect_marker(&result);", true),
        ("partial_optional_inactive", "let result = api.extract_optional<api.Marker>(api.marker_box(), false);", true),
        ("partial_optional_active", "let result = api.extract_optional<api.Marker>(api.marker_box(), true);", false),
        ("partial_optional_unknown", "let result = api.extract_optional<api.Marker>(api.marker_box(), choice);", false),
        ("partial_optional_inactive_path", "if choice { return; } let result = api.extract_optional<api.Marker>(api.marker_box(), choice);", true),
        ("partial_optional_active_path", "if !choice { return; } let result = api.extract_optional<api.Marker>(api.marker_box(), choice);", false),
        ("partial_optional_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract_optional<api.Marker>(wrapped, choice); match consume result { case ?value { api.inspect_marker(&value); } case none {} }", true),
        ("deferred_partial_optional_inactive", "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), false);", true),
        ("deferred_partial_optional_active", "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), true);", false),
        ("deferred_partial_optional_unknown", "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);", false),
        ("deferred_partial_optional_inactive_path", "if choice { return; } let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);", true),
        ("deferred_partial_optional_active_path", "if !choice { return; } let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);", false),
        ("deferred_partial_optional_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract_optional<api.Marker>(wrapped, choice); match consume result { case ?value { api.inspect_marker(&value); } case none {} }", true),
        ("partial_choice_inactive", "let result = api.extract_choice<api.Marker>(api.marker_box(), false);", true),
        ("partial_choice_active", "let result = api.extract_choice<api.Marker>(api.marker_box(), true);", false),
        ("partial_choice_unknown", "let result = api.extract_choice<api.Marker>(api.marker_box(), choice);", false),
        ("partial_choice_inactive_path", "if choice { return; } let result = api.extract_choice<api.Marker>(api.marker_box(), choice);", true),
        ("partial_choice_active_path", "if !choice { return; } let result = api.extract_choice<api.Marker>(api.marker_box(), choice);", false),
        ("partial_choice_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract_choice<api.Marker>(wrapped, choice); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }", true),
        ("deferred_partial_choice_inactive", "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), false);", true),
        ("deferred_partial_choice_active", "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), true);", false),
        ("deferred_partial_choice_unknown", "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);", false),
        ("deferred_partial_choice_inactive_path", "if choice { return; } let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);", true),
        ("deferred_partial_choice_active_path", "if !choice { return; } let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);", false),
        ("deferred_partial_choice_complete_child", "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract_choice<api.Marker>(wrapped, choice); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }", true),
        ("partial_future_unpolled", "let pending = api.deferred_extract_optional<api.Marker>(api.marker_box(), true);", true),
        ("consuming_constructor", "let permit = api.permission(); let id = permit.id; let sink = api.Sink(permit); api.inspect_permission(id);", false),
        ("suspending_live_address", "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", true),
        ("suspending_aliased_address", "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; let pending = api.select_await<api.Address>(api.address(&payload), choice, &mut state); yield await pending; }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", true),
        ("suspending_selected_owner", "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; let pending = api.select_await<api.Address>(api.address(&payload), choice, &mut state); let selected = if choice { yield pending; } else { yield pending; }; yield await selected; }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", true),
        ("suspending_replaced_borrow", "let outer: u32 = 4; let mut outer_state: api.State = { count = 0 }; let result = { let payload: u32 = 5; let mut state: api.State = { count = 0 }; let mut pending = api.select_await<(&u32)>(&outer, choice, &mut outer_state); pending = api.select_await<(&u32)>(&payload, choice, &mut state); yield await pending; }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }", false),
        ("suspending_borrow_active", "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield await api.select_await<(&u32)>(&payload, choice, &mut state); }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }", true),
        ("suspending_borrow_expired", "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<(&u32)>(&payload, choice, &mut state); }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }", false),
        ("suspending_expired_address", "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }", false),
        ("suspending_independent_marker", "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_marker(&value); } case .Empty {} }", true),
        ("unchanged_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.keep(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);", true),
        ("changed_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);", false),
        ("independent_dependency", "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_independent(&result.1);", true),

        (
            "projected_input",
            "let result = api.extract<(api.Receipt?)>(api.product<(api.Receipt?)>(api.make())); api.observe(&result.1);",
            true,
        ),
        (
            "two_inputs",
            "let result = api.pair<(api.Receipt?)>(api.make(), api.make()); api.observe(&result.0); api.observe(&result.1);",
            true,
        ),
        (
            "two_inputs_forged",
            "let result = api.pair<(api.Receipt?)>(api.make(), none); api.observe(&result.1);",
            false,
        ),
        (
            "copied_input",
            "let original = api.stamp(); let result = api.tuple<api.Stamp>(original); api.observe_stamp(&result.1);",
            false,
        ),
        (
            "copied_product",
            "let original = api.stamp(); let result = api.product<api.Stamp>(original); api.observe_stamp(&result.value);",
            false,
        ),
        (
            "live_address",
            "let payload: u32 = 4; let result = api.tuple<api.Address>(api.address(&payload)); api.observe_address(&result.1);",
            true,
        ),
        (
            "expired_address",
            "let result = { let payload: u32 = 4; yield api.tuple<api.Address>(api.address(&payload)); }; api.observe_address(&result.1);",
            false,
        ),
        (
            "independent_marker",
            "let result = { let payload: u32 = 4; yield api.tuple<api.Address>(api.address(&payload)); }; api.observe_marker(&result.1);",
            true,
        ),
        (
            "live_address_mutation",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_address(&result.1);",
            true,
        ),
        (
            "expired_address_mutation",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_address(&result.1);",
            false,
        ),
        (
            "marker_mutation",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield api.mutate<api.Address>(api.address(&payload), &mut state); }; api.observe_marker(&result.1);",
            true,
        ),
        (
            "expired_reference",
            "let result = { let payload: u32 = 4; yield api.with_reference<(api.Receipt?)>(api.make(), &payload); }; api.observe(&result.0); api.reference(result.1);",
            false,
        ),
    ] {
        if let Some(failure) = check(&provider, imported, name, body, valid) {
            failures.push(failure);
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn check(
    provider: &crate::Compilation,
    imported: bool,
    name: &str,
    body: &str,
    valid: bool,
) -> Option<String> {
    let effect = if body.contains("await ") || body.contains("Future<") {
        "async "
    } else {
        ""
    };

    let source =
        format!(r"
        module app;

        using api;

        {effect}func caller(pos choice: bool)
        {{
            {body}
        }}
    ");

    let consumer = if imported {
        execution_consumer(
            provider,
            &source
                .replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."),
        )
    } else {
        compilation_from_sources([API, &source])
    };

    let diagnostics = consumer.check_diagnostics();

    if diagnostics.has_errors() == valid {
        return Some(format!(
            "{name}, imported={imported}, valid={valid}: {diagnostics:#?}"
        ));
    }

    if !valid {
        if !diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                    | bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency
                    | bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
            )
        }) {
            return Some(format!("unexpected rejection for {name}: {diagnostics:#?}"));
        }
    }

    None
}

#[test]
fn source_owned_transfers_preserve_complete_carriers_and_dependencies() {
    check_owned_transfers(false);
}

#[test]
fn imported_owned_transfers_preserve_complete_carriers_and_dependencies() {
    check_owned_transfers(true);
}
