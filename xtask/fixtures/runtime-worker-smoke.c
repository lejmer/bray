#if defined(__linux__)
typedef struct
{
    uintptr_t address;
} WorkerSmokeReference;

typedef struct
{
    uint32_t stage;
    uint32_t category;
    int64_t native_code;
} WorkerSmokeFailure;

typedef struct
{
    uintptr_t context;
    uint32_t (*call)(uintptr_t, uint32_t, const WorkerSmokeReference*, uint64_t*, RunOutcome*);
} WorkerSmokeCallback;

extern uint32_t bray_runtime_worker_creation(
    const SchedulerReference*,
    const WorkerSmokeCallback*,
    WorkerSmokeReference*
);
extern uint32_t bray_runtime_worker_demand(const WorkerSmokeReference*, uint32_t, WorkerSmokeFailure*);
extern void bray_runtime_worker_stop(const WorkerSmokeReference*);
extern void bray_runtime_worker_destruction(const WorkerSmokeReference*);
extern int __real_pthread_create(pthread_t*, const pthread_attr_t*, void* (*)(void*), void*);
extern int __real_pthread_join(pthread_t, void**);

static atomic_bool reject_worker_creation;
static atomic_uint worker_create_attempts;
static atomic_uint worker_joins;
static atomic_uint worker_retains;
static atomic_uint worker_releases;
static atomic_uint worker_bound;
static atomic_uint worker_unbound;
static atomic_uint worker_ready;
static atomic_uint worker_cleanups;
static atomic_uint worker_completions;
static atomic_bool worker_ordering_failure;
static _Thread_local uint64_t worker_attachment;
static _Thread_local pthread_t worker_native_thread;

int __wrap_pthread_create(
    pthread_t* thread,
    const pthread_attr_t* attributes,
    void* (*entry)(void*),
    void* context
)
{
    atomic_fetch_add(&worker_create_attempts, 1);

    if (atomic_exchange(&reject_worker_creation, 0))
        return EAGAIN;

    return __real_pthread_create(thread, attributes, entry, context);
}

int __wrap_pthread_join(pthread_t thread, void** result)
{
    if (pthread_equal(thread, pthread_self()))
        abort();

    atomic_fetch_add(&worker_joins, 1);
    return __real_pthread_join(thread, result);
}

static uint32_t worker_smoke_cleanup(uint64_t identity)
{
    if (
        identity == 0 ||
        identity != worker_attachment ||
        !pthread_equal(worker_native_thread, pthread_self()) ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&worker_unbound) != 0
    )
        atomic_store(&worker_ordering_failure, 1);

    atomic_fetch_add(&worker_cleanups, 1);
    return 0;
}

static uint32_t worker_smoke_callback(
    uintptr_t context,
    uint32_t operation,
    const WorkerSmokeReference* scope,
    uint64_t* argument,
    RunOutcome* outcome
)
{
    (void)context;
    (void)scope;
    (void)outcome;

    switch (operation)
    {
        case 0:
            atomic_fetch_add(&worker_retains, 1);
            break;
        case 1:
            if (
                atomic_load(&worker_releases) + 1 == atomic_load(&worker_retains) &&
                atomic_load(&worker_cleanups) != 1
            )
                atomic_store(&worker_ordering_failure, 1);

            atomic_fetch_add(&worker_releases, 1);
            break;
        case 2:
            worker_attachment = bray_runtime_current_native_thread_identity();
            worker_native_thread = pthread_self();

            if (
                worker_attachment == 0 ||
                bray_runtime_thread_attachment_cleanup(worker_smoke_cleanup) != 0
            )
                abort();

            atomic_fetch_add(&worker_bound, 1);
            break;
        case 3:
            atomic_store(&worker_ready, 1);
            break;
        case 5:
            if (atomic_load(&worker_cleanups) != 1)
                atomic_store(&worker_ordering_failure, 1);

            atomic_fetch_add(&worker_unbound, 1);
            break;
        case 6:
            *argument = 1;
            break;
        case 8:
            if (atomic_load(&worker_cleanups) != 1)
                atomic_store(&worker_ordering_failure, 1);

            atomic_fetch_add(&worker_completions, 1);
            break;
        default:
            abort();
    }

    return 0;
}

static int check_worker_runtime(void)
{
    SchedulerReference scheduler = {0};
    WorkerSmokeReference workers = {0};
    WorkerSmokeCallback callback = {0, worker_smoke_callback};
    WorkerSmokeFailure failure = {0};
    unsigned created = atomic_load(&worker_create_attempts);
    unsigned joined = atomic_load(&worker_joins);
    unsigned live = atomic_load(&live_allocations);

    if (
        bray_runtime_initialization(1, 1) != 0 ||
        bray_runtime_scheduler_creation(29, 0, 4, &scheduler) != 0 ||
        bray_runtime_worker_creation(&scheduler, &callback, &workers) != 0 ||
        atomic_load(&worker_create_attempts) != created ||
        bray_runtime_worker_demand(&workers, 0, &failure) != 0
    )
        return 248;

    while (atomic_load(&worker_ready) == 0)
        sched_yield();

    atomic_store(&reject_worker_creation, 1);

    if (
        bray_runtime_worker_demand(&workers, 2, &failure) == 0 ||
        failure.stage != 2 ||
        failure.category == 0 ||
        failure.native_code != EAGAIN
    )
        return 249;

    unsigned attempts = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);
    bray_runtime_worker_stop(&workers);
    bray_runtime_worker_stop(&workers);
    bray_runtime_worker_destruction(&workers);
    bray_runtime_scheduler_destruction(&scheduler);
    uint32_t shutdown = shutdown_product();
    atomic_store(&reject_allocations, 0);

    if (
        shutdown != 0 ||
        atomic_load(&worker_ordering_failure) ||
        atomic_load(&worker_create_attempts) != created + 2 ||
        atomic_load(&worker_joins) != joined + 1 ||
        atomic_load(&worker_retains) != 2 ||
        atomic_load(&worker_releases) != 2 ||
        atomic_load(&worker_bound) != 1 ||
        atomic_load(&worker_unbound) != 1 ||
        atomic_load(&worker_cleanups) != 1 ||
        atomic_load(&worker_completions) != 1 ||
        atomic_load(&allocation_attempts) != attempts ||
        atomic_load(&live_allocations) != live
    )
        return 250;

    fprintf(stdout, "worker rollback: created=1 rejected=1 joined=1 cleanup=1 shutdown_allocations=0\n");
    return 0;
}
#else
static int check_worker_runtime(void)
{
    return 0;
}
#endif
