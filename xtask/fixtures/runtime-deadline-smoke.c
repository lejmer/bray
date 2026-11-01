// Concatenated after event callback and native platform declarations.
typedef struct { uintptr_t address; } SchedulerReference;
typedef struct { uintptr_t address; } DeadlineReference;

extern uint32_t bray_runtime_scheduler_creation(uint32_t, uint64_t, size_t, SchedulerReference*);
extern void bray_runtime_scheduler_destruction(const SchedulerReference*);
extern uint32_t bray_runtime_deadline_creation(
    const SchedulerReference*,
    size_t,
    DeadlineReference*
);
extern void bray_runtime_deadline_destruction(const DeadlineReference*);
extern uint32_t bray_runtime_deadline_registration(
    const DeadlineReference*,
    uint64_t,
    uint64_t,
    const EventCallback*,
    uint64_t*
);
extern uint32_t bray_runtime_deadline_withdrawal(const DeadlineReference*, uint64_t);
extern void bray_runtime_deadline_scope_withdrawal(const DeadlineReference*, uint64_t);
extern uint32_t bray_runtime_deadline_dispatch(const DeadlineReference*, uint64_t);
extern uint32_t bray_runtime_deadline_next(const DeadlineReference*, uint64_t*);
extern size_t bray_runtime_deadline_membership_count(const DeadlineReference*);
extern void bray_runtime_deadline_closure(const SchedulerReference*);

typedef struct
{
    DeadlineReference* deadlines;
    SchedulerReference* scheduler;
    const TaskReference* target;
    unsigned wakes, releases, ordinal;
    unsigned* sequence;
    uint32_t failure;
    int close, block;
    atomic_bool entered, proceed;
} DeadlineContext;

static uint32_t deadline_wake(uintptr_t context, uint32_t notify)
{
    DeadlineContext* value = (DeadlineContext*)context;

    // Both notification and release must reenter without a collection lock.
    bray_runtime_deadline_membership_count(value->deadlines);

    if (!notify)
    {
        ++value->releases;
        return 0;
    }

    ++value->wakes;

    if (
        value->target &&
        bray_runtime_task_control_cancellation(value->target, 1) != 3
    )
        abort();

    if (value->sequence)
        value->ordinal = ++*value->sequence;

    if (value->close)
        bray_runtime_deadline_closure(value->scheduler);

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

    return value->failure;
}

static uint32_t register_deadline(
    DeadlineReference* owner,
    uint64_t deadline,
    uint64_t scope,
    DeadlineContext* context,
    uint64_t* identity
)
{
    EventCallback callback =
    {
        (uintptr_t)context,
        deadline_wake
    };

    return bray_runtime_deadline_registration(owner, deadline, scope, &callback, identity);
}

#ifdef _WIN32
static DWORD WINAPI deadline_dispatch_thread(void* context)
#else
static void* deadline_dispatch_thread(void* context)
#endif
{
    DeadlineReference* owner = context;

    if (bray_runtime_deadline_dispatch(owner, UINT64_MAX) != 0)
        abort();

    return 0;
}

