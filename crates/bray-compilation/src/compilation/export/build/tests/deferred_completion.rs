use super::fixtures::execution_consumer;

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

    struct StateIncident
    {
        state: &State;
    }

    struct ReportOnExit
    {
        state: &State;

        enter() -> &State
        {
            return self.state;
        }

        exit(pos state: &State) -> Result<unit, StateIncident>
        {
            return Error(StateIncident { state = state });
        }
    }

    async func retain_state_in_report(pos state: &State, pos cell: &Cell) -> Result<(&Cell), bool>
    {
        let reporter = ReportOnExit { state = state };

        _ = with loan = reporter
        {
            return Ok(cell);
        };

        return Ok(cell);
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

    async func fresh_result() -> Marker
    {
        return make();
    }

    func returned_fresh_frame() -> Future<Marker>
    {
        return fresh_result();
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
        r"let pending = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.only<api.Marker>(api.Marker {}, false); }; let task = pending.start(); let value = try await task.join(); api.observe(&value);",
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
        r"
            let cell: api.Cell = { value = 1 };

            let result =
            {
                let state: api.State = { count = 0 };

                let pending = if choice
                {
                    yield api.retain_state_in_report(&state, &cell);
                }
                else
                {
                    yield api.pass<Result<(&api.Cell), bool>>(Ok(&cell));
                };

                yield await pending;
            };

            _ = result;
        ",
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
        r"let mut state: api.State = { count = 0 }; let pending = if choice { yield api.touch<api.Marker>(api.make(), &mut state); } else { yield api.opaque(api.Marker {}); }; let value = await pending; state.count += 1; if choice { api.observe(&value); }",
        true,
    ),
    (
        "task_pending_permission_unavailable",
        r"let permit = api.permission(); let id = permit.id; let pending = if choice { yield api.pass<api.Permit>(permit); } else { yield api.other<api.Permit>(api.Permit { id = id }); }; let task = pending.start(); if choice { api.inspect_permission(id); }",
        false,
    ),
    (
        "task_unrelated_join_permission_unavailable",
        r"let permit = api.permission(); let id = permit.id; let task = api.pass<api.Permit>(permit).start(); let unrelated = if choice { yield api.pass<api.Marker>(api.make()); } else { yield api.other<api.Marker>(api.Marker {}); }; let value = try await unrelated.start().join(); if choice { api.observe(&value); api.inspect_permission(id); }",
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
    (
        "returned_fresh_carrier",
        r"let value = await api.returned_fresh_frame(); api.observe(&value);",
        true,
    ),
    (
        "same_site_nested_frames",
        r"
        let mut first: Future<Future<api.Marker>>? = none;
        let mut second: Future<Future<api.Marker>>? = none;
        let mut index: u32 = 0;
        while index < 2
        {
            let pending = api.pass<Future<api.Marker>>(api.pass<api.Marker>(api.make()));
            if index == 0 { first = pending; } else { second = pending; }
            index += 1;
        }
        match consume first { case ?pending { let inner = await pending; let value = await inner; api.observe(&value); } case none {} }
        match consume second { case ?pending { let inner = await pending; let value = await inner; api.observe(&value); } case none {} }
    ",
        true,
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

        let rejection_site = match *name {
            "preformed_task_fabricated_after_completion" | "same_site_independent_second" => Some("observe(&value)"),
            "nullable_fresh_permission" | "nullable_dropped_permission" | "task_discarded_permission"
            | "task_unrelated_join_permission_unavailable" => Some("inspect_permission(id)"),
            _ => None,
        };

        if let Some(rejection_site) = rejection_site {
            assert!(diagnostics.has_errors(), "fabricated authority must be rejected, name={name}, imported={imported}");

            for diagnostic in diagnostics.iter() {
                assert_eq!(diagnostic.kind(), bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);

                let span = diagnostic.primary_span().expect("rejected operation retains its source location");
                let document = consumer.source(span.source_id()).expect("diagnostic source belongs to the consumer");
                let start = usize::try_from(span.start().bytes()).expect("fixture offset fits the host");
                let end = usize::try_from(span.end().bytes()).expect("fixture offset fits the host");
                let call = document.text().rfind(rejection_site).expect("negative control retains its checked operation");

                assert!(start >= call && end <= call + rejection_site.len(),
                    "negative control must fail at observation, name={name}, imported={imported}, span={span:?}");
            }
        }

        if *name == "opaque_await_preserves_report_borrow" {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }

        let normal_completion = match *name {
            "dormant" | "dormant_partial" | "same_site_loop" => Some(true),
            "only_diverging" => Some(false),
            _ => None,
        };

        if let Some(expected) = normal_completion
            && !diagnostics.has_errors()
        {
            let dependencies = crate::compilation::result_dependencies::source_function_dependencies(&consumer, "caller");

            assert_eq!(dependencies.normal_completion(), expected, "deferred case {name}, imported={imported}");
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

#[test]
fn returned_frame_contributes_its_body_only_when_executed() {
    let provider = super::matrix::provider("returned-frame-behavior", r"
        module api;

        struct State { ready: bool; }
        async func child(pos state: &State) -> bool { return state.ready; }
        async func replaced() -> bool { return false; }
        func factory(pos state: &State) -> Future<bool> { return child(state); }
    ");

    let source = r"
        module app;
        using api;

        async func awaited()
        {
            let state = api.State { ready = true };
            let pending = api.factory(&state);
            let moved = pending;
            _ = await moved;
        }
        async func started()
        {
            let state = api.State { ready = true };
            let pending = api.factory(&state);
            let moved = pending;
            _ = await moved.start().join();
        }
        async func discarded()
        {
            let state = api.State { ready = false };
            _ = api.factory(&state);
        }
        async func replaced()
        {
            let state = api.State { ready = true };
            let mut pending = api.replaced();
            pending = api.factory(&state);
            _ = await pending;
        }
    ";

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."))
        } else {
            crate::test_support::with_fixture_caller(&provider, source)
        };

        let cancellation = crate::CancellationToken::new();
        let binding = consumer.binding_context(&cancellation).expect("fixture declarations bind");

        for (name, expected) in [("awaited", &["child"][..]), ("started", &["child"][..]), ("discarded", &[][..]), ("replaced", &["child"][..])] {
            let key = crate::test_support::source_module_function_body_key(&consumer, &["app"], name);
            let flow = consumer.storage_flow(key.clone()).expect("the frame has checked storage flow");
            let lowered = consumer.lowered_unit(key).expect("the frame use reaches lowering");

            assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "{name}, imported={imported}: {lowered:#?}");

            let called = flow.value().deferred_calls().map(|call| {
                let declaration = call.target().declaration().expect("selected producer is a declaration");

                binding.member_name(declaration.symbol()).expect("selected declaration resolves")
                    .expect("selected producer has a name").as_str()
            }).filter(|name| matches!(*name, "child" | "replaced")).collect::<Vec<_>>();

            assert_eq!(called, expected, "{name}, imported={imported}");
        }
    }
}

#[test]
fn widened_frame_producers_keep_only_authority_from_normal_completions() {
    let provider = super::matrix::provider("deferred-widening", API);
    let mut selections = String::new();

    for index in 0..9 {
        use std::fmt::Write;

        write!(selections, "if selector == {index} {{ yield api.pass<api.Marker>(api.make()); }} else ")
            .expect("writing fixture selections cannot fail");
    }

    for imported in [false, true] {
        for (observation, last_completes) in [
            ("await pending", false),
            ("await pending", true),
            ("try await pending.start().join()", false),
            ("try await pending.start().join()", true),
        ] {
            let source = format!(r"
                module app;
                using api;

                async func caller(pos selector: u8)
                {{
                    let pending = {selections} {{ yield api.only<api.Marker>(api.Marker {{}}, {last_completes}); }};
                    let value = {observation};
                    api.observe(&value);
                }}
            ");

            let consumer = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                crate::test_support::with_fixture_caller(&provider, &source)
            };

            let result = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, "caller"))
                .expect("widened frame producer reaches lowering");

            if last_completes {
                bray_testing::assert_goal_state_diagnostic_kind(result.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven);
            } else {
                assert!(result.value().is_some() && !result.diagnostics().has_errors(), "{result:#?}");
            }
        }
    }
}

