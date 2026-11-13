use super::fixtures::{compilation, compilation_from_sources, execution_consumer};

const API: &str = r"
    trusted module api;

    using core.memory;

    struct Marker {}

    extern func external();

    struct Cell
    {
        mut value: u32;
    }

    func opaque_cell(pos value: &Cell) -> Future<(&Cell)>
    {
        return pass<(&Cell)>(value);
    }

    struct State
    {
        mut count: u32;
    }

    trusted predicate unchanged(owner: &State);

    trusted func marked_state() -> State
        ensures(trusted unchanged(owner = &result))
    {
        return { count = 0 };
    }

    func observe_state(pos value: &State)
        requires(trusted unchanged(owner = value))
    {
    }

    async func change_state(pos mut value: State) -> State
    {
        value.count += 1;
        return value;
    }

    extern func mutate_state(pos value: &mut State);

    async func opaque_change_state(pos mut value: State) -> State
    {
        mutate_state(&mut value);
        return value;
    }

    struct Permit
    {
        id: u32;
    }

    trusted predicate permitted(id: u32);

    trusted func permission() -> Permit
        ensures(trusted permitted(id = result.id))
    {
        return { id = 1 };
    }

    func inspect_permission(pos id: u32)
        requires(trusted permitted(id = id))
    {
    }

    async func discard<T>(pos value: T) {}

    async func maybe<T>(pos value: T, pos keep: bool) -> T?
    {
        if keep
        {
            return value;
        }

        return none;
    }

    async func maybe_fresh(pos value: Permit, pos keep: bool) -> Permit?
    {
        if keep
        {
            return value;
        }

        let fresh: Permit = { id = 2 };

        return fresh;
    }

    trusted predicate held(owner: &Marker);

    trusted func make() -> Marker
        ensures(trusted held(owner = &result))
    {
        return Marker {};
    }

    func observe(pos value: &Marker)
        requires(trusted held(owner = value))
    {
    }

    async func pass<T>(pos value: T) -> T
    {
        return value;
    }

    async func formal_tuple(pos input: (Marker, u32), pos cancel: bool) -> (Marker, u32)
        requires(trusted held(owner = &input.0))
    {
        let wrapper: Box<(Marker, u32)> = { value = input };
        let task = pass<Box<(Marker, u32)>>(wrapper).start();

        let value = if cancel
        {
            yield try await task.cancel();
        }
        else
        {
            yield try await task.join();
        };

        observe(&value.value.0);
        return value.value;
    }

    async func formal_array(pos input: [Marker; 2], pos cancel: bool) -> [Marker; 2]
        requires(trusted held(owner = &input[1]))
    {
        let task = pass<[Marker; 2]>(input).start();

        let value = if cancel
        {
            yield try await task.cancel();
        }
        else
        {
            yield try await task.join();
        };

        observe(&value[1]);
        return value;
    }

    async func task_result<T>(pos value: T, pos cancel: bool) -> RunResult<T>
    {
        let task = pass<T>(value).start();

        if cancel
        {
            return await task.cancel();
        }

        return await task.join();
    }

    func opaque(pos value: Marker) -> Future<Marker>
    {
        return pass<Marker>(value);
    }

    async func other<T>(pos value: T) -> T
    {
        return value;
    }

    async func touch<T>(pos value: T, pos state: &mut State) -> T
    {
        state.count += 1;
        return value;
    }

    func forward<T>(pos value: T) -> T
    {
        return value;
    }

    async func choose_pending<T>(pos left: Future<T>, pos right: Future<T>, pos first: bool) -> Future<T>
    {
        if first
        {
            return left;
        }

        return right;
    }

    func stop() -> never
    {
        loop {}
    }

    async func only<T>(pos value: T, pos run: bool) -> T
    {
        if run
        {
            return value;
        }

        stop();
    }

    func observe_reference(pos value: &u32) {}

    struct Address
    {
        marker: Marker;
        pointer: DataPointer<u32>;
    }

    trusted func address(pos value: &u32) -> Address
        ensures(
            trusted held(owner = &result.marker),
            trusted core.memory.valid_read<u32>(pointer = result.pointer, count = 1)
        )
    {
        return
        {
            marker = Marker {},
            pointer = core.memory.address_of<u32>(value)
        };
    }

    func observe_address(pos value: &Address)
        requires(trusted core.memory.valid_read<u32>(pointer = value.pointer, count = 1))
    {
    }

    func observe_independent(pos value: &Address)
        requires(trusted held(owner = &value.marker))
    {
    }
    struct Box<T>
    {
        value: T;
    }

    func boxed<T>(pos value: T) -> Box<T>
    {
        return
        {
            value = value
        };
    }

    async func swap<T>(pos left: T, pos right: T) -> (T, T)
    {
        return (right, left);
    }

    trusted predicate sealed(owner: &Box<Marker>);

    trusted func seal() -> Box<Marker>
        ensures(trusted sealed(owner = &result))
    {
        return
        {
            value = Marker {}
        };
    }

    async func optional_extract(pos value: Box<Marker>, pos take: bool) -> Marker?
        ensures(take || result matches none)
    {
        if take
        {
            return value.value;
        }

        return none;
    }

    async func code_extract(pos value: Box<Marker>, pos code: u32) -> Marker?
        ensures(code == 0 || result matches none)
    {
        if code == 0
        {
            return value.value;
        }

        return none;
    }

    trusted async func real_marker(pos value: r64) -> Marker
        when(value == value)
        {
            ensures(trusted held(owner = &result))
        }
    {
        return Marker {};
    }

    trusted async func real_pair(pos left: r64, pos right: r64) -> Marker
        when(left == right)
        {
            ensures(trusted held(owner = &result))
        }
    {
        return Marker {};
    }

    union Envelope<T>
    {
        Stored(pos value: T);
        Empty;
    }

    func staged<T>(pos value: T, ignored: bool = false) -> T
    {
        return value;
    }

    struct Flag
    {
        value: bool;
    }

    async func flag_marker(pos flag: Flag) -> Marker
        requires(flag.value)
        ensures(trusted held(owner = &result))
    {
        return make();
    }

";

