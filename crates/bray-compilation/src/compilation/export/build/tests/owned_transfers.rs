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
    trusted func permission_box() -> Box<Permit>
        ensures(trusted permitted(id = result.value.id))
    {
        return
        {
            value = Permit
            {
                id = 1
            }
        };
    }

    func select_inputs<T>(pos left: T, pos right: T, pos first: bool) -> (u32, T)
    {
        if first
        {
            return (0, left);
        }

        return (0, right);
    }

    func select_join<T>(pos left: T, pos right: T, pos first: bool) -> (u32, T)
    {
        return (0, if first
        {
            yield left;
        }
        else
        {
            yield right;
        });
    }

    func select_forward<T>(pos left: T, pos right: T, pos first: bool) -> (u32, T)
    {
        return select_join<T>(left, right, first);
    }

    func swap<T>(pos left: T, pos right: T, pos first: bool) -> (T, T)
    {
        if first
        {
            return (left, right);
        }

        return (right, left);
    }

    func swap_join<T>(pos left: T, pos right: T, pos first: bool) -> (T, T)
    {
        return if first
        {
            yield (left, right);
        }
        else
        {
            yield (right, left);
        };
    }

    func mixed<T>(pos left: Box<T>, pos right: T, pos first: bool) -> Box<T>
    {
        if first
        {
            return left;
        }

        return
        {
            value = right
        };
    }

    func mixed_join<T>(pos left: Box<T>, pos right: T, pos first: bool) -> Box<T>
    {
        return if first
        {
            yield left;
        }
        else
        {
            let alternative: Box<T> =
            {
                value = right
            };

            yield alternative;
        };
    }

    func select_direct<T>(pos left: T, pos right: T, pos first: bool) -> T
    {
        if first
        {
            return left;
        }

        return right;
    }

    func select_direct_join<T>(pos left: T, pos right: T, pos first: bool) -> T
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

    func select_loop<T>(pos left: T, pos right: T, pos first: bool) -> T
    {
        let mut selected = left;

        while first
        {
            selected = right;
            break;
        }

        return selected;
    }

    func select_nullable_inputs_exits<T>(pos left: T, pos right: T, pos first: bool, pos take: bool) -> T?
    {
        if !take
        {
            return none;
        }

        if first
        {
            return left;
        }

        return right;
    }

    func select_nullable_inputs_join<T>(pos left: T, pos right: T, pos first: bool, pos take: bool) -> T?
    {
        return if !take
        {
            yield none;
        }
        else
        {
            let value = if first
            {
                yield left;
            }
            else
            {
                yield right;
            };

            yield value;
        };
    }

    func select_variant_inputs_exits<T>(
        pos left: T,
        pos right: T,
        pos first: bool,
        pos take: bool,
    ) -> Choice<T>
    {
        if !take
        {
            return .Empty;
        }

        if first
        {
            return .Value(left);
        }

        return .Value(right);
    }

    func select_variant_inputs_join<T>(
        pos left: T,
        pos right: T,
        pos first: bool,
        pos take: bool,
    ) -> Choice<T>
    {
        return if !take
        {
            yield .Empty;
        }
        else
        {
            let value = if first
            {
                yield left;
            }
            else
            {
                yield right;
            };

            yield .Value(value);
        };
    }

    func stop() -> never
    {
        loop
        {
        }
    }

    func only_return<T>(pos value: T, pos selected: bool) -> T
    {
        if selected
        {
            return value;
        }

        stop();
    }

    async func only_deferred<T>(pos value: T, pos selected: bool) -> T
    {
        await pause();

        if selected
        {
            return value;
        }

        stop();
    }

    func choose_or_stop<T>(
        pos left: T,
        pos right: T,
        pos first: bool,
        pos run: bool,
    ) -> T
    {
        if !run
        {
            stop();
        }

        return if first
        {
            yield left;
        }
        else
        {
            yield right;
        };
    }

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
    let mut checked = 0;

    for (name, body) in cases {
        for valid in [true, false] {
            let body = body.replace("INPUT", if valid { "api.make()" } else { "none" });

            checked += 1;

            if let Some(failure) = check(&provider, imported, name, &body, valid) {
                failures.push(failure);
            }
        }
    }

    for (name, body, valid) in [
        (
            "nullable_payload_join",
            "let value: api.Marker? = if choice { yield api.make_marker(); } else { yield none; }; match consume value { case ?item { api.inspect_marker(&item); } case none {} }",
            true,
        ),
        (
            "nullable_payload_fabricated",
            "let value: api.Marker? = if choice { yield api.make_marker(); } else { yield api.Marker {}; }; match consume value { case ?item { api.inspect_marker(&item); } case none {} }",
            false,
        ),
        (
            "array_factory",
            "let [left, right] = api.factory(); api.observe(&left); api.observe(&right);",
            true,
        ),
        (
            "array_payload_factory",
            "let [left, right] = api.marker_factory(); api.inspect_marker(&left); api.inspect_marker(&right);",
            true,
        ),
        (
            "array_conditional_factory",
            "let [left, right] = api.conditional_factory(choice); match consume left { case ?item { api.inspect_marker(&item); } case none {} } match consume right { case ?item { api.inspect_marker(&item); } case none {} }",
            true,
        ),
        (
            "conditional_absent_input",
            "let result = api.select_variant<(api.Receipt?)>(api.absent(), choice); match consume result { case .Value(value) { api.observe(&value); } case .Empty {} }",
            true,
        ),
        (
            "conditional_optional_exits",
            "let result = api.select_optional<api.Receipt>(api.make(), choice); match consume result { case ?value { let wrapped: api.Receipt? = value; api.observe(&wrapped); } case none {} }",
            true,
        ),
        (
            "conditional_optional_join",
            "let result = api.select_optional_join<api.Receipt>(api.make(), choice); match consume result { case ?value { let wrapped: api.Receipt? = value; api.observe(&wrapped); } case none {} }",
            true,
        ),
        (
            "conditional_copy",
            "let original = api.stamp(); let result = api.select_variant<api.Stamp>(original, choice); match consume result { case .Value(value) { api.observe_stamp(&value); } case .Empty {} }",
            false,
        ),
        (
            "conditional_live_address",
            "let payload: u32 = 4; let result = api.select_variant<api.Address>(api.address(&payload), choice); match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            true,
        ),
        (
            "conditional_expired_address",
            "let result = { let payload: u32 = 4; yield api.select_variant<api.Address>(api.address(&payload), choice); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            false,
        ),
        (
            "conditional_independent_marker",
            "let result = { let payload: u32 = 4; yield api.select_variant<api.Address>(api.address(&payload), choice); }; match consume result { case .Value(value) { api.observe_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "conditional_authority_before",
            "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); api.inspect_permission(id);",
            false,
        ),
        (
            "conditional_authority_active",
            "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }",
            true,
        ),
        (
            "conditional_authority_inactive",
            "let permit = api.permission(); let id = permit.id; let result = api.select_variant<api.Permit>(permit, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }",
            false,
        ),
        (
            "independent_permission",
            "let permit = api.permission(); let other = api.permission(); let id = other.id; let result = api.select_variant<api.Permit>(permit, choice); api.inspect_permission(id);",
            true,
        ),
        (
            "conditional_consuming_receiver",
            "let result = api.select_variant<api.Resource>(api.resource(), choice); match consume result { case .Value(value) { api.inspect_resource(&value); } case .Empty {} }",
            true,
        ),
        (
            "conditional_consuming_receiver_forged",
            "let result = api.select_variant<api.Resource>(api.Resource { marker = api.Marker {} }, choice); match consume result { case .Value(value) { api.inspect_resource(&value); } case .Empty {} }",
            false,
        ),
        (
            "consuming_receiver_external_mutation",
            "let mut state: api.Dependency = { marker = api.Marker {} }; let owner = api.make_related(&state); api.cleanup_referent(&mut state); api.observe_related(&owner, &state);",
            false,
        ),
        (
            "consuming_receiver_independent_claim",
            "let mut state: api.Dependency = { marker = api.Marker {} }; let owner = api.make_related(&state); api.cleanup_referent(&mut state); api.observe_independent(&owner);",
            true,
        ),
        (
            "consumed_permission",
            "let permit = api.permission(); let id = permit.id; api.discard<api.Permit>(permit); api.inspect_permission(id);",
            false,
        ),
        (
            "deferred_consumed_permission",
            "let permit = api.permission(); let id = permit.id; await api.deferred_discard<api.Permit>(permit); api.inspect_permission(id);",
            false,
        ),
        (
            "copied_original_authority",
            "let original = api.stamp(); api.discard<api.Stamp>(original); api.observe_stamp(&original);",
            false,
        ),
        (
            "consumed_independent_permission",
            "let permit = api.permission(); let other = api.permission(); let id = other.id; api.discard<api.Permit>(permit); api.inspect_permission(id);",
            true,
        ),
        (
            "deferred_authority_active",
            "let permit = api.permission(); let id = permit.id; let result = await api.select_deferred<api.Permit>(permit, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }",
            true,
        ),
        (
            "deferred_authority_inactive",
            "let permit = api.permission(); let id = permit.id; let result = await api.select_deferred<api.Permit>(permit, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }",
            false,
        ),
        (
            "deferred_live_capture",
            "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); api.inspect_permission(id);",
            true,
        ),
        (
            "deferred_dropped_capture",
            "let id = { let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); yield id; }; api.inspect_permission(id);",
            false,
        ),
        (
            "deferred_dropped_nested_capture",
            "let id = { let permit = api.permission(); let id = permit.id; let pending = (0, api.deferred_discard<api.Permit>(permit)); yield id; }; api.inspect_permission(id);",
            false,
        ),
        (
            "conditional_future_active",
            "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); match consume result { case .Value(value) { api.inspect_permission(id); } case .Empty {} }",
            true,
        ),
        (
            "conditional_future_inactive",
            "let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); match consume result { case .Value(value) {} case .Empty { api.inspect_permission(id); } }",
            false,
        ),
        (
            "conditional_future_dropped",
            "let id = { let permit = api.permission(); let id = permit.id; let pending = api.deferred_discard<api.Permit>(permit); let result = await api.select_deferred<Future<unit>>(pending, choice); yield id; }; api.inspect_permission(id);",
            false,
        ),
        (
            "extract_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract<api.Marker>(wrapped); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "extract_partial_input",
            "let result = api.extract<api.Marker>(api.marker_box()); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "deferred_partial_input_inactive",
            "let pending = api.deferred_extract<api.Marker>(api.marker_box());",
            true,
        ),
        (
            "deferred_partial_input_awaited",
            "let result = await api.deferred_extract<api.Marker>(api.marker_box()); api.inspect_marker(&result);",
            false,
        ),
        (
            "deferred_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract<api.Marker>(wrapped); api.inspect_marker(&result);",
            true,
        ),
        (
            "partial_optional_inactive",
            "let result = api.extract_optional<api.Marker>(api.marker_box(), false);",
            true,
        ),
        (
            "partial_optional_active",
            "let result = api.extract_optional<api.Marker>(api.marker_box(), true);",
            false,
        ),
        (
            "partial_optional_unknown",
            "let result = api.extract_optional<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "partial_optional_inactive_path",
            "if choice { return; } let result = api.extract_optional<api.Marker>(api.marker_box(), choice);",
            true,
        ),
        (
            "partial_optional_active_path",
            "if !choice { return; } let result = api.extract_optional<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "partial_optional_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract_optional<api.Marker>(wrapped, choice); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "deferred_partial_optional_inactive",
            "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), false);",
            true,
        ),
        (
            "deferred_partial_optional_active",
            "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), true);",
            false,
        ),
        (
            "deferred_partial_optional_unknown",
            "let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "deferred_partial_optional_inactive_path",
            "if choice { return; } let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);",
            true,
        ),
        (
            "deferred_partial_optional_active_path",
            "if !choice { return; } let result = await api.deferred_extract_optional<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "deferred_partial_optional_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract_optional<api.Marker>(wrapped, choice); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "partial_choice_inactive",
            "let result = api.extract_choice<api.Marker>(api.marker_box(), false);",
            true,
        ),
        (
            "partial_choice_active",
            "let result = api.extract_choice<api.Marker>(api.marker_box(), true);",
            false,
        ),
        (
            "partial_choice_unknown",
            "let result = api.extract_choice<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "partial_choice_inactive_path",
            "if choice { return; } let result = api.extract_choice<api.Marker>(api.marker_box(), choice);",
            true,
        ),
        (
            "partial_choice_active_path",
            "if !choice { return; } let result = api.extract_choice<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "partial_choice_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = api.extract_choice<api.Marker>(wrapped, choice); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "deferred_partial_choice_inactive",
            "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), false);",
            true,
        ),
        (
            "deferred_partial_choice_active",
            "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), true);",
            false,
        ),
        (
            "deferred_partial_choice_unknown",
            "let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "deferred_partial_choice_inactive_path",
            "if choice { return; } let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);",
            true,
        ),
        (
            "deferred_partial_choice_active_path",
            "if !choice { return; } let result = await api.deferred_extract_choice<api.Marker>(api.marker_box(), choice);",
            false,
        ),
        (
            "deferred_partial_choice_complete_child",
            "let wrapped: api.Box<api.Marker> = { value = api.make_marker() }; let result = await api.deferred_extract_choice<api.Marker>(wrapped, choice); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "partial_future_unpolled",
            "let pending = api.deferred_extract_optional<api.Marker>(api.marker_box(), true);",
            true,
        ),
        (
            "consuming_constructor",
            "let permit = api.permission(); let id = permit.id; let sink = api.Sink(permit); api.inspect_permission(id);",
            false,
        ),
        (
            "suspending_live_address",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            true,
        ),
        (
            "suspending_aliased_address",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; let pending = api.select_await<api.Address>(api.address(&payload), choice, &mut state); yield await pending; }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            true,
        ),
        (
            "suspending_selected_owner",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; let pending = api.select_await<api.Address>(api.address(&payload), choice, &mut state); let selected = if choice { yield pending; } else { yield pending; }; yield await selected; }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            true,
        ),
        (
            "suspending_replaced_borrow",
            "let outer: u32 = 4; let mut outer_state: api.State = { count = 0 }; let result = { let payload: u32 = 5; let mut state: api.State = { count = 0 }; let mut pending = api.select_await<(&u32)>(&outer, choice, &mut outer_state); pending = api.select_await<(&u32)>(&payload, choice, &mut state); yield await pending; }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }",
            false,
        ),
        (
            "suspending_borrow_active",
            "let payload: u32 = 4; let result = { let mut state: api.State = { count = 0 }; yield await api.select_await<(&u32)>(&payload, choice, &mut state); }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }",
            true,
        ),
        (
            "suspending_borrow_expired",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<(&u32)>(&payload, choice, &mut state); }; match consume result { case .Value(value) { api.observe_borrow(value); } case .Empty {} }",
            false,
        ),
        (
            "suspending_expired_address",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_address(&value); } case .Empty {} }",
            false,
        ),
        (
            "suspending_independent_marker",
            "let result = { let payload: u32 = 4; let mut state: api.State = { count = 0 }; yield await api.select_await<api.Address>(api.address(&payload), choice, &mut state); }; match consume result { case .Value(value) { api.observe_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "unchanged_dependency",
            "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.keep(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);",
            true,
        ),
        (
            "changed_dependency",
            "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_related(&result.1, &state);",
            false,
        ),
        (
            "independent_dependency",
            "let mut state: api.Dependency = { marker = api.Marker {} }; let result = api.replace(api.make_related(&state), &mut state); api.observe_independent(&result.1);",
            true,
        ),
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
        (
            "select_exits_both",
            "let result = api.select_inputs<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_exits_left",
            "let result = api.select_inputs<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_exits_right",
            "let result = api.select_inputs<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_join_both",
            "let result = api.select_join<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_join_left",
            "let result = api.select_join<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_join_right",
            "let result = api.select_join<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_forward_both",
            "let result = api.select_forward<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_forward_left",
            "let result = api.select_forward<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_forward_right",
            "let result = api.select_forward<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "swap_both",
            "let result = api.swap<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "swap_left",
            "let result = api.swap<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "swap_right",
            "let result = api.swap<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "swap_join_both",
            "let result = api.swap_join<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "swap_join_left",
            "let result = api.swap_join<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "swap_join_right",
            "let result = api.swap_join<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.0); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "mixed_both",
            "let result = api.mixed<api.Marker>(api.marker_box(), api.make_marker(), choice); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_left",
            "let result = api.mixed<api.Marker>(api.marker_box(), api.Marker {}, choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_right",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed<api.Marker>(left, api.make_marker(), choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_join_both",
            "let result = api.mixed_join<api.Marker>(api.marker_box(), api.make_marker(), choice); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_join_left",
            "let result = api.mixed_join<api.Marker>(api.marker_box(), api.Marker {}, choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_join_right",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed_join<api.Marker>(left, api.make_marker(), choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_permission",
            "let result = api.mixed<api.Permit>(api.permission_box(), api.permission(), choice); api.inspect_permission(result.value.id);",
            true,
        ),
        (
            "mixed_permission_fabricated",
            "let result = api.mixed<api.Permit>(api.permission_box(), api.Permit { id = 1 }, choice); api.inspect_permission(result.value.id);",
            false,
        ),
        (
            "mixed_expired_address",
            "let outer: u32 = 4; let result = { let inner: u32 = 5; yield api.mixed<api.Address>(api.product<api.Address>(api.address(&outer)), api.address(&inner), choice); }; api.observe_address(&result.value);",
            false,
        ),
        (
            "mixed_independent_address",
            "let outer: u32 = 4; let result = { let inner: u32 = 5; yield api.mixed<api.Address>(api.product<api.Address>(api.address(&outer)), api.address(&inner), choice); }; api.observe_marker(&result.value);",
            true,
        ),
        (
            "mixed_join_permission",
            "let result = api.mixed_join<api.Permit>(api.permission_box(), api.permission(), choice); api.inspect_permission(result.value.id);",
            true,
        ),
        (
            "mixed_join_permission_fabricated",
            "let result = api.mixed_join<api.Permit>(api.permission_box(), api.Permit { id = 1 }, choice); api.inspect_permission(result.value.id);",
            false,
        ),
        (
            "mixed_join_expired_address",
            "let outer: u32 = 4; let result = { let inner: u32 = 5; yield api.mixed_join<api.Address>(api.product<api.Address>(api.address(&outer)), api.address(&inner), choice); }; api.observe_address(&result.value);",
            false,
        ),
        (
            "mixed_join_independent_address",
            "let outer: u32 = 4; let result = { let inner: u32 = 5; yield api.mixed_join<api.Address>(api.product<api.Address>(api.address(&outer)), api.address(&inner), choice); }; api.observe_marker(&result.value);",
            true,
        ),
        (
            "select_inputs_fabricated",
            "let result = api.select_inputs<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_join_fabricated",
            "let result = api.select_join<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_forward_fabricated",
            "let result = api.select_forward<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "swap_fabricated",
            "let result = api.swap<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result.0);",
            false,
        ),
        (
            "swap_join_fabricated",
            "let result = api.swap_join<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result.0);",
            false,
        ),
        (
            "mixed_fabricated",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed<api.Marker>(left, api.Marker {}, choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_join_fabricated",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed_join<api.Marker>(left, api.Marker {}, choice); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "select_direct_both",
            "let result = api.select_direct<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_left",
            "let result = api.select_direct<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_right",
            "let result = api.select_direct<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_neither",
            "let result = api.select_direct<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_join_both",
            "let result = api.select_direct_join<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_join_left",
            "let result = api.select_direct_join<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_join_right",
            "let result = api.select_direct_join<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_join_neither",
            "let result = api.select_direct_join<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_loop_both",
            "let result = api.select_loop<api.Marker>(api.make_marker(), api.make_marker(), choice); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_loop_left",
            "let result = api.select_loop<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_loop_right",
            "let result = api.select_loop<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_loop_neither",
            "let result = api.select_loop<api.Marker>(api.Marker {}, api.Marker {}, choice); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_nullable_inputs_exits_both",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.make_marker(), api.make_marker(), choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "select_nullable_inputs_exits_left",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.make_marker(), api.Marker {}, choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_exits_right",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.Marker {}, api.make_marker(), choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_exits_neither",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.Marker {}, api.Marker {}, choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_join_both",
            "let result = api.select_nullable_inputs_join<api.Marker>(api.make_marker(), api.make_marker(), choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "select_nullable_inputs_join_left",
            "let result = api.select_nullable_inputs_join<api.Marker>(api.make_marker(), api.Marker {}, choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_join_right",
            "let result = api.select_nullable_inputs_join<api.Marker>(api.Marker {}, api.make_marker(), choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_join_neither",
            "let result = api.select_nullable_inputs_join<api.Marker>(api.Marker {}, api.Marker {}, choice, active); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_variant_inputs_exits_both",
            "let result = api.select_variant_inputs_exits<api.Marker>(api.make_marker(), api.make_marker(), choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "select_variant_inputs_exits_left",
            "let result = api.select_variant_inputs_exits<api.Marker>(api.make_marker(), api.Marker {}, choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_exits_right",
            "let result = api.select_variant_inputs_exits<api.Marker>(api.Marker {}, api.make_marker(), choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_exits_neither",
            "let result = api.select_variant_inputs_exits<api.Marker>(api.Marker {}, api.Marker {}, choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_join_both",
            "let result = api.select_variant_inputs_join<api.Marker>(api.make_marker(), api.make_marker(), choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "select_variant_inputs_join_left",
            "let result = api.select_variant_inputs_join<api.Marker>(api.make_marker(), api.Marker {}, choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_join_right",
            "let result = api.select_variant_inputs_join<api.Marker>(api.Marker {}, api.make_marker(), choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_join_neither",
            "let result = api.select_variant_inputs_join<api.Marker>(api.Marker {}, api.Marker {}, choice, active); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "alternative_disposed",
            "let result = api.select_direct<api.Permit>(api.permission(), api.permission(), choice); api.discard(result); api.inspect_permission(1);",
            false,
        ),
        (
            "alternative_old_scalar",
            "let result = api.select_direct<api.Permit>(api.permission(), api.permission(), choice); let id = result.id; api.discard(result); api.inspect_permission(id);",
            false,
        ),
        (
            "alternative_independent",
            "let other = api.permission(); let result = api.select_direct<api.Permit>(api.permission(), api.permission(), choice); api.discard(result); api.inspect_permission(other.id);",
            true,
        ),
        (
            "alternative_array",
            "let result = [api.select_direct<api.Marker>(api.make_marker(), api.make_marker(), choice)]; let [value] = result; api.inspect_marker(&value);",
            true,
        ),
        (
            "alternative_product",
            "let result = api.product<api.Marker>(api.select_direct<api.Marker>(api.make_marker(), api.make_marker(), choice)); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "alternative_nullable",
            "let result = api.select_nullable<api.Marker>(api.select_direct<api.Marker>(api.make_marker(), api.make_marker(), choice), choice); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "alternative_union",
            "let result = api.select_variant<api.Marker>(api.select_direct<api.Marker>(api.make_marker(), api.make_marker(), choice), choice); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "select_inputs_known_True_True",
            "let result = api.select_inputs<api.Marker>(api.make_marker(), api.Marker {}, true); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_inputs_known_True_False",
            "let result = api.select_inputs<api.Marker>(api.Marker {}, api.make_marker(), true); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_inputs_known_False_True",
            "let result = api.select_inputs<api.Marker>(api.Marker {}, api.make_marker(), false); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_inputs_known_False_False",
            "let result = api.select_inputs<api.Marker>(api.make_marker(), api.Marker {}, false); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_join_known_True_True",
            "let result = api.select_join<api.Marker>(api.make_marker(), api.Marker {}, true); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_join_known_True_False",
            "let result = api.select_join<api.Marker>(api.Marker {}, api.make_marker(), true); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_join_known_False_True",
            "let result = api.select_join<api.Marker>(api.Marker {}, api.make_marker(), false); api.inspect_marker(&result.1);",
            true,
        ),
        (
            "select_join_known_False_False",
            "let result = api.select_join<api.Marker>(api.make_marker(), api.Marker {}, false); api.inspect_marker(&result.1);",
            false,
        ),
        (
            "select_direct_known_True_True",
            "let result = api.select_direct<api.Marker>(api.make_marker(), api.Marker {}, true); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_known_True_False",
            "let result = api.select_direct<api.Marker>(api.Marker {}, api.make_marker(), true); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_known_False_True",
            "let result = api.select_direct<api.Marker>(api.Marker {}, api.make_marker(), false); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_known_False_False",
            "let result = api.select_direct<api.Marker>(api.make_marker(), api.Marker {}, false); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_join_known_True_True",
            "let result = api.select_direct_join<api.Marker>(api.make_marker(), api.Marker {}, true); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_join_known_True_False",
            "let result = api.select_direct_join<api.Marker>(api.Marker {}, api.make_marker(), true); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_direct_join_known_False_True",
            "let result = api.select_direct_join<api.Marker>(api.Marker {}, api.make_marker(), false); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_direct_join_known_False_False",
            "let result = api.select_direct_join<api.Marker>(api.make_marker(), api.Marker {}, false); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_nullable_inputs_exits_known_True_True",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.make_marker(), api.Marker {}, true, true); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "select_nullable_inputs_exits_known_True_False",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.Marker {}, api.make_marker(), true, true); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_nullable_inputs_exits_known_False_True",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.Marker {}, api.make_marker(), false, true); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            true,
        ),
        (
            "select_nullable_inputs_exits_known_False_False",
            "let result = api.select_nullable_inputs_exits<api.Marker>(api.make_marker(), api.Marker {}, false, true); match consume result { case ?value { api.inspect_marker(&value); } case none {} }",
            false,
        ),
        (
            "select_variant_inputs_join_known_True_True",
            "let result = api.select_variant_inputs_join<api.Marker>(api.make_marker(), api.Marker {}, true, true); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "select_variant_inputs_join_known_True_False",
            "let result = api.select_variant_inputs_join<api.Marker>(api.Marker {}, api.make_marker(), true, true); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_variant_inputs_join_known_False_True",
            "let result = api.select_variant_inputs_join<api.Marker>(api.Marker {}, api.make_marker(), false, true); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            true,
        ),
        (
            "select_variant_inputs_join_known_False_False",
            "let result = api.select_variant_inputs_join<api.Marker>(api.make_marker(), api.Marker {}, false, true); match consume result { case .Value(value) { api.inspect_marker(&value); } case .Empty {} }",
            false,
        ),
        (
            "select_context_left",
            "if choice { let result = api.select_join<api.Marker>(api.make_marker(), api.Marker {}, choice); api.inspect_marker(&result.1); }",
            true,
        ),
        (
            "select_context_right",
            "if !choice { let result = api.select_inputs<api.Marker>(api.Marker {}, api.make_marker(), choice); api.inspect_marker(&result.1); }",
            true,
        ),
        (
            "select_loop_known_True_True",
            "let result = api.select_loop<api.Marker>(api.Marker {}, api.make_marker(), true); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_loop_known_True_False",
            "let result = api.select_loop<api.Marker>(api.make_marker(), api.Marker {}, true); api.inspect_marker(&result);",
            false,
        ),
        (
            "select_loop_known_False_True",
            "let result = api.select_loop<api.Marker>(api.make_marker(), api.Marker {}, false); api.inspect_marker(&result);",
            true,
        ),
        (
            "select_loop_known_False_False",
            "let result = api.select_loop<api.Marker>(api.Marker {}, api.make_marker(), false); api.inspect_marker(&result);",
            false,
        ),
        (
            "mixed_known_True_True",
            "let result = api.mixed<api.Marker>(api.marker_box(), api.Marker {}, true); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_known_True_False",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed<api.Marker>(left, api.make_marker(), true); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_known_False_True",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed<api.Marker>(left, api.make_marker(), false); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_known_False_False",
            "let result = api.mixed<api.Marker>(api.marker_box(), api.Marker {}, false); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_join_known_True_True",
            "let result = api.mixed_join<api.Marker>(api.marker_box(), api.Marker {}, true); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_join_known_True_False",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed_join<api.Marker>(left, api.make_marker(), true); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "mixed_join_known_False_True",
            "let left: api.Box<api.Marker> = { value = api.Marker {} }; let result = api.mixed_join<api.Marker>(left, api.make_marker(), false); api.inspect_marker(&result.value);",
            true,
        ),
        (
            "mixed_join_known_False_False",
            "let result = api.mixed_join<api.Marker>(api.marker_box(), api.Marker {}, false); api.inspect_marker(&result.value);",
            false,
        ),
        (
            "only_return_True",
            "let value = api.only_return<api.Marker>(api.make_marker(), true); api.inspect_marker(&value);",
            true,
        ),
        (
            "only_return_False",
            "let value = api.only_return<api.Marker>(api.make_marker(), false); api.inspect_marker(&value);",
            true,
        ),
        (
            "only_deferred_True",
            "let value = await api.only_deferred<api.Marker>(api.make_marker(), true); api.inspect_marker(&value);",
            true,
        ),
        (
            "only_deferred_False",
            "let value = await api.only_deferred<api.Marker>(api.make_marker(), false); api.inspect_marker(&value);",
            true,
        ),
        (
            "impossible_choice",
            "let value = api.choose_or_stop<api.Marker>(api.Marker {}, api.Marker {}, choice, false); api.inspect_marker(&value);",
            true,
        ),
        (
            "possible_choice",
            "let value = api.choose_or_stop<api.Marker>(api.make_marker(), api.make_marker(), choice, true); api.inspect_marker(&value);",
            true,
        ),
        (
            "possible_choice_fabricated",
            "let value = api.choose_or_stop<api.Marker>(api.make_marker(), api.Marker {}, choice, true); api.inspect_marker(&value);",
            false,
        ),
        (
            "inactive_never_returning_future",
            "let pending: Future<api.Marker> = api.only_deferred<api.Marker>(api.make_marker(), false);",
            true,
        ),
        (
            "scalar_impossible_completion",
            "let value = api.only_return<bool>(true, false); if value {}",
            true,
        ),
        (
            "captured_true_selector",
            "let mut selected = true; let pending = api.only_deferred<api.Marker>(api.make_marker(), selected); selected = false; let value = await pending; api.inspect_marker(&value);",
            true,
        ),
        (
            "captured_false_selector",
            "let mut selected = false; let pending = api.only_deferred<api.Marker>(api.make_marker(), selected); selected = true; let value = await pending; api.inspect_marker(&value);",
            true,
        ),
    ] {
        checked += 1;

        if let Some(failure) = check(&provider, imported, name, body, valid) {
            failures.push(failure);
        }
    }

    assert!(failures.is_empty(), "owned cases completed: imported={imported}, count={checked}\n{}", failures.join("\n"));
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

    let source = format!(
        r"
        module app;

        using api;

        {effect}func caller(pos choice: bool, pos active: bool)
        {{
            {body}
        }}
    "
    );

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
            "{name}, imported={imported}, valid={valid}: {}", super::fixtures::diagnostic_messages(diagnostics)
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
            return Some(format!("unexpected rejection for {name}: {}", super::fixtures::diagnostic_messages(diagnostics)));
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