#[test]
fn widened_task_observation_keeps_completed_report_dependencies() {
    let provider = super::matrix::provider("task-observation-reports", API);
    let mut selections = String::new();

    for index in 0..9 {
        use std::fmt::Write;

        write!(selections, "if selector == {index} {{ yield api.retain_state_in_report(state, cell); }} else ")
            .expect("writing fixture selections cannot fail");
    }

    let source = format!(r"
        module app;
        using api;

        async func caller(pos selector: u8, pos state: &api.State, pos cell: &api.Cell)
        {{
            let pending: Future<Result<(&api.Cell), bool>> = {selections} {{
                yield api.only<Result<(&api.Cell), bool>>(Ok(cell), false);
            }};
            let task = pending.start();
            _ = try await task.join();
        }}
    ");

    for imported in [false, true] {
        use bray_symbols::{DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal};

        let consumer = if imported {
            execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."))
        } else {
            crate::test_support::with_fixture_caller(&provider, &source)
        };

        let name = "caller";

        let lowered = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, name))
            .expect("widened task observer lowers");

        assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "imported={imported}: {lowered:#?}");

        let expected = DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(1))),
            DependencyRequirementKind::ValueDependencies,
        );

        let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

        assert_eq!(summary.completion_reports().as_deref(), Some(&[expected][..]), "imported={imported}");
    }
}