const CASES: &[(&str, &str, bool)] = &[
    (
        "formal_tuple_task_resumes_nested_carrier",
        r"let value = await api.formal_tuple((api.make(), 0), choice); api.observe(&value.0);",
        true,
    ),
    (
        "formal_array_task_resumes_nested_carrier",
        r"let input: [api.Marker; 2] = [api.Marker {}, api.make()]; let value = await api.formal_array(input, choice); api.observe(&value[1]);",
        true,
    ),
    (
        "formal_tuple_rejects_another_owners_carrier",
        r"let unrelated = api.make(); let value = await api.formal_tuple((api.Marker {}, 0), choice); api.observe(&value.0);",
        false,
    ),
    (
        "implicit_nullable_task_retains_frame",
        r"let task = api.pass<api.Marker>(api.make()).start(); let holder = api.forward<(Task<api.Marker>?)>(task); match consume holder { case ?moved { let value = try await moved.join(); api.observe(&value); } case none {} }",
        true,
    ),
    (
        "staged_nullable_task_retains_adapted_frame",
        r"let task = api.pass<api.Marker>(api.make()).start(); let holder = api.staged<(Task<api.Marker>?)>(task); match consume holder { case ?moved { let value = try await moved.cancel(); api.observe(&value); } case none {} }",
        true,
    ),
    (
        "union_future_retains_capture_identity",
        r"let input: api.Envelope<Future<api.Marker>> = api.Envelope.Stored(api.pass<api.Marker>(api.make())); let holder = await api.pass<api.Envelope<Future<api.Marker>>>(input); match consume holder { case Stored(pending) { let value = await pending; api.observe(&value); } case Empty {} }",
        true,
    ),
    (
        "union_future_retains_live_reference",
        r"let cell: api.Cell = { value = 1 }; let input: api.Envelope<Future<(&api.Cell)>> = api.Envelope.Stored(api.pass<(&api.Cell)>(&cell)); let holder = await api.pass<api.Envelope<Future<(&api.Cell)>>>(input); match consume holder { case Stored(pending) { let value = await pending; _ = value.value; } case Empty {} }",
        true,
    ),
    (
        "task_returns_nullable_future",
        r"let formed = api.pass<api.Marker>(api.make()); let input: Future<api.Marker>? = formed; let holder = try await api.pass<(Future<api.Marker>?)>(input).start().join(); match consume holder { case ?pending { let value = await pending; api.observe(&value); } case none {} }",
        true,
    ),
    (
        "task_returns_union_future",
        r"let formed = api.pass<api.Marker>(api.make()); let input: api.Envelope<Future<api.Marker>> = api.Envelope.Stored(formed); let holder = try await api.pass<api.Envelope<Future<api.Marker>>>(input).start().join(); match consume holder { case Stored(pending) { let value = await pending; api.observe(&value); } case Empty {} }",
        true,
    ),
    (
        "union_task_input_keeps_authority_private",
        r"let permit = api.permission(); let id = permit.id; let pending = api.pass<api.Permit>(permit); let input: api.Envelope<Future<api.Permit>> = api.Envelope.Stored(pending); let task = api.discard<api.Envelope<Future<api.Permit>>>(input).start(); api.inspect_permission(id); _ = await task.join();",
        false,
    ),
    (
        "nullable_task_result_keeps_physical_address_bounds",
        r"let value = { let payload: u32 = 4; let input: Future<api.Address>? = api.pass<api.Address>(api.address(&payload)); let holder = try await api.pass<(Future<api.Address>?)>(input).start().join(); yield match consume holder { case ?pending { yield await pending; } case none { loop {} } }; }; api.observe_address(&value);",
        false,
    ),
    (
        "nullable_task_result_keeps_independent_evidence",
        r"let value = { let payload: u32 = 4; let input: Future<api.Address>? = api.pass<api.Address>(api.address(&payload)); let holder = try await api.pass<(Future<api.Address>?)>(input).start().join(); yield match consume holder { case ?pending { yield await pending; } case none { loop {} } }; }; api.observe_independent(&value);",
        true,
    ),
    (
        "task_normal_or_diverging_producer",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.only<api.Marker>(api.make(), false); }; let task = pending.start(); let value = try await task.join(); api.observe(&value);",
        true,
    ),
    (
        "task_opaque_body_mutation_retires_input_evidence",
        r"let task = api.opaque_change_state(api.marked_state()).start(); let value = try await task.join(); api.observe_state(&value);",
        false,
    ),
    (
        "returned_task_in_array_retains_frame",
        r"let task = api.pass<api.Marker>(api.make()).start(); let holder = await api.pass<[Task<api.Marker>; 1]>([task]); let [forwarded] = holder; let value = try await forwarded.join(); api.observe(&value);",
        true,
    ),
    (
        "forwarded_task_retains_frame",
        r"let task = api.pass<api.Marker>(api.make()).start(); let forwarded = api.forward<Task<api.Marker>>(task); let value = try await forwarded.join(); api.observe(&value);",
        true,
    ),
    (
        "returned_task_in_product_retains_frame",
        r"let task = api.pass<api.Marker>(api.make()).start(); let holder = await api.pass<api.Box<Task<api.Marker>>>(api.boxed<Task<api.Marker>>(task)); let value = try await holder.value.cancel(); api.observe(&value);",
        true,
    ),
    (
        "task_returns_inactive_future",
        r"let task = api.pass<Future<api.Marker>>(api.pass<api.Marker>(api.make())).start(); let pending = try await task.join(); let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "task_returns_observation_future",
        r"let inner = api.pass<api.Marker>(api.make()).start(); let outer = inner.join().start(); let outcome = try await outer.cancel(); let value = try outcome; api.observe(&value);",
        true,
    ),
    (
        "forwarded_task_cannot_create_witness",
        r"let task = api.pass<api.Marker>(api.Marker {}).start(); let forwarded = api.forward<Task<api.Marker>>(task); let value = try await forwarded.join(); api.observe(&value);",
        false,
    ),
    (
        "nested_task_cannot_create_witness",
        r"let task = api.pass<Future<api.Marker>>(api.pass<api.Marker>(api.Marker {})).start(); let pending = try await task.cancel(); let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "normal_reference_releases_frame_borrow",
        r"let cell: api.Cell = { value = 1 }; let result = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<(&api.Cell)>(&cell, &mut state); } else { yield api.pass<(&api.Cell)>(&cell); }; yield await pending; }; _ = result.value;",
        true,
    ),
    (
        "task_reference_releases_frame_borrow",
        r"let cell: api.Cell = { value = 1 }; let outcome = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<(&api.Cell)>(&cell, &mut state); } else { yield api.opaque_cell(&cell); }; let task = pending.start(); yield await task.join(); }; match consume outcome { case Completed(value) { _ = value.value; } case Panicked(report) {} case Cancelled {} }",
        true,
    ),
    (
        "opaque_await_preserves_report_borrow",
        r"let cell: api.Cell = { value = 1 }; let result = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<(&api.Cell)>(&cell, &mut state); } else { yield api.opaque_cell(&cell); }; yield await pending; }; _ = result.value;",
        false,
    ),
    (
        "task_known_result_after_opaque",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; let task = pending.start(); let value = try await task.join(); api.external(); if choice { api.observe(&value); }",
        true,
    ),
    (
        "task_opaque_result_after_opaque",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; let task = pending.start(); let value = try await task.join(); api.external(); if choice { api.observe(&value); }",
        true,
    ),
    (
        "opaque_frame_borrow_released",
        r"let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<api.Marker>(api.make(), &mut state); } else { yield api.opaque(api.Marker {}); }; let value = await pending; if choice { api.observe(&value); }",
        true,
    ),
    (
        "task_pending_permission_unavailable",
        r"let permit = api.permission(); let id = permit.id; let pending = if choice { yield api.pass<api.Permit>(permit); } else { yield api.other<api.Permit>(api.Permit { id = id }); }; let task = pending.start(); if choice { api.inspect_permission(id); }",
        false,
    ),
    (
        "task_unrelated_join_permission_unavailable",
        r"let permit = api.permission(); let id = permit.id; let task = api.pass<api.Permit>(permit).start(); let value = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; if choice { api.inspect_permission(id); }",
        false,
    ),
    (
        "task_join_opaque_reference_live",
        r"let cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); let result = try await task.join(); _ = result.value;",
        true,
    ),
    (
        "task_join_opaque_reference_mutation",
        r"let mut cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); let result = try await task.join(); cell.value = 2; _ = result.value;",
        false,
    ),
    (
        "task_cancel_opaque_reference_live",
        r"let cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); let result = try await task.cancel(); _ = result.value;",
        true,
    ),
    (
        "task_cancel_opaque_reference_mutation",
        r"let mut cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); let result = try await task.cancel(); cell.value = 2; _ = result.value;",
        false,
    ),
    (
        "task_join_reference_escape",
        r"let result = { let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); yield try await task.join(); }; _ = result.value;",
        false,
    ),
    (
        "task_join_reference_mutation",
        r"let mut cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); let result = try await task.join(); cell.value = 2; _ = result.value;",
        false,
    ),
    (
        "task_join_reference_live",
        r"let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); let result = try await task.join(); _ = result.value;",
        true,
    ),
    (
        "task_join_reference_match_escape",
        r"let outcome = { let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); yield await task.join(); }; match consume outcome { case Completed(result) { _ = result.value; } case Panicked(report) {} case Cancelled {} }",
        false,
    ),
    (
        "task_join_opaque_reference_escape",
        r"let result = { let cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); yield try await task.join(); }; _ = result.value;",
        false,
    ),
    (
        "task_cancel_reference_escape",
        r"let result = { let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); yield try await task.cancel(); }; _ = result.value;",
        false,
    ),
    (
        "task_cancel_reference_mutation",
        r"let mut cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); let result = try await task.cancel(); cell.value = 2; _ = result.value;",
        false,
    ),
    (
        "task_cancel_reference_live",
        r"let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); let result = try await task.cancel(); _ = result.value;",
        true,
    ),
    (
        "task_cancel_reference_match_escape",
        r"let outcome = { let cell: api.Cell = { value = 1 }; let task = api.pass<(&api.Cell)>(&cell).start(); yield await task.cancel(); }; match consume outcome { case Completed(result) { _ = result.value; } case Panicked(report) {} case Cancelled {} }",
        false,
    ),
    (
        "task_cancel_opaque_reference_escape",
        r"let result = { let cell: api.Cell = { value = 1 }; let task = api.opaque_cell(&cell).start(); yield try await task.cancel(); }; _ = result.value;",
        false,
    ),
    (
        "task_forwarded_reference_escape",
        r"let outcome = { let cell: api.Cell = { value = 1 }; yield await api.task_result<(&api.Cell)>(&cell, choice); }; match consume outcome { case Completed(result) { _ = result.value; } case Panicked(report) {} case Cancelled {} }",
        false,
    ),
    (
        "task_forwarded_reference_live",
        r"let cell: api.Cell = { value = 1 }; let outcome = await api.task_result<(&api.Cell)>(&cell, choice); match consume outcome { case Completed(result) { _ = result.value; } case Panicked(report) {} case Cancelled {} }",
        true,
    ),
    (
        "preformed_future_selected_after_completion",
        r"let left = api.pass<api.Marker>(api.make()); let right = api.other<api.Marker>(api.Marker {}); let pending = if choice { yield left; } else { yield right; };let value = await pending;if choice { api.observe(&value); }",
        true,
    ),
    (
        "preformed_future_fabricated_after_completion",
        r"let left = api.pass<api.Marker>(api.make()); let right = api.other<api.Marker>(api.Marker {}); let pending = if choice { yield left; } else { yield right; };let value = await pending;if !choice { api.observe(&value); }",
        false,
    ),
    (
        "opaque_future_selected_after_completion",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); };let value = await pending;if choice { api.observe(&value); }",
        true,
    ),
    (
        "opaque_future_fabricated_after_completion",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); };let value = await pending;if !choice { api.observe(&value); }",
        false,
    ),
    (
        "preformed_task_selected_after_completion",
        r"let left = api.pass<api.Marker>(api.make()); let right = api.other<api.Marker>(api.Marker {}); let pending = if choice { yield left; } else { yield right; };let task = pending.start(); let value = try await task.join();if choice { api.observe(&value); }",
        true,
    ),
    (
        "preformed_task_fabricated_after_completion",
        r"let left = api.pass<api.Marker>(api.make()); let right = api.other<api.Marker>(api.Marker {}); let pending = if choice { yield left; } else { yield right; };let task = pending.start(); let value = try await task.join();if !choice { api.observe(&value); }",
        false,
    ),
    (
        "opaque_task_selected_after_completion",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); };let task = pending.start(); let value = try await task.join();if choice { api.observe(&value); }",
        true,
    ),
    (
        "opaque_task_fabricated_after_completion",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); };let task = pending.start(); let value = try await task.join();if !choice { api.observe(&value); }",
        false,
    ),
    (
        "returned_task_outcome",
        r"let result = await api.task_result(api.make(), choice);
            if let RunResult.Completed(value) = result { api.observe(&value); }",
        true,
    ),
    (
        "returned_task_outcome_fabricated",
        r"let result = await api.task_result(api.Marker {}, choice);
            if let RunResult.Completed(value) = result { api.observe(&value); }",
        false,
    ),
    (
        "unknown_alternative",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "unknown_refined",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; if choice { let value = await pending; api.observe(&value); }",
        true,
    ),
    (
        "unknown_selected",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; if !choice { let value = await pending; api.observe(&value); }",
        false,
    ),
    (
        "unknown_mutated_selector",
        r"let mut selected = choice; let pending = if selected { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; selected = true; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "copied_guard_normal",
        r"let mut run = choice; let pending = api.only<api.Marker>(api.Marker {}, run); run = false; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "copied_guard_partial",
        r"let mut take = true; let pending = api.optional_extract(api.seal(), take); take = false; _ = await pending;",
        false,
    ),
    (
        "copied_guard_inactive",
        r"let mut take = false; let pending = api.optional_extract(api.seal(), take); take = true; _ = await pending;",
        true,
    ),
    (
        "direct",
        r"let value = await api.pass<api.Marker>(api.make()); api.observe(&value);",
        true,
    ),
    (
        "alias",
        r"let first = api.pass<api.Marker>(api.make()); let second = first; let value = await second; api.observe(&value);",
        true,
    ),
    (
        "distinct_sites",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.pass<api.Marker>(api.make()); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "distinct_declarations",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.make()); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "both_fabricated",
        r"let pending = if choice { yield api.pass<api.Marker>(api.Marker {}); } else { yield api.other<api.Marker>(api.Marker {}); }; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "left_fabricated",
        r"let pending = if choice { yield api.pass<api.Marker>(api.Marker {}); } else { yield api.other<api.Marker>(api.make()); }; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "right_fabricated",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "known_left",
        r"let pending = if true { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "known_right",
        r"let pending = if false { yield api.pass<api.Marker>(api.Marker {}); } else { yield api.other<api.Marker>(api.make()); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "refined_left",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; if choice { let value = await pending; api.observe(&value); }",
        true,
    ),
    (
        "refined_right",
        r"let pending = if choice { yield api.pass<api.Marker>(api.Marker {}); } else { yield api.other<api.Marker>(api.make()); }; if !choice { let value = await pending; api.observe(&value); }",
        true,
    ),
    (
        "selected_fabrication",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; if !choice { let value = await pending; api.observe(&value); }",
        false,
    ),
    (
        "two_live",
        r"let first = api.pass<api.Marker>(api.make()); let second = api.pass<api.Marker>(api.make()); let left = await first; api.observe(&left); let right = await second; api.observe(&right); api.observe(&left);",
        true,
    ),
    (
        "two_live_independent_fabrication",
        r"let first = api.pass<api.Marker>(api.make()); let second = api.pass<api.Marker>(api.Marker {}); let left = await first; api.observe(&left); let right = await second; api.observe(&right);",
        false,
    ),
    (
        "identity_helper",
        r"let pending = api.forward<Future<api.Marker>>(api.pass<api.Marker>(api.make())); let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "helper_choice",
        r"let pending = await api.choose_pending<api.Marker>(api.pass<api.Marker>(api.make()), api.other<api.Marker>(api.make()), choice); let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "helper_choice_fabrication",
        r"let pending = await api.choose_pending<api.Marker>(api.pass<api.Marker>(api.make()), api.other<api.Marker>(api.Marker {}), choice); let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "dormant",
        r"let pending: Future<api.Marker> = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.only<api.Marker>(api.make(), false); };",
        true,
    ),
    (
        "normal_or_diverging",
        r"let pending = if choice { yield api.only<api.Marker>(api.make(), true); } else { yield api.only<api.Marker>(api.Marker {}, false); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "only_diverging",
        r"let pending = if choice { yield api.only<api.Marker>(api.Marker {}, false); } else { yield api.only<api.Marker>(api.Marker {}, false); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "diverging_or_fabrication",
        r"let pending = if choice { yield api.only<api.Marker>(api.make(), false); } else { yield api.only<api.Marker>(api.Marker {}, true); }; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "mutated_selector",
        r"let mut selected = choice; let pending = if selected { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.make()); }; selected = !selected; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "mutated_selector_fabrication",
        r"let mut selected = choice; let pending = if selected { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; selected = true; let value = await pending; api.observe(&value);",
        false,
    ),
    (
        "frame_borrow_ends",
        r"let value = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<api.Marker>(api.make(), &mut state); } else { yield api.touch<api.Marker>(api.make(), &mut state); }; yield await pending; }; api.observe(&value);",
        true,
    ),
    (
        "address_live",
        r"let payload: u32 = 4; let value = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<api.Address>(api.address(&payload), &mut state); } else { yield api.touch<api.Address>(api.address(&payload), &mut state); }; yield await pending; }; api.observe_address(&value);",
        true,
    ),
    (
        "address_expired",
        r"let value = { let payload: u32 = 4; let pending = if choice { yield api.pass<api.Address>(api.address(&payload)); } else { yield api.other<api.Address>(api.address(&payload)); }; yield await pending; }; api.observe_address(&value);",
        false,
    ),
    (
        "independent_address_marker",
        r"let value = { let payload: u32 = 4; let pending = if choice { yield api.pass<api.Address>(api.address(&payload)); } else { yield api.other<api.Address>(api.address(&payload)); }; yield await pending; }; api.observe_independent(&value);",
        true,
    ),
    (
        "borrow_live",
        r"let payload: u32 = 4; let value = { let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<(&u32)>(&payload, &mut state); } else { yield api.touch<(&u32)>(&payload, &mut state); }; yield await pending; }; api.observe_reference(value);",
        true,
    ),
    (
        "borrow_expired",
        r"let value = { let payload: u32 = 4; let pending = if choice { yield api.pass<(&u32)>(&payload); } else { yield api.other<(&u32)>(&payload); }; yield await pending; }; api.observe_reference(value);",
        false,
    ),
    (
        "same_site_loop",
        r"
        let mut first: Future<api.Marker>? = none;
        let mut second: Future<api.Marker>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.pass<api.Marker>(api.make());
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume first { case ?pending { let value = await pending; api.observe(&value); } case none {} }
        match consume second { case ?pending { let value = await pending; api.observe(&value); } case none {} }
    ",
        true,
    ),
    (
        "same_site_fabrication",
        r"
        let mut first: Future<api.Marker>? = none;
        let mut second: Future<api.Marker>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let marker = if index == 0 { yield api.make(); } else { yield api.Marker {}; };
            let pending = api.pass<api.Marker>(marker);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume second { case ?pending { let value = await pending; api.observe(&value); } case none {} }
    ",
        false,
    ),
    (
        "tuple_owners",
        r"let pair = (api.pass<api.Marker>(api.make()), api.other<api.Marker>(api.make())); let (first, second) = pair; let left = await first; api.observe(&left); let right = await second; api.observe(&right); api.observe(&left);",
        true,
    ),
    (
        "array_owners",
        r"let values = [api.pass<api.Marker>(api.make()), api.other<api.Marker>(api.make())]; let [first, second] = values; let left = await first; api.observe(&left); let right = await second; api.observe(&right);",
        true,
    ),
    (
        "boxed_future",
        r"let wrapped = api.boxed<Future<api.Marker>>(api.pass<api.Marker>(api.make())); let value = await wrapped.value; api.observe(&value);",
        true,
    ),
    (
        "boxed_capture",
        r"let value = await api.pass<api.Box<api.Marker>>(api.boxed<api.Marker>(api.make())); api.observe(&value.value);",
        true,
    ),
    (
        "nullable_owner",
        r"let pending: Future<api.Marker>? = if choice { yield api.pass<api.Marker>(api.make()); } else { yield none; }; match consume pending { case ?selected { let value = await selected; api.observe(&value); } case none {} }",
        true,
    ),
    (
        "swap_both_valid",
        r"let pending = if choice { yield api.swap<api.Marker>(api.make(), api.make()); } else { yield api.swap<api.Marker>(api.make(), api.make()); }; let (left, right) = await pending; api.observe(&left); api.observe(&right);",
        true,
    ),
    (
        "swap_independent",
        r"let pending = if choice { yield api.swap<api.Marker>(api.make(), api.Marker {}); } else { yield api.swap<api.Marker>(api.make(), api.Marker {}); }; let (left, right) = await pending; api.observe(&right);",
        true,
    ),
    (
        "swap_fabrication",
        r"let pending = if choice { yield api.swap<api.Marker>(api.make(), api.Marker {}); } else { yield api.swap<api.Marker>(api.make(), api.Marker {}); }; let (left, right) = await pending; api.observe(&left);",
        false,
    ),
    (
        "inactive_partial",
        r"let pending = if choice { yield api.optional_extract(api.seal(), false); } else { yield api.optional_extract(api.seal(), false); }; _ = await pending;",
        true,
    ),
    (
        "active_partial",
        r"let pending = if choice { yield api.optional_extract(api.seal(), true); } else { yield api.optional_extract(api.seal(), true); }; _ = await pending;",
        false,
    ),
    (
        "dormant_partial",
        r"let pending = if choice { yield api.optional_extract(api.seal(), true); } else { yield api.optional_extract(api.seal(), true); };",
        true,
    ),
    (
        "captured_field_fact",
        r"let pending = if choice { yield api.flag_marker(api.Flag { value = true }); } else { yield api.flag_marker(api.Flag { value = true }); }; let value = await pending; api.observe(&value);",
        true,
    ),
    (
        "task_direct",
        r"let task = api.pass<api.Marker>(api.make()).start(); let value = try await task.join(); api.observe(&value);",
        true,
    ),
    (
        "task_selected_future",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.make()); }; let task = pending.start(); let value = try await task.join(); api.observe(&value);",
        true,
    ),
    (
        "task_cancel_completion",
        r"let task = api.pass<api.Marker>(api.make()).start(); let value = try await task.cancel(); api.observe(&value);",
        true,
    ),
    (
        "task_cancel_fabrication",
        r"let task = api.pass<api.Marker>(api.Marker {}).start(); let value = try await task.cancel(); api.observe(&value);",
        false,
    ),
    (
        "task_alias",
        r"let task = api.pass<api.Marker>(api.make()).start(); let alias = task; let value = try await alias.join(); api.observe(&value);",
        true,
    ),
    (
        "task_frame_borrow_ends",
        r"let value = { let mut state: api.State = { count = 0 }; let task = api.touch<api.Marker>(api.make(), &mut state).start(); yield try await task.join(); }; api.observe(&value);",
        true,
    ),
    (
        "task_unknown_alternative",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; let task = pending.start(); let value = try await task.join(); api.observe(&value);",
        false,
    ),
    (
        "task_partial_transfer",
        r"let task = api.optional_extract(api.seal(), true).start(); _ = try await task.join();",
        false,
    ),
    (
        "owned_body_mutation",
        r"let value = await api.change_state(api.marked_state()); api.observe_state(&value);",
        false,
    ),
    (
        "selected_body_mutation",
        r"let pending = if choice { yield api.pass<api.State>(api.marked_state()); } else { yield api.change_state(api.marked_state()); }; let value = await pending; api.observe_state(&value);",
        false,
    ),
    (
        "task_owned_body_mutation",
        r"let task = api.change_state(api.marked_state()).start(); let value = try await task.join(); api.observe_state(&value);",
        false,
    ),
    (
        "task_owned_state",
        r"let task = api.pass<api.State>(api.marked_state()).start(); let value = try await task.join(); api.observe_state(&value);",
        true,
    ),
    (
        "task_address_live",
        r"let payload: u32 = 4; let task = api.pass<api.Address>(api.address(&payload)).start(); let value = try await task.join(); api.observe_address(&value);",
        true,
    ),
    (
        "task_address_expired",
        r"let value = { let payload: u32 = 4; let task = api.pass<api.Address>(api.address(&payload)).start(); yield try await task.join(); }; api.observe_address(&value);",
        false,
    ),
    (
        "task_independent_address_marker",
        r"let value = { let payload: u32 = 4; let task = api.pass<api.Address>(api.address(&payload)).start(); yield try await task.join(); }; api.observe_independent(&value);",
        true,
    ),
    (
        "task_completed_permission",
        r"let permit = api.permission(); let id = permit.id; let task = api.pass<api.Permit>(permit).start(); let value = try await task.join(); api.inspect_permission(id);",
        true,
    ),
    (
        "nullable_completed_permission",
        r"let permit = api.permission(); let id = permit.id; let value = await api.maybe<api.Permit>(permit, choice); match consume value { case ?owner { api.inspect_permission(id); } case none {} }",
        true,
    ),
    (
        "nullable_fresh_permission",
        r"
            let permit = api.permission();
            let id = permit.id;
            let value = await api.maybe_fresh(permit, choice);

            match consume value
            {
                case ?owner
                {
                    api.inspect_permission(id);
                }
                case none
                {
                }
            }
        ",
        false,
    ),
    (
        "nullable_dropped_permission",
        r"let id = { let permit = api.permission(); let id = permit.id; let value = await api.maybe<api.Permit>(permit, choice); match consume value { case ?owner { api.inspect_permission(id); } case none {} } yield id; }; api.inspect_permission(id);",
        false,
    ),
    (
        "task_unobserved_outcome_permission",
        r"let permit = api.permission(); let id = permit.id; let task = api.pass<api.Permit>(permit).start(); let outcome = await task.cancel(); api.inspect_permission(id);",
        false,
    ),
    (
        "task_dropped_permission",
        r"let id = { let permit = api.permission(); let id = permit.id; let task = api.pass<api.Permit>(permit).start(); let value = try await task.join(); yield id; }; api.inspect_permission(id);",
        false,
    ),
    (
        "task_active_discarded_permission",
        r"let permit = api.permission(); let id = permit.id; let task = api.discard<api.Permit>(permit).start(); api.inspect_permission(id); _ = await task.join();",
        false,
    ),
    (
        "task_discarded_permission",
        r"let permit = api.permission(); let id = permit.id; let task = api.discard<api.Permit>(permit).start(); _ = try await task.join(); api.inspect_permission(id);",
        false,
    ),
    (
        "nullable_address_live",
        r"let payload: u32 = 4; let pending: Future<api.Address>? = if choice { yield api.pass<api.Address>(api.address(&payload)); } else { yield none; }; match consume pending { case ?selected { let value = await selected; api.observe_address(&value); } case none {} }",
        true,
    ),
    (
        "nullable_address_expired",
        r"let mut stored: api.Address? = none; { let payload: u32 = 4; let pending: Future<api.Address>? = if choice { yield api.pass<api.Address>(api.address(&payload)); } else { yield none; }; match consume pending { case ?selected { stored = await selected; } case none {} } } match consume stored { case ?received { api.observe_address(&received); } case none {} }",
        false,
    ),
    (
        "nullable_independent_marker",
        r"let mut stored: api.Address? = none; { let payload: u32 = 4; let pending: Future<api.Address>? = if choice { yield api.pass<api.Address>(api.address(&payload)); } else { yield none; }; match consume pending { case ?selected { stored = await selected; } case none {} } } match consume stored { case ?received { api.observe_independent(&received); } case none {} }",
        true,
    ),
    (
        "refined_owned_body_effects",
        r"let pending = if choice { yield api.pass<api.State>(api.marked_state()); } else { yield api.change_state(api.marked_state()); }; if choice { let value = await pending; api.observe_state(&value); }",
        true,
    ),
    (
        "task_refined_owned_body_effects",
        r"let pending = if choice { yield api.pass<api.State>(api.marked_state()); } else { yield api.change_state(api.marked_state()); }; if choice { let task = pending.start(); let value = try await task.join(); api.observe_state(&value); }",
        true,
    ),
    (
        "task_selected_owned_body_mutation",
        r"let pending = if choice { yield api.pass<api.State>(api.marked_state()); } else { yield api.change_state(api.marked_state()); }; if !choice { let task = pending.start(); let value = try await task.join(); api.observe_state(&value); }",
        false,
    ),
    (
        "task_unknown_refined",
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.opaque(api.Marker {}); }; if choice { let task = pending.start(); let value = try await task.join(); api.observe(&value); }",
        true,
    ),
    (
        "task_fabrication",
        r"let task = api.pass<api.Marker>(api.Marker {}).start(); let value = try await task.join(); api.observe(&value);",
        false,
    ),
    (
        "captured_guard_refined",
        r"let marker = if choice { yield api.make(); } else { yield api.Marker {}; }; let pending = api.pass<api.Marker>(marker); if choice { let value = await pending; api.observe(&value); }",
        true,
    ),
    (
        "captured_guard_unselected",
        r"let marker = if choice { yield api.make(); } else { yield api.Marker {}; }; let pending = api.pass<api.Marker>(marker); if !choice { let value = await pending; api.observe(&value); }",
        false,
    ),
    (
        "captured_guard_mutated",
        r"let mut selected = choice; let marker = if selected { yield api.make(); } else { yield api.Marker {}; }; let pending = api.pass<api.Marker>(marker); selected = true; if selected { let value = await pending; api.observe(&value); }",
        false,
    ),
    (
        "same_site_copied_inactive_guard",
        r"
        let mut first: Future<(api.Marker?)>? = none;
        let mut second: Future<(api.Marker?)>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.optional_extract(api.seal(), index != 0);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume first { case ?pending { _ = await pending; } case none {} }
",
        true,
    ),
    (
        "same_site_copied_active_guard",
        r"
        let mut first: Future<(api.Marker?)>? = none;
        let mut second: Future<(api.Marker?)>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.optional_extract(api.seal(), index != 0);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume second { case ?pending { _ = await pending; } case none {} }
",
        false,
    ),
    (
        "same_site_numeric_active_guard",
        r"
        let mut first: Future<(api.Marker?)>? = none;
        let mut second: Future<(api.Marker?)>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.code_extract(api.seal(), index);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume first { case ?pending { _ = await pending; } case none {} }
",
        false,
    ),
    (
        "same_site_numeric_inactive_guard",
        r"
        let mut first: Future<(api.Marker?)>? = none;
        let mut second: Future<(api.Marker?)>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.code_extract(api.seal(), index);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume second { case ?pending { _ = await pending; } case none {} }
",
        true,
    ),
    (
        "same_site_independent_first",
        r"
        let mut first: Future<api.Marker>? = none;
        let mut second: Future<api.Marker>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let marker = if index == 0 { yield api.make(); } else { yield api.Marker {}; };
            let pending = api.pass<api.Marker>(marker);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume first { case ?pending { let value = await pending; api.observe(&value); } case none {} }
",
        true,
    ),
    (
        "same_site_independent_second",
        r"
        let mut first: Future<api.Marker>? = none;
        let mut second: Future<api.Marker>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let marker = if index == 0 { yield api.make(); } else { yield api.Marker {}; };
            let pending = api.pass<api.Marker>(marker);
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume second { case ?pending { let value = await pending; api.observe(&value); } case none {} }
",
        false,
    ),
    (
        "copied_real_equality_unknown",
        r"
        let value: r64 = if choice { yield 1.0; } else { yield 0.0 / 0.0; };
        let marker = await api.real_marker(value);
        api.observe(&marker);
",
        false,
    ),
    (
        "copied_real_equality_finite",
        r"
        let marker = await api.real_marker(1.0);
        api.observe(&marker);
",
        true,
    ),
    (
        "copied_real_alias_equality_unknown",
        r"
        let value: r64 = if choice { yield 1.0; } else { yield 0.0 / 0.0; };
        let marker = await api.real_pair(value, value);
        api.observe(&marker);
",
        false,
    ),
];

