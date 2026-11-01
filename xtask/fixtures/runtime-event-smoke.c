// The bootstrap smoke host concatenates this after the task ABI declarations.
// It links only Bray implementation and native platform providers.
typedef struct { uintptr_t address; } EventReference;
typedef uint32_t (*EventWake)(uintptr_t, uint32_t);
typedef struct { uintptr_t context; EventWake call; } EventCallback;

extern uintptr_t bray_runtime_event_creation(uint64_t, EventReference*);
extern uint32_t bray_runtime_event_acquisition(uint64_t, uintptr_t, EventReference*);
extern void bray_runtime_event_release(const EventReference*);
extern uint32_t bray_runtime_event_observation(const EventReference*, uint64_t*);
extern uint32_t bray_runtime_event_registration(const EventReference*, uint64_t, const TaskReference*, const EventCallback*);
extern uint32_t bray_runtime_event_readiness(const TaskReference*);
extern uint32_t bray_runtime_event_withdrawal(const TaskReference*);
extern uint32_t bray_runtime_event_notification(const EventReference*, uint32_t, uint64_t*, uint32_t*);
extern uint32_t bray_runtime_event_signal(uintptr_t);
extern uint32_t bray_runtime_event_destruction(uint64_t, uintptr_t);
extern void bray_runtime_event_clear(uint64_t);

static int check_event_cold_shutdown(void)
{
    EventReference event = {0};
    uint64_t runtime = bray_runtime_task_control_creation();
    uintptr_t identity = bray_runtime_event_creation(runtime, &event);

    if (!identity || bray_runtime_structured_shutdown() != 4 ||
        bray_runtime_event_destruction(runtime, identity) != 0)
        return 225;

    bray_runtime_event_release(&event);

    if (bray_runtime_structured_shutdown() != 0 || atomic_load(&live_allocations) != 0 ||
        bray_runtime_event_signal(identity) != 3)
        return 226;

    return 0;
}

typedef struct
{
    EventReference event;
    TaskReference target;
    uint64_t runtime;
    uintptr_t identity;
    unsigned wakes, releases;
    uint32_t failure;
    int destroy, resolved, released_event;
    atomic_bool entered, proceed;
    int block;
} EventContext;

static uint32_t event_wake(uintptr_t context, uint32_t notify)
{
    EventContext* value = (EventContext*)context;

    if (!notify)
    {
        ++value->releases;
        return 0;
    }

    ++value->wakes;
    uint64_t generation;

    // This reentrant read would deadlock under the event collection acquisition.
    if (!value->released_event)
        bray_runtime_event_observation(&value->event, &generation);

    if (value->destroy && bray_runtime_event_destruction(value->runtime, value->identity) != 0)
        abort();

    if (value->block)
    {
        atomic_store_explicit(&value->entered, 1, memory_order_release);

        while (!atomic_load_explicit(&value->proceed, memory_order_acquire))
        {
#ifdef _WIN32
            SwitchToThread();
#else
            sched_yield();
#endif
        }
    }

    TaskSnapshot snapshot;

    if (bray_runtime_task_control_snapshot(&value->target, &snapshot) != 0)
        abort();

    return value->failure;
}

typedef struct
{
    uintptr_t identity;
    uint32_t status;
    atomic_bool start;
} EventSignal;

#ifdef _WIN32
static DWORD WINAPI event_signal_thread(void* context)
#else
static void* event_signal_thread(void* context)
#endif
{
    EventSignal* signal = context;

    if (bray_runtime_current_native_thread_identity() != 0)
        abort();

    while (!atomic_load_explicit(&signal->start, memory_order_acquire))
    {
#ifdef _WIN32
        SwitchToThread();
#else
        sched_yield();
#endif
    }

    signal->status = bray_runtime_event_signal(signal->identity);

    if (bray_runtime_current_native_thread_identity() != 0)
        abort();

    return 0;
}

#ifdef _WIN32
typedef HANDLE EventThread;
#else
typedef pthread_t EventThread;
#endif

static EventThread start_event_signal(EventSignal* signal)
{
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, event_signal_thread, signal, 0, NULL);

    if (!thread)
        abort();
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, event_signal_thread, signal) != 0)
        abort();
#endif

    return thread;
}

static void join_event_signal(EventThread thread)
{
#ifdef _WIN32
    if (WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0)
        abort();

    CloseHandle(thread);
#else
    if (pthread_join(thread, NULL) != 0)
        abort();
#endif
}