#[test]
fn fully_widened_task_result_retains_the_possible_panic_payload() {
    use bray_symbols::{DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal};
    use std::fmt::Write;

    let provider = super::matrix::provider("fully-widened-task-report", r#"
        module api;
        struct Incident { value: &bool; }
        struct Resource
        {
            value: &bool;
            enter() -> &bool { return self.value; }
            exit(pos lease: &bool) -> Result<unit, Incident> { return Error(Incident { value = lease }); }
        }
        async func run(pos should_panic: bool, pos left: &bool, pos right: &bool) -> u8
        {
            if should_panic
            {
                let resource = Resource { value = left };
                _ = with _ = resource { panic("child"); };
            }
            return 7;
        }
    "#);

    let mut choices = String::new();

    for index in 0..7 {
        write!(choices, "if selector == {index} {{ yield {index}; }} else ").expect("writing fixture branches succeeds");
    }

    for (arguments, escapes) in [("&local, right", true), ("right, &local", false)] {
        let source = format!(r"
            module app;
            using api;
            async func observe(pos selector: u8, pos branch: bool, pos right: &bool) -> (u8, RunResult<u8>)
            {{
                let prior: u8 = {choices} {{ yield 7; }};
                let local = selector == 4;
                return (prior, await api.run(branch, {arguments}).start().join());
            }}
        ");

        for imported in [false, true] {
            let consumer = if imported {
                execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                    .replace("api.", "example.package.api."))
            } else {
                crate::test_support::with_fixture_caller(&provider, &source)
            };

            let flow = consumer.storage_flow(crate::test_support::source_function_body_key(&consumer, "observe"))
                .expect("the widened observation reaches storage checking");

            if escapes {
                bray_testing::assert_goal_state_diagnostic_kind(flow.diagnostics(),
                    bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency);
            } else {
                assert!(!flow.diagnostics().has_errors(), "imported={imported}: {:?}", flow.diagnostics());

                let expected = DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(2))),
                    DependencyRequirementKind::ValueDependencies,
                );

                assert!(flow.value().dependencies().requirements().contains(&expected), "imported={imported}");
            }
        }
    }
}