fn check_deferred_completion(imported: bool, api: &str, cases: &[(&str, &str, bool)]) {
    let provider = super::matrix::provider("deferred-focused", api);

    check_cases(&provider, imported, cases);
}

fn check_cases(provider: &crate::Compilation, imported: bool, cases: &[(&str, &str, bool)]) {
    let mut failures = Vec::new();

    for (name, body, valid) in cases {
        let _phase = super::matrix::Phase::start("case", name, imported);

        let source = format!(
            r"
            module app;

            using api;

            async func caller(pos choice: bool)
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
            crate::test_support::with_fixture_caller(provider, &source)
        };

        let checked = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, "caller"))
            .expect("matrix caller must lower");

        let diagnostics = checked.diagnostics();

        if matches!(*name, "preformed_task_fabricated_after_completion" | "same_site_independent_second") {
            assert!(diagnostics.has_errors(), "fabricated authority must be rejected, name={name}, imported={imported}");

            for diagnostic in diagnostics.iter() {
                assert_eq!(diagnostic.kind(), bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);

                let span = diagnostic.primary_span().expect("rejected operation retains its source location");
                let document = consumer.source(span.source_id()).expect("diagnostic source belongs to the consumer");
                let start = usize::try_from(span.start().bytes()).expect("fixture offset fits the host");
                let end = usize::try_from(span.end().bytes()).expect("fixture offset fits the host");
                let call = document.text().find("observe(&value)").expect("negative control retains its observe call");

                assert!(start >= call && end <= call + "observe(&value)".len(),
                    "negative control must fail at observation, name={name}, imported={imported}, span={span:?}");
            }
        }

        if *name == "same_site_loop" && !diagnostics.has_errors() {
            assert!(crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller")
                .normal_completion(), "valid deferred loop must retain its continuation, imported={imported}");
        }

        if diagnostics.has_errors() == *valid {
            failures.push(format!(
                "{name}, imported={imported}, valid={valid}: {}", super::fixtures::diagnostic_messages(diagnostics)
            ));
        } else if !valid
            && !diagnostics.iter().all(|diagnostic| {
                matches!(
                    diagnostic.kind(),
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven
                        | bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency
                        | bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow
                        | bray_diagnostics::DiagnosticKind::CheckingTrustedWitnessTransferNotProven
                )
            })
        {
            failures.push(format!("unexpected rejection for {name}: {}", super::fixtures::diagnostic_messages(diagnostics)));
        }
    }

    assert!(failures.is_empty(), "deferred cases completed: imported={imported}, count={}\n{}", cases.len(), failures.join("\n"));
}