static int check_deadline_runtime(void)
{
    SchedulerReference scheduler = {0};
    DeadlineReference deadlines = {0};
    TaskReference target = {0};
    uint64_t runtime = bray_runtime_task_control_creation();
    uint64_t handle = bray_runtime_task_control_admission(runtime);

    if (
        !handle ||
        bray_runtime_task_control_retention(runtime, handle, &target) != 0
    )
        return 248;

    if (bray_runtime_scheduler_creation(1, 0, SIZE_MAX, &scheduler) != 0)
        return 230;

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_deadline_creation(&scheduler, SIZE_MAX, &deadlines) != 1)
        return 231;

    atomic_store(&reject_allocations, 0);

    // The default unbounded limit must not allocate its maximum at creation.
    if (bray_runtime_deadline_creation(&scheduler, SIZE_MAX, &deadlines) != 0)
        return 232;

    DeadlineContext rejected = { .deadlines = &deadlines };
    uint64_t untouched = 77;
    atomic_store(&reject_allocations, 1);

    if (
        register_deadline(&deadlines, 1, 0, &rejected, &untouched) != 1 ||
        untouched != 77 ||
        rejected.wakes != 0 ||
        rejected.releases != 0 ||
        bray_runtime_deadline_membership_count(&deadlines) != 0
    )
        return 233;

    atomic_store(&reject_allocations, 0);

    enum { MEMBERS = 256 };
    DeadlineContext contexts[MEMBERS] = {0};
    uint64_t identities[MEMBERS] = {0};
    contexts[MEMBERS - 1].target = &target;
    unsigned sequence = 0;
    unsigned allocations_before = atomic_load(&allocation_attempts);
    size_t bytes_before = atomic_load(&retained_bytes);
    clock_t insertion_start = clock();

    for (unsigned index = 0; index < MEMBERS; ++index)
    {
        contexts[index].deadlines = &deadlines;
        contexts[index].sequence = &sequence;

        if (
            register_deadline(
                &deadlines,
                MEMBERS - index,
                9,
                &contexts[index],
                &identities[index]
            ) != 0
        )
            return 234;
    }

    double insertion_ns = (double)(clock() - insertion_start) * 1e9 / CLOCKS_PER_SEC / MEMBERS;
    size_t membership_bytes = atomic_load(&retained_bytes) - bytes_before;
    uint64_t next = 0;

    if (
        bray_runtime_deadline_next(&deadlines, &next) != 1 ||
        next != 1 ||
        bray_runtime_deadline_membership_count(&deadlines) != MEMBERS ||
        bray_runtime_deadline_dispatch(&deadlines, 0) != 0
    )
        return 235;

    unsigned before = bray_allocation_probe_begin();
    clock_t expiry_start = clock();
    uint32_t status = bray_runtime_deadline_dispatch(&deadlines, MEMBERS);
    double expiry_ns = (double)(clock() - expiry_start) * 1e9 / CLOCKS_PER_SEC / MEMBERS;
    unsigned expiry_allocations = bray_allocation_probe_end(before);

    TaskSnapshot snapshot;

    if (
        status != 0 ||
        expiry_allocations != 0 ||
        sequence != MEMBERS ||
        bray_runtime_task_control_snapshot(&target, &snapshot) != 0 ||
        snapshot.phase != 0 ||
        bray_runtime_task_control_cancellation(&target, 0) != 3
    )
        return 236;

    for (unsigned index = 0; index < MEMBERS; ++index)
    {
        if (
            contexts[index].wakes != 1 ||
            contexts[index].releases != 1 ||
            contexts[index].ordinal != MEMBERS - index ||
            bray_runtime_deadline_withdrawal(&deadlines, identities[index]) != 0
        )
            return 237;
    }

    printf(
        "deadlines members=%u bytes=%zu setup_allocations=%u "
        "insertion_cpu_ns=%.1f expiry_cpu_ns=%.1f expiry_allocations=%u\n",
        MEMBERS,
        membership_bytes,
        atomic_load(&allocation_attempts) - allocations_before,
        insertion_ns,
        expiry_ns,
        expiry_allocations
    );

    DeadlineContext first =
    {
        .deadlines = &deadlines,
        .sequence = &sequence,
        .failure = 17
    };
    DeadlineContext second = { .deadlines = &deadlines, .sequence = &sequence };
    uint64_t first_id, second_id;
    sequence = 0;

    if (
        register_deadline(&deadlines, 5, 7, &first, &first_id) != 0 ||
        register_deadline(&deadlines, 5, 8, &second, &second_id) != 0 ||
        bray_runtime_deadline_dispatch(&deadlines, 5) != 17 ||
        first.ordinal != 1 ||
        second.ordinal != 2 ||
        first.releases != 1 ||
        second.releases != 1
    )
        return 238;

    first = (DeadlineContext){ .deadlines = &deadlines };
    second = (DeadlineContext){ .deadlines = &deadlines };

    if (
        register_deadline(&deadlines, 5, 7, &first, &first_id) != 0 ||
        register_deadline(&deadlines, 5, 8, &second, &second_id) != 0 ||
        bray_runtime_deadline_withdrawal(&deadlines, first_id) != 1 ||
        bray_runtime_deadline_withdrawal(&deadlines, first_id) != 0
    )
        return 239;

    bray_runtime_deadline_scope_withdrawal(&deadlines, 8);

    if (
        first.wakes != 0 ||
        second.wakes != 0 ||
        first.releases != 1 ||
        second.releases != 1 ||
        bray_runtime_deadline_membership_count(&deadlines) != 0
    )
        return 240;

    DeadlineContext committed = { .deadlines = &deadlines, .block = 1 };
    uint64_t committed_id;

    if (register_deadline(&deadlines, 0, 0, &committed, &committed_id) != 0)
        return 241;

#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, deadline_dispatch_thread, &deadlines, 0, NULL);

    if (!thread)
        abort();
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, deadline_dispatch_thread, &deadlines) != 0)
        abort();
#endif

    while (!atomic_load_explicit(&committed.entered, memory_order_acquire))
    {
#ifdef _WIN32
        SwitchToThread();
#else
        sched_yield();
#endif
    }

    bray_runtime_deadline_closure(&scheduler);

    if (
        bray_runtime_deadline_withdrawal(&deadlines, committed_id) != 0 ||
        committed.releases != 0 ||
        register_deadline(&deadlines, 0, 0, &rejected, &untouched) != 4 ||
        untouched != 77 ||
        rejected.releases != 0
    )
        return 242;

    atomic_store_explicit(&committed.proceed, 1, memory_order_release);
    join_event_signal(thread);

    if (
        committed.wakes != 1 ||
        committed.releases != 1
    )
        return 243;

    bray_runtime_deadline_destruction(&deadlines);

    bray_runtime_scheduler_destruction(&scheduler);

    if (bray_runtime_scheduler_creation(1, 0, SIZE_MAX, &scheduler) != 0)
        return 244;

    if (bray_runtime_deadline_creation(&scheduler, 1, &deadlines) != 0)
        return 244;

    first = (DeadlineContext)
    {
        .deadlines = &deadlines,
        .scheduler = &scheduler,
        .close = 1
    };
    second = (DeadlineContext){ .deadlines = &deadlines };

    if (
        register_deadline(&deadlines, 1, 0, &first, &first_id) != 0 ||
        register_deadline(&deadlines, 2, 0, &second, &untouched) != 2 ||
        untouched != 77 ||
        second.releases != 0
    )
        return 245;

    bray_runtime_deadline_closure(&scheduler);

    if (
        bray_runtime_deadline_dispatch(&deadlines, 1) != 0 ||
        first.wakes != 1 ||
        first.releases != 1
    )
        return 246;

    bray_runtime_deadline_destruction(&deadlines);
    bray_runtime_scheduler_destruction(&scheduler);

    if (bray_runtime_scheduler_creation(1, 0, SIZE_MAX, &scheduler) != 0)
        return 250;

    DeadlineReference unformed = {77};
    bray_runtime_deadline_closure(&scheduler);

    if (
        bray_runtime_deadline_creation(&scheduler, SIZE_MAX, &unformed) != 4 ||
        unformed.address != 77
    )
        return 251;

    bray_runtime_scheduler_destruction(&scheduler);

    if (
        bray_runtime_task_control_destruction(&target) != 0 ||
        bray_runtime_task_control_reference_release(&target) != 0
    )
        return 249;

    return 0;
}