#[test]
fn published_nullable_payloads_survive_consumed_results_and_opaque_calls() {
    let created = r#"
                let mut owner = match consume api.make(flag)
                {
                    case Ok(value) { yield value; }
                    case Error(status) { return status; }
                };
    "#;
    let branch = r#"
                if api.opaque() != 0
                {
                    api.take(owner);
                    return 1;
                }

                api.take(owner);
    "#;
    for guard in ["!(result matches Ok(_))", "result matches Error(_)"] {
    for (published, setup, body, valid) in [
        (false, created, "_ = api.opaque();\n                api.take(owner);", true),
        (true, created, "api.take(owner);", true),
        (true, created, "_ = api.opaque();\n                api.take(owner);", true),
        (true, created, branch, true),
        (true, "let owner: api.Owner? = api.Owner\n                {\n                    value = 0\n                };", "api.take(owner);", false),
        (true, created, "owner = api.Owner\n                {\n                    value = 1\n                };\n                api.take(owner);", false),
        (true, "let owner: api.Owner? = none;", "api.take(owner);", true),
    ] {
        let publication = if published { "publish(&owner);" } else { "" };
        let declarations = format!(r#"
            trusted module api;

            struct Owner
            {{
                value: u32;
            }}

            trusted predicate owned(owner: &Owner);

            predicate optional_owned(owner: &(Owner?)) = owner matches none || owned(owner = present(owner));

            const func present(pos owner: &(Owner?)) -> &Owner
                requires(owner matches ?_)
                executes(pure, total)
            {{
                return match owner
                {{
                    case ?value {{ yield &value; }}
                    case none {{ panic("missing owner"); }}
                }};
            }}

            const func created(pos owner: &Result<(Owner?), u32>) -> &(Owner?)
                requires(owner matches Ok(_))
                executes(pure, total)
            {{
                return match owner
                {{
                    case Ok(value) {{ yield &value; }}
                    case Error(_) {{ panic("failed creation"); }}
                }};
            }}

            trusted func publish(pos owner: &Owner)
                ensures(trusted owned(owner = owner))
                executes(pure, total)
            {{
            }}

            trusted func make(pos flag: bool) -> Result<(Owner?), u32>
                ensures({guard} || trusted optional_owned(owner = created(&result)))
            {{
                if flag
                {{
                    let owner: Owner =
                    {{
                        value = 0
                    }};

                    {publication}
                    return Ok(owner);
                }}

                return Error(1);
            }}

            func take(pos owner: Owner?)
                requires(trusted optional_owned(owner = &owner))
            {{
            }}

            extern func opaque() -> u32;
        "#);
        let provider = compilation(&declarations);
        assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());
        let source = format!(r#"
            module app;

            using api;

            func caller(pos flag: bool) -> u32
            {{
                {setup}
                {body}
                return 0;
            }}
        "#);
        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                compilation_from_sources([declarations.as_str(), &source])
            };
            let diagnostics = application.check_diagnostics();
            assert_eq!(!diagnostics.has_errors(), valid, "guard={guard}, published={published}, imported={imported}: {source}\n{diagnostics:#?}\n{:#?}",
                crate::compilation::result_dependencies::source_function_dependencies(&provider, "make"));
            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);
            }
        }
    }
    }
}