fn check_matrix_catalogue(names: &[&str]) {
    assert_eq!(CASES.iter().map(|case| case.0.to_ascii_lowercase()).collect::<Vec<_>>(), names);
}

fn check_matrix_case(imported: bool, index: usize, expected: &str) {
    static PROVIDER: std::sync::LazyLock<crate::Compilation> = std::sync::LazyLock::new(|| super::matrix::provider("deferred", API));

    let wait = super::matrix::Phase::start("provider-wait", "deferred", imported);
    let provider = &*PROVIDER;

    drop(wait);
    assert_eq!(CASES[index].0.to_ascii_lowercase(), expected, "matrix registration must match its row");
    check_cases(provider, imported, &CASES[index..index + 1]);
}

super::matrix::cases! {
    formal_tuple_task_resumes_nested_carrier = 0,
    formal_array_task_resumes_nested_carrier = 1,
    formal_tuple_rejects_another_owners_carrier = 2,
    implicit_nullable_task_retains_frame = 3,
    staged_nullable_task_retains_adapted_frame = 4,
    union_future_retains_capture_identity = 5,
    union_future_retains_live_reference = 6,
    task_returns_nullable_future = 7,
    task_returns_union_future = 8,
    union_task_input_keeps_authority_private = 9,
    nullable_task_result_keeps_physical_address_bounds = 10,
    nullable_task_result_keeps_independent_evidence = 11,
    task_normal_or_diverging_producer = 12,
    task_opaque_body_mutation_retires_input_evidence = 13,
    returned_task_in_array_retains_frame = 14,
    forwarded_task_retains_frame = 15,
    returned_task_in_product_retains_frame = 16,
    task_returns_inactive_future = 17,
    task_returns_observation_future = 18,
    forwarded_task_cannot_create_witness = 19,
    nested_task_cannot_create_witness = 20,
    normal_reference_releases_frame_borrow = 21,
    task_reference_releases_frame_borrow = 22,
    opaque_await_preserves_report_borrow = 23,
    task_known_result_after_opaque = 24,
    task_opaque_result_after_opaque = 25,
    opaque_frame_borrow_released = 26,
    task_pending_permission_unavailable = 27,
    task_unrelated_join_permission_unavailable = 28,
    task_join_opaque_reference_live = 29,
    task_join_opaque_reference_mutation = 30,
    task_cancel_opaque_reference_live = 31,
    task_cancel_opaque_reference_mutation = 32,
    task_join_reference_escape = 33,
    task_join_reference_mutation = 34,
    task_join_reference_live = 35,
    task_join_reference_match_escape = 36,
    task_join_opaque_reference_escape = 37,
    task_cancel_reference_escape = 38,
    task_cancel_reference_mutation = 39,
    task_cancel_reference_live = 40,
    task_cancel_reference_match_escape = 41,
    task_cancel_opaque_reference_escape = 42,
    task_forwarded_reference_escape = 43,
    task_forwarded_reference_live = 44,
    preformed_future_selected_after_completion = 45,
    preformed_future_fabricated_after_completion = 46,
    opaque_future_selected_after_completion = 47,
    opaque_future_fabricated_after_completion = 48,
    preformed_task_selected_after_completion = 49,
    preformed_task_fabricated_after_completion = 50,
    opaque_task_selected_after_completion = 51,
    opaque_task_fabricated_after_completion = 52,
    returned_task_outcome = 53,
    returned_task_outcome_fabricated = 54,
    unknown_alternative = 55,
    unknown_refined = 56,
    unknown_selected = 57,
    unknown_mutated_selector = 58,
    copied_guard_normal = 59,
    copied_guard_partial = 60,
    copied_guard_inactive = 61,
    direct = 62,
    alias = 63,
    distinct_sites = 64,
    distinct_declarations = 65,
    both_fabricated = 66,
    left_fabricated = 67,
    right_fabricated = 68,
    known_left = 69,
    known_right = 70,
    refined_left = 71,
    refined_right = 72,
    selected_fabrication = 73,
    two_live = 74,
    two_live_independent_fabrication = 75,
    identity_helper = 76,
    helper_choice = 77,
    helper_choice_fabrication = 78,
    dormant = 79,
    normal_or_diverging = 80,
    only_diverging = 81,
    diverging_or_fabrication = 82,
    mutated_selector = 83,
    mutated_selector_fabrication = 84,
    frame_borrow_ends = 85,
    address_live = 86,
    address_expired = 87,
    independent_address_marker = 88,
    borrow_live = 89,
    borrow_expired = 90,
    same_site_loop = 91,
    same_site_fabrication = 92,
    tuple_owners = 93,
    array_owners = 94,
    boxed_future = 95,
    boxed_capture = 96,
    nullable_owner = 97,
    swap_both_valid = 98,
    swap_independent = 99,
    swap_fabrication = 100,
    inactive_partial = 101,
    active_partial = 102,
    dormant_partial = 103,
    captured_field_fact = 104,
    task_direct = 105,
    task_selected_future = 106,
    task_cancel_completion = 107,
    task_cancel_fabrication = 108,
    task_alias = 109,
    task_frame_borrow_ends = 110,
    task_unknown_alternative = 111,
    task_partial_transfer = 112,
    owned_body_mutation = 113,
    selected_body_mutation = 114,
    task_owned_body_mutation = 115,
    task_owned_state = 116,
    task_address_live = 117,
    task_address_expired = 118,
    task_independent_address_marker = 119,
    task_completed_permission = 120,
    nullable_completed_permission = 121,
    nullable_fresh_permission = 122,
    nullable_dropped_permission = 123,
    task_unobserved_outcome_permission = 124,
    task_dropped_permission = 125,
    task_active_discarded_permission = 126,
    task_discarded_permission = 127,
    nullable_address_live = 128,
    nullable_address_expired = 129,
    nullable_independent_marker = 130,
    refined_owned_body_effects = 131,
    task_refined_owned_body_effects = 132,
    task_selected_owned_body_mutation = 133,
    task_unknown_refined = 134,
    task_fabrication = 135,
    captured_guard_refined = 136,
    captured_guard_unselected = 137,
    captured_guard_mutated = 138,
    same_site_copied_inactive_guard = 139,
    same_site_copied_active_guard = 140,
    same_site_numeric_active_guard = 141,
    same_site_numeric_inactive_guard = 142,
    same_site_independent_first = 143,
    same_site_independent_second = 144,
    copied_real_equality_unknown = 145,
    copied_real_equality_finite = 146,
    copied_real_alias_equality_unknown = 147,
}