static int prepare_event(EventContext* context, uint64_t runtime)
{
    *context = (EventContext){ .runtime = runtime };
    uint64_t task = bray_runtime_task_control_admission(runtime);

    context->identity = bray_runtime_event_creation(runtime, &context->event);

    return !task || !context->identity ||
        bray_runtime_task_control_retention(runtime, task, &context->target) != 0;
}

static void release_event_context(EventContext* context, int destroyed)
{
    if (!destroyed && bray_runtime_event_destruction(context->runtime, context->identity) != 0)
        abort();

    if ((!context->resolved && bray_runtime_task_control_destruction(&context->target) != 0) ||
        bray_runtime_task_control_reference_release(&context->target) != 0)
        abort();

    if (!context->released_event)
        bray_runtime_event_release(&context->event);
}

static int check_event_runtime(void)
{
    uint64_t runtime = bray_runtime_task_control_creation();
    EventContext context;

    if (!runtime || prepare_event(&context, runtime))
        return 201;

    size_t event_bytes = 0;
    lock_allocations();

    for (ReportAllocation* allocation = report_allocations; allocation; allocation = allocation->next)
    {
        if (allocation->memory == (void*)context.event.address)
            event_bytes = allocation->bytes;
    }

    unlock_allocations();

    printf("event storage_bytes=%zu reference_bytes=%zu callback_bytes=%zu\n",
        event_bytes, sizeof(EventReference), sizeof(EventCallback));

    EventReference rejected = {0};

    if (bray_runtime_event_acquisition(runtime + 1, context.identity, &rejected) != 3 ||
        bray_runtime_event_acquisition(0, context.identity, &rejected) != 3 ||
        bray_runtime_event_destruction(runtime + 1, context.identity) != 3 ||
        bray_runtime_event_signal(0) != 3 || bray_runtime_event_creation(0, &rejected) != 0)
        return 220;

    unsigned allocation_before = bray_allocation_probe_begin();
    uintptr_t denied = bray_runtime_event_creation(runtime, &rejected);
    unsigned allocation_failures = bray_allocation_probe_end(allocation_before);

    if (denied != 0 || allocation_failures != 1)
        return 221;

    // A stale observation commits immediately, without publishing membership.
    uint64_t generation = 0;
    uint32_t changed = 0;

    if (bray_runtime_event_notification(&context.event, 0, &generation, &changed) != 0 || generation != 1 || changed != 1 ||
        bray_runtime_event_registration(&context.event, 0, &context.target, &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
        context.wakes != 1 || context.releases != 1 || bray_runtime_event_readiness(&context.target) != 1 ||
        bray_runtime_event_withdrawal(&context.target) != 1)
        return 202;

    // Task admission has already backed every mandatory wait/delivery link.
    unsigned before = bray_allocation_probe_begin();

    for (unsigned index = 0; index < 2000; ++index)
    {
        if (bray_runtime_event_registration(&context.event, generation, &context.target, &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
            bray_runtime_event_readiness(&context.target) != 0)
            return 203;

        if (index % 2 == 0 && bray_runtime_event_withdrawal(&context.target) != 1)
            return 204;

        if (bray_runtime_event_notification(&context.event, 0, &generation, &changed) != 0 || changed != 1 ||
            bray_runtime_event_readiness(&context.target) != 1)
            return 205;
    }

    if (bray_allocation_probe_end(before) != 0 || context.wakes != 1001 || context.releases != 2001)
        return 206;

    release_event_context(&context, 0);

    // Registration and an unattached external signal have exactly one outcome.
    for (unsigned index = 0; index < 64; ++index)
    {
        if (prepare_event(&context, runtime))
            return 207;

        EventSignal signal = { .identity = context.identity };
        EventThread thread = start_event_signal(&signal);

        atomic_store_explicit(&signal.start, 1, memory_order_release);

        if (bray_runtime_event_registration(&context.event, 0, &context.target, &(EventCallback){ (uintptr_t)&context, event_wake }) != 0)
            return 208;

        join_event_signal(thread);

        if (signal.status != 0 || context.wakes != 1 || context.releases != 1)
            return 209;

        release_event_context(&context, 0);
    }

    // Inspect the internal C-layout storage through the actual retained owner.
    // This serial fixture is its sole user while preparing the exhaustion case.
    typedef struct
    {
        uint64_t generation;
        _Bool closed;
        uint64_t head, tail;
    } EventState;
    typedef struct
    {
        atomic_size_t references;
        EventState state;
    } EventStorage;

    if (prepare_event(&context, runtime))
        return 217;

    ((EventStorage*)context.event.address)->state.generation = UINT64_MAX;

    if (bray_runtime_event_registration(&context.event, UINT64_MAX, &context.target,
            &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
        bray_runtime_event_notification(&context.event, 0, &generation, &changed) != 8 ||
        generation != UINT64_MAX || changed != 1 || context.wakes != 1 || context.releases != 1 ||
        bray_runtime_event_observation(&context.event, &generation) != 1 ||
        bray_runtime_event_signal(context.identity) != 0)
        return 218;

    // Closure permits immediate delivery but never publishes another membership.
    if (bray_runtime_event_registration(&context.event, UINT64_MAX, &context.target,
            &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
        context.wakes != 2 || context.releases != 2)
        return 219;

    release_event_context(&context, 0);

    // A callback failure must still drain every committed descriptor.
    EventContext second;

    if (prepare_event(&context, runtime) || prepare_event(&second, runtime))
        return 215;

    context.failure = 4;

    if (bray_runtime_event_registration(&context.event, 0, &context.target, &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
        bray_runtime_event_registration(&context.event, 0, &second.target, &(EventCallback){ (uintptr_t)&second, event_wake }) != 0 ||
        bray_runtime_event_signal(context.identity) != 4 || context.wakes != 1 || second.wakes != 1 ||
        context.releases != 1 || second.releases != 1)
        return 216;

    release_event_context(&context, 0);
    release_event_context(&second, 0);

    // Once callback entry commits, withdrawal cannot revoke its retained owners.
    if (prepare_event(&context, runtime) || prepare_event(&second, runtime))
        return 210;

    context.block = 1;
    context.destroy = 1;

    if (bray_runtime_event_registration(&context.event, 0, &context.target, &(EventCallback){ (uintptr_t)&context, event_wake }) != 0 ||
        bray_runtime_event_registration(&context.event, 0, &second.target, &(EventCallback){ (uintptr_t)&second, event_wake }) != 0)
        return 211;

    EventSignal signal = { .identity = context.identity };
    EventThread thread = start_event_signal(&signal);

    atomic_store_explicit(&signal.start, 1, memory_order_release);

    while (!atomic_load_explicit(&context.entered, memory_order_acquire))
    {
#ifdef _WIN32
        SwitchToThread();
#else
        sched_yield();
#endif
    }

    // The membership was popped before entry, so withdrawal succeeds without
    // releasing the independently committed callback or its context.
    if (bray_runtime_event_readiness(&context.target) != 1 || bray_runtime_event_readiness(&second.target) != 0 ||
        bray_runtime_event_withdrawal(&context.target) != 1 || context.releases != 0 ||
        bray_runtime_event_signal(context.identity) != 3 || bray_runtime_event_withdrawal(&second.target) != 0 ||
        bray_runtime_task_control_destruction(&second.target) != 0)
        return 212;

    second.resolved = 1;

    atomic_store_explicit(&context.proceed, 1, memory_order_release);
    join_event_signal(thread);

    if (signal.status != 0 || context.wakes != 1 || context.releases != 1)
        return 213;

    if (bray_runtime_event_acquisition(runtime, context.identity, &rejected) != 3 ||
        bray_runtime_event_acquisition(runtime + 1, context.identity, &rejected) != 3)
        return 214;

    release_event_context(&context, 1);
    release_event_context(&second, 0);
    // Source alone retains the event through reentrant close and final disposal.
    if (prepare_event(&context, runtime))
        return 222;

    context.destroy = 1;

    if (bray_runtime_event_registration(&context.event, 0, &context.target,
            &(EventCallback){ (uintptr_t)&context, event_wake }) != 0)
        return 223;

    bray_runtime_event_release(&context.event);
    context.released_event = 1;
    before = bray_allocation_probe_begin();
    uint32_t final_status = bray_runtime_event_signal(context.identity);
    unsigned final_allocations = bray_allocation_probe_end(before);

    if (final_status != 0 || final_allocations != 0 || context.wakes != 1 || context.releases != 1)
        return 224;

    release_event_context(&context, 1);
    bray_runtime_event_clear(runtime);
    puts("event delivery: stale/register/withdraw/commit/unattached/close/denied-allocation passed");
    return 0;
}