#[test]
fn task_panic_report_moves_from_a_value_to_its_propagation_boundary() {
    use bray_symbols::{DependencyFailureKind, DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot, SymbolOrdinal};

    let provider = super::matrix::provider("task-panic-report-propagation", r#"
        module api;
        struct Incident { value: &bool; }
        struct Resource
        {
            value: &bool;
            enter() -> &bool { return self.value; }
            exit(pos lease: &bool) -> Result<unit, Incident> { return Error(Incident { value = lease }); }
        }
        async func stop(pos choose: bool, pos left: &bool, pos right: &bool)
        {
            if choose
            {
                let resource = Resource { value = left };
                _ = with _ = resource { panic("left"); };
            }
            let resource = Resource { value = right };
            _ = with _ = resource { panic("right"); };
        }

        async func observe(pos choose: bool, pos left: &bool, pos right: &bool) -> RunResult<unit>
        {
            return await stop(choose, left, right).start().join();
        }

        async func observe_both(pos left: &bool, pos right: &bool) -> [RunResult<unit>; 2]
        {
            return [await observe(true, left, right), await observe(false, left, right)];
        }
    "#);

    let source = r"
        module app;
        using api;
        async func return_left(pos left: &bool, pos right: &bool) -> RunResult<unit>
        {
            return await api.stop(true, left, right).start().join();
        }
        async func propagate_right(pos left: &bool, pos right: &bool)
        {
            let observed = await api.stop(false, left, right).start().join();
            try observed;
        }
        async func catch_left(pos left: &bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            return catch { try await api.stop(true, left, right).start().join(); };
        }
        async func escaping(pos choose: bool, pos right: &bool)
        {
            let local = choose;
            let observed = await api.stop(true, &local, right).start().join();
            try observed;
        }
        async func independent(pos choose: bool, pos right: &bool)
        {
            let local = choose;
            let observed = await api.stop(false, &local, right).start().join();
            try observed;
        }
        async func escaping_catch(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let observed = await api.stop(true, &local, right).start().join();
            let caught = catch { try observed; };
            return caught;
        }
        async func independent_catch(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let observed = await api.stop(false, &local, right).start().join();
            let caught = catch { try observed; };
            return caught;
        }
        async func escaping_return(pos choose: bool, pos right: &bool) -> RunResult<unit>
        {
            let local = choose;
            return await api.stop(true, &local, right).start().join();
        }
        async func independent_return(pos choose: bool, pos right: &bool) -> RunResult<unit>
        {
            let local = choose;
            return await api.stop(false, &local, right).start().join();
        }
        async func escaping_forwarded(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let observed = await api.observe(true, &local, right);
            return catch { try observed; };
        }
        async func independent_forwarded(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let observed = await api.observe(false, &local, right);
            return catch { try observed; };
        }
        async func escaping_selected_late(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let pending = api.observe(choose, &local, right);
            if choose
            {
                let observed = await pending;
                return catch { try observed; };
            }
            return catch {};
        }
        async func independent_selected_late(pos choose: bool, pos right: &bool) -> Result<unit, PanicReport>
        {
            let local = choose;
            let pending = api.observe(choose, &local, right);
            if !choose
            {
                let observed = await pending;
                return catch { try observed; };
            }
            return catch {};
        }
        async func escaping_wrapped(pos choose: bool, pos right: &bool) -> Result<RunResult<unit>, PanicReport>
        {
            let local = choose;
            let observed = await api.stop(true, &local, right).start().join();
            return catch (observed);
        }
        async func independent_wrapped(pos choose: bool, pos right: &bool) -> Result<RunResult<unit>, PanicReport>
        {
            let local = choose;
            let observed = await api.stop(false, &local, right).start().join();
            return catch (observed);
        }
        async func discarded(pos choose: bool, pos right: &bool)
        {
            let local = choose;
            _ = await api.stop(true, &local, right).start().join();
        }
        async func forward_both(pos left: &bool, pos right: &bool) -> [RunResult<unit>; 2]
        {
            return await api.observe_both(left, right);
        }
    ";

    for imported in [false, true] {
        let consumer = if imported {
            execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."))
        } else {
            crate::test_support::with_fixture_caller(&provider, source)
        };

        let dependency = |ordinal| DependencyRequirement::direct(
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(ordinal))),
            DependencyRequirementKind::ValueDependencies,
        );

        for name in ["escaping", "escaping_catch", "escaping_return", "escaping_wrapped", "escaping_forwarded", "escaping_selected_late"] {
            let escaping = consumer.storage_flow(crate::test_support::source_function_body_key(&consumer, name))
                .expect("escaping report reaches storage checking");

            bray_testing::assert_goal_state_diagnostic_kind(escaping.diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency);
        }

        for name in ["independent", "independent_catch", "independent_return", "independent_wrapped", "independent_forwarded", "independent_selected_late", "discarded"] {
            let independent = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, name))
                .expect("independently backed report reaches lowering");

            assert!(independent.value().is_some() && !independent.diagnostics().has_errors(),
                "{name}, imported={imported}: {independent:#?}");
        }

        for name in ["return_left", "propagate_right", "catch_left"] {
            let lowered = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, name))
                .expect("task report reaches lowering");

            assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "{name}, imported={imported}: {lowered:#?}");

            let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, name);

            assert!(summary.completion_reports().is_none(), "{name}, imported={imported}: {summary:#?}");

            if name == "propagate_right" {
                let rows = summary.failure_outcomes().expect("propagated report has a panic outcome");

                let borrowed = rows.iter().filter(|row| row.kind == Some(DependencyFailureKind::Panic))
                    .filter_map(|row| row.report.as_deref()).filter(|report| !report.is_empty()).collect::<Vec<_>>();

                assert!(!borrowed.is_empty(), "{name}, imported={imported}: {rows:#?}");

                assert!(borrowed.iter().all(|report| *report == [dependency(1)]),
                    "{name}, imported={imported}: {borrowed:#?}");
            } else {
                assert_eq!(summary.requirements(), &[dependency(0)], "{name}, imported={imported}");
            }
        }

        let lowered = consumer.lowered_unit(crate::test_support::source_function_body_key(&consumer, "forward_both"))
            .expect("both array report payloads reach lowering");

        assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "imported={imported}: {lowered:#?}");

        let summary = crate::compilation::result_dependencies::source_function_dependencies(&consumer, "forward_both");
        let values = consumer.semantic_value_store().expect("array selectors belong to the consumer's semantic store");
        let mut carried = std::collections::BTreeSet::new();

        for payload in summary.captured_values().iter().flat_map(|row| &*row.report_payloads) {
            let [bray_symbols::DependencyProjection::Element(term), bray_symbols::DependencyProjection::UnionPayloadField(_)] =
                payload.destination.projections() else { panic!("report retains its precise array component: {payload:?}"); };

            let index = values.constant_term_integer(*term).and_then(|value| value.to_u64())
                .and_then(|index| u32::try_from(index).ok()).expect("array report selector is a checked ordinal");

            if !payload.requirements.is_empty() {
                assert_eq!(payload.requirements.as_ref(), &[dependency(index)], "imported={imported}: {payload:?}");

                carried.insert(index);
            }
        }

        assert_eq!(carried, [0, 1].into(), "imported={imported}: {summary:#?}");
    }
}