#[test]
fn private_formal_task_admission_preserves_only_its_carrier() {
    let api = r"
    trusted module api;

    struct Marker {}

    struct Box<T>
    {
        value: T;
    }

    trusted predicate held(owner: &Marker);

    trusted func make() -> Marker
        ensures(trusted held(owner = &result))
    {
        return Marker {};
    }

    func observe(pos value: &Marker)
        requires(trusted held(owner = value))
    {
    }

    async func pass<T>(pos value: T) -> T
    {
        return value;
    }

    async func formal_tuple(pos input: (Marker, u32), pos cancel: bool) -> (Marker, u32)
        requires(trusted held(owner = &input.0))
    {
        let wrapper: Box<(Marker, u32)> = { value = input };
        let task = pass<Box<(Marker, u32)>>(wrapper).start();

        let value = if cancel
        {
            yield try await task.cancel();
        }
        else
        {
            yield try await task.join();
        };

        observe(&value.value.0);
        return value.value;
    }

    async func formal_array(pos input: [Marker; 2], pos cancel: bool) -> [Marker; 2]
        requires(trusted held(owner = &input[1]))
    {
        let task = pass<[Marker; 2]>(input).start();

        let value = if cancel
        {
            yield try await task.cancel();
        }
        else
        {
            yield try await task.join();
        };

        observe(&value[1]);
        return value;
    }

";

    let cases = [
        ("tuple_owned", r"let value = await api.formal_tuple((api.make(), 0), choice); api.observe(&value.0);", true),
        ("array_owned", r"let input: [api.Marker; 2] = [api.Marker {}, api.make()]; let value = await api.formal_array(input, choice); api.observe(&value[1]);", true),
        ("tuple_fabricated", r"let unrelated = api.make(); let value = await api.formal_tuple((api.Marker {}, 0), choice); api.observe(&value.0);", false),
        ("array_fabricated", r"let unrelated = api.make(); let input: [api.Marker; 2] = [api.Marker {}, api.Marker {}]; let value = await api.formal_array(input, choice); api.observe(&value[1]);", false),
    ];

    for imported in [false, true] {
        check_deferred_completion(imported, api, &cases);
    }
}