#[test]
fn produced_owned_results_survive_enclosing_scope_cleanup() {
    for complex in [false, true] {
    for (region, transfer) in [("", "yield"), ("loop", "break"), ("return", "return")] {
    if complex && transfer == "return" { continue; }

    for (guard, cleanup, created, valid) in [
        ("", "", "trusted api.create()", true),
        ("let guard = api.Guard\n{\n};", "", "trusted api.create()", true),
        ("let guard = api.Guard\n{\n};", "release();", "trusted api.create()", true),
        ("let guard = api.Guard\n{\n};", "release();", "api.Worker\n{\n    marker = api.Marker {}\n}", false),
    ] {
        let extra_variants = if complex { "Created(pos owner: Worker);\nBacking;" } else { "" };
        let declarations = format!(r#"
            trusted module api;

            struct Marker
            {{
            }}

            struct Worker
            {{
                marker: Marker;
            }}

            trusted predicate live(owner: &Marker);

            trusted func create() -> Worker
                ensures(trusted live(owner = &result.marker))
            {{
                return
                {{
                    marker = Marker {{}}
                }};
            }}

            extern func release();

            struct Guard
            {{
                destruct()
                {{
                    {cleanup}
                }}
            }}

            union Choice
            {{
                Existing(pos owner: Worker);
                Empty;
                {extra_variants}
            }}

            func inspect(pos owner: &Worker)
                requires(trusted live(owner = &owner.marker))
            {{
            }}
        "#);
        let provider = compilation(&declarations);
        assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

        let source = format!(r#"
            module app;

            using api;

            func caller(pos branch: bool, pos other: bool, pos third: bool)
            {{
                let selected: api.Choice = {region}
                {{
                    {guard}
                    {transfer} if branch
                    {{
                        yield api.Choice.Existing({created});
                    }}
                    else
                    {{
                        yield api.Choice.Empty;
                    }};
                }};

                match consume selected
                {{
                    case api.Choice.Existing(owner)
                    {{
                        api.inspect(&owner);
                    }}
                    case api.Choice.Empty
                    {{
                    }}
                }}
            }}
        "#);
        let source = if complex {
            source.replace("yield api.Choice.Empty;", &format!(r#"
                yield if other
                {{
                    yield api.Choice.Created({created});
                }}
                else
                {{
                    yield if third
                    {{
                        yield api.Choice.Empty;
                    }}
                    else
                    {{
                        yield api.Choice.Backing;
                    }};
                }};
            "#))
                .replace("case api.Choice.Empty", r#"
                    case api.Choice.Created(owner)
                    {
                        api.inspect(&owner);
                    }
                    case api.Choice.Backing
                    {
                    }
                    case api.Choice.Empty"#)
        } else { source };
        let source = if transfer == "return" {
            format!(r#"
                module app;

                using api;

                func caller(pos branch: bool, pos other: bool, pos third: bool) -> api.Worker
                    ensures(trusted api.live(owner = &result.marker))
                {{
                    {guard}
                    return {created};
                }}
            "#)
        } else { source };
        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                compilation_from_sources([declarations.as_str(), source.as_str()])
            };
            let diagnostics = application.check_diagnostics();
            assert_eq!(!diagnostics.has_errors(), valid,
                "complex={complex}, region={region}, guard={guard}, cleanup={cleanup}, imported={imported}: {source}\n{diagnostics:#?}");
            if valid {
                let lowered = application.lowered_unit(
                    crate::test_support::source_function_body_key(&application, "caller"))
                    .expect("produced result survives cleanup through lowering");

                assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");
                assert!(lowered.value().is_some(), "valid composition produces MIR: {lowered:#?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);
            }
        }
    }
}
}
}

#[test]
fn caught_nullable_results_preserve_container_evidence() {
    for asynchronous in [false, true] {
    let declarations = r#"
        trusted module api;

        struct Marker
        {
        }

        struct Worker
        {
            marker: Marker;
        }

        trusted predicate optional(owner: &(Worker?));
        extern func may_fail();

        trusted func create() -> Worker?
            ensures(trusted optional(owner = &result))
        {
            may_fail();

            let owner: Worker =
            {
                marker = Marker {}
            };

            return owner;
        }

        func take(pos owner: Worker?)
            requires(owner matches ?_, trusted optional(owner = &owner))
        {
        }
    "#;
    let declarations = if asynchronous {
        declarations.replace("trusted func create()", "trusted async func create()")
    } else { declarations.to_owned() };
    let provider = compilation(&declarations);
    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for (name, setup, valid) in [
        ("direct", "let value = trusted api.create();", true),
        ("caught", r#"
            let caught = catch trusted api.create();
            let value = match consume caught
            {
                case Ok(owner)
                {
                    yield owner;
                }
                case Error(_)
                {
                    return;
                }
            };
        "#, true),
        ("fabricated", r#"
            let owner: api.Worker =
            {
                marker = api.Marker {}
            };
            let value: api.Worker? = owner;
        "#, false),
    ] {
        let source = format!(r#"
            module app;

            using api;

            func caller()
            {{
                {setup}
                if value matches none
                {{
                    return;
                }}

                api.take(value);
            }}
        "#);
        let source = if asynchronous {
            source.replace("func caller()", "async func caller()")
                .replace("trusted api.create()", "await trusted api.create()")
        } else { source };
        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                compilation_from_sources([declarations.as_str(), source.as_str()])
            };
            let diagnostics = application.check_diagnostics();
            assert_eq!(!diagnostics.has_errors(), valid, "{name}, asynchronous={asynchronous}, imported={imported}: {source}\n{diagnostics:#?}");
            if valid {
                let lowered = application.lowered_unit(crate::test_support::source_function_body_key(&application, "caller"))
                    .expect("caught result has a checked lowering");

                assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");
                assert!(lowered.value().is_some(), "{lowered:#?}");
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);
            }
        }
    }
}
}

#[test]
fn ending_a_mutable_view_preserves_an_independent_borrowed_owner() {
    let declarations = r#"trusted module api;

struct Marker
{
}

struct Owner
{
    mut marker: Marker;
}

trusted predicate acquired(owner: &Marker);
predicate retained(owner: &Owner) = acquired(owner = &owner.marker);

trusted predicate held(owner: &Marker);

struct Lease
{
    marker: Marker;

    destruct()
    {
    }
}

trusted func acquire() -> Lease
    ensures(trusted held(owner = &result.marker))
{
    return
    {
        marker = Marker {}
    };
}


struct Entries
{
    mut counter: u32;
}

struct Anchor
{
    mut entries: Entries;
    owner: &Owner;
    lease: &Lease;
}

predicate access(owner: &Anchor) = retained(owner = owner.owner) && held(owner = &owner.lease.marker);

trusted func arena_anchor(pos owner: &Owner, pos lease: &Lease) -> Anchor
    requires(trusted retained(owner = owner), trusted held(owner = &lease.marker))
    ensures(trusted retained(owner = owner), trusted access(owner = &result))
{
    return
    {
        owner = owner,
        lease = lease,
        entries =
        {
            counter = 0
        }
    };
}

trusted func entries(pos anchor: &mut Anchor) -> &mut Entries
    requires(trusted access(owner = &anchor))
    ensures(trusted access(owner = &anchor))
{
    return &mut anchor.entries;
}

trusted func update(pos entries: &mut Entries, pos owner: &Owner)
    requires(trusted retained(owner = owner))
    ensures(trusted retained(owner = owner))
{
    entries.counter += 1;
}

func inspect(pos owner: &Owner)
    requires(trusted retained(owner = owner))
{
}


async func pause()
{
}
"#;
    let provider = compilation(declarations);
    assert!(!provider.check_diagnostics().has_errors(), "{:?}", provider.check_diagnostics());

    for (repeats, mutation, asynchronous) in [
        (false, false, false), (true, false, false),
        (false, false, true), (true, false, true),
        (false, true, false), (false, true, true),
    ] {
        let repeat = if repeats { "while" } else { "if" };
        let mode = if mutation { "&mut " } else { "&" };
        let asynchronous = if asynchronous { "async " } else { "" };
        let suspension = if asynchronous.is_empty() { "" } else { "await api.pause();" };
        let mutate = if mutation { "owner.marker = api.Marker {};" } else { "" };
        let source = format!(r#"
            module app;

            using api;

            {asynchronous}func caller(pos owner: {mode}api.Owner, pos again: bool)
                requires(trusted api.retained(owner = &owner))
            {{
                {repeat} again
                {{
                    let request: u32 =
                    {{
                        let lease = trusted api.acquire();
                        let mut anchor = trusted api.arena_anchor(&owner, &lease);
                        let current = trusted api.entries(&mut anchor);
                        trusted api.update(current, &owner);
                        {suspension}
                        yield 0;
                    }};

                    {mutate}
                    api.inspect(&owner);
                }}
            }}
        "#);
        for imported in [false, true] {
            let application = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                compilation_from_sources([declarations, source.as_str()])
            };
            let diagnostics = application.check_diagnostics();
            assert_eq!(!diagnostics.has_errors(), !mutation,
                "repeats={repeats}, mutation={mutation}, async={asynchronous}, imported={imported}: {source}\n{diagnostics:#?}");
            if mutation {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);
            } else {
                let lowered = application.lowered_unit(crate::test_support::source_function_body_key(&application, "caller"))
                    .expect("borrowed owner scope composition lowers");

                assert!(!lowered.diagnostics().has_errors(), "{lowered:#?}");
                assert!(lowered.value().is_some(), "{lowered:#?}");
            }
        }
    }
}