#[test]
fn task_observation_keeps_both_failure_kinds_when_panic_rows_fill_the_budget() {
    use std::fmt::Write;

    let mut fields = String::new();
    let mut panics = String::new();

    for index in 0..8 {
        write!(fields, "mut slot{index}: bool; ").expect("writing fixture fields cannot fail");

        write!(panics, "if selector == {index} {{ target.slot{index} = true; panic(\"stop\"); }} ")
            .expect("writing fixture branches cannot fail");
    }

    let provider = super::matrix::provider("task-result-kind-budget", &format!(r"
        module api;
        struct Cell {{ {fields} }}
        async func stop(pos selector: u8, pos target: &mut Cell) {{ {panics} loop {{}} }}
    "));

    let source = r"
        module app;
        using api;
        async func caller(pos selector: u8, pos target: &mut api.Cell)
        {
            _ = await api.stop(selector, target).start().join();
        }
    ";

    for imported in [false, true] {
        use bray_bound_tree::{BoundExpression, StorageTaskOutcome};
        use bray_symbols::DependencyFailureKind;

        let consumer = if imported {
            execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."))
        } else {
            crate::test_support::with_fixture_caller(&provider, source)
        };

        let key = crate::test_support::source_function_body_key(&consumer, "caller");
        let unit = consumer.bound_unit(key.clone()).expect("task observer binds");
        let flow = consumer.storage_flow(key.clone()).expect("task observer checks");
        let lowered = consumer.lowered_unit(key).expect("task observer lowers");

        assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "imported={imported}: {lowered:#?}");

        let await_site = unit.value().tree().expressions().find_map(|(id, expression)| {
            matches!(expression, BoundExpression::Await(_)).then_some(id.into())
        }).expect("caller awaits the task observation");

        let outcomes = flow.value().task_outcome_mutations(await_site).map(|(outcome, _)| outcome).collect::<Vec<_>>();

        assert!(outcomes.contains(&StorageTaskOutcome::Abnormal {
            depth: 0,
            kind: DependencyFailureKind::Panic,
            row: None,
        }), "panic rows share one group, imported={imported}: {outcomes:?}");

        assert!(outcomes.iter().any(|outcome| matches!(outcome, StorageTaskOutcome::Abnormal {
            depth: 0,
            kind: DependencyFailureKind::Cancellation,
            ..
        })), "the cancellation group remains reachable, imported={imported}: {outcomes:?}");

        assert!(!outcomes.iter().any(|outcome| matches!(outcome, StorageTaskOutcome::Completed { .. } | StorageTaskOutcome::Widened)),
            "the noncompleting body publishes only the two abnormal kinds, imported={imported}: {outcomes:?}");
    }
}

#[test]
fn task_observation_keeps_the_selected_panic_row() {
    let provider = super::matrix::provider("task-panic-rows", r#"
        module api;
        struct Cell { mut value: bool; }

        async func stop(pos choose: bool, pos left: &mut Cell, pos right: &mut Cell)
        {
            if choose { left.value = false; panic("left"); }
            right.value = false;
            panic("right");
        }
    "#);

    let source = r"
        module app;
        using api;

        async func selected(pos choose: bool, pos left: &mut api.Cell, pos right: &mut api.Cell)
        {
            let task = api.stop(choose, left, right).start();
            if choose { _ = await task.join(); }
            else { _ = await task.join(); }
        }

        async func both(pos choose: bool, pos left: &mut api.Cell, pos right: &mut api.Cell)
        {
            let task = api.stop(choose, left, right).start();
            _ = await task.join();
        }
    ";

    for imported in [false, true] {
        use bray_bound_tree::StorageTaskOutcome;

        let consumer = if imported {
            execution_consumer(&provider, &source.replace("using api;", "using example.package.api;")
                .replace("api.", "example.package.api."))
        } else {
            crate::test_support::with_fixture_caller(&provider, source)
        };

        let panic_rows = |name| {
            let key = crate::test_support::source_function_body_key(&consumer, name);
            let unit = consumer.bound_unit(key.clone()).expect("task observer binds");
            let flow = consumer.storage_flow(key.clone()).expect("task observer checks");
            let lowered = consumer.lowered_unit(key).expect("task observer lowers");

            assert!(lowered.value().is_some() && !lowered.diagnostics().has_errors(), "{lowered:#?}");

            unit.value().tree().expressions().filter_map(|(expression, node)| {
                let bray_bound_tree::BoundExpression::Await(_) = node else { return None; };

                let mut rows = Vec::new();

                for (outcome, _) in flow.value().task_outcome_mutations(expression.into()) {
                    match outcome {
                        StorageTaskOutcome::Abnormal { depth: 0, kind: bray_symbols::DependencyFailureKind::Panic, row } => {
                            rows.push(row.expect("a reached panic retains its body row"));
                        }
                        StorageTaskOutcome::Completed { .. } => panic!("the panicking task has no normal output"),
                        _ => {}
                    }
                }

                Some(rows)
            }).collect::<Vec<_>>()
        };

        let selected = panic_rows("selected");

        let [left, right] = selected.as_slice() else { panic!("each guard arm awaits once: {selected:?}"); };

        let ([left], [right]) = (left.as_slice(), right.as_slice()) else {
            panic!("each guard selects one body panic: {selected:?}");
        };

        assert_ne!(left, right);

        let mut expected = [*left, *right];

        expected.sort_unstable();

        assert_eq!(panic_rows("both"), [expected.to_vec()]);
    }
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
    tuple_owners = 92,
    array_owners = 93,
    boxed_future = 94,
    boxed_capture = 95,
    nullable_owner = 96,
    swap_both_valid = 97,
    swap_independent = 98,
    swap_fabrication = 99,
    inactive_partial = 100,
    active_partial = 101,
    dormant_partial = 102,
    captured_field_fact = 103,
    task_direct = 104,
    task_selected_future = 105,
    task_cancel_completion = 106,
    task_cancel_fabrication = 107,
    task_alias = 108,
    task_frame_borrow_ends = 109,
    task_unknown_alternative = 110,
    task_partial_transfer = 111,
    owned_body_mutation = 112,
    selected_body_mutation = 113,
    task_owned_body_mutation = 114,
    task_owned_state = 115,
    task_address_live = 116,
    task_address_expired = 117,
    task_independent_address_marker = 118,
    task_completed_permission = 119,
    nullable_completed_permission = 120,
    nullable_fresh_permission = 121,
    nullable_dropped_permission = 122,
    task_unobserved_outcome_permission = 123,
    task_dropped_permission = 124,
    task_active_discarded_permission = 125,
    task_discarded_permission = 126,
    nullable_address_live = 127,
    nullable_address_expired = 128,
    nullable_independent_marker = 129,
    refined_owned_body_effects = 130,
    task_refined_owned_body_effects = 131,
    task_selected_owned_body_mutation = 132,
    task_unknown_refined = 133,
    task_fabrication = 134,
    captured_guard_refined = 135,
    captured_guard_unselected = 136,
    captured_guard_mutated = 137,
    same_site_copied_inactive_guard = 138,
    same_site_copied_active_guard = 139,
    same_site_numeric_active_guard = 140,
    same_site_numeric_inactive_guard = 141,
    same_site_independent_first = 142,
    same_site_independent_second = 143,
    copied_real_equality_unknown = 144,
    copied_real_equality_finite = 145,
    copied_real_alias_equality_unknown = 146,
    returned_fresh_carrier = 147,
    same_site_nested_frames = 148,
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
