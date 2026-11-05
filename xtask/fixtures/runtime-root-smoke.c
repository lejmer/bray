typedef struct
{
    uintptr_t tasks;
    uintptr_t timers;
    uint32_t capabilities;
    const void* invoke;
    const void* admit_context;
    const void* release_context;
} RootConfiguration;

typedef struct
{
    uint32_t status;
    uint64_t task;
} RootAdmission;

extern uint32_t bray_runtime_main_thread_lane_startup(RootConfiguration configuration);
extern uint32_t bray_runtime_main_thread_lane_drive(void);
extern RootAdmission bray_runtime_root_execution(uintptr_t frame, RunOutcome* outcome);
extern RunOutcome bray_runtime_root_terminal_observation(uint64_t task);
extern uint32_t bray_runtime_root_completion_resolution(uint64_t task);
extern uint32_t bray_runtime_root_cancellation_request(uint64_t task);

enum RootMode
{
    ROOT_SUCCESS,
    ROOT_PANICKED,
    ROOT_CANCELLED,
};

typedef struct
{
    ActivationHeader header;
    uintptr_t value;
    enum RootMode mode;
    unsigned resumes;
    unsigned lexical_cleanup;
} RootFrame;

static uint64_t root_thread;
static unsigned root_destructions;

static void require_root_thread(void)
{
    if (bray_runtime_current_native_thread_identity() != root_thread)
        abort();
}

static ActivationState root_state(
    uintptr_t context,
    uint32_t state,
    uint32_t operation,
    uintptr_t argument
)
{
    (void)context;
    (void)state;
    (void)operation;
    (void)argument;
    require_root_thread();

    return (ActivationState){ .affinity = 2, .lanes = 4 };
}

static void root_resume(ActivationProgress* progress, uintptr_t context)
{
    RootFrame* frame = (RootFrame*)context;
    require_root_thread();
    ++frame->resumes;

    if (frame->mode == ROOT_CANCELLED)
    {
        *progress = (ActivationProgress){ .kind = 0, .state = 1 };
        return;
    }

    if (frame->mode == ROOT_SUCCESS && frame->resumes == 1)
    {
        *progress = (ActivationProgress){ .kind = 5, .state = 1 };
        return;
    }

    frame->lexical_cleanup = 1;

    if (frame->mode == ROOT_PANICKED)
        *progress = (ActivationProgress){ .kind = 3, .state = 1, .report = panic_report() };
    else
        *progress = (ActivationProgress){ .kind = 1, .state = 1, .payload = (uintptr_t)&frame->value };
}

static void root_cancel(ActivationProgress* progress, uintptr_t context)
{
    RootFrame* frame = (RootFrame*)context;
    require_root_thread();
    frame->lexical_cleanup = 1;
    *progress = (ActivationProgress){ .kind = 2, .state = 1 };
}

static void root_broadcast(ActivationProgress* progress, uintptr_t context)
{
    (void)context;
    require_root_thread();
    *progress = (ActivationProgress){ .kind = 1 };
}

static void root_resolve(ActivationProgress* progress, uintptr_t context, uint32_t exit)
{
    (void)context;
    (void)exit;
    require_root_thread();
    *progress = (ActivationProgress){ .kind = 1 };
}

static void root_move(uintptr_t context, uintptr_t destination)
{
    RootFrame* frame = (RootFrame*)context;
    require_root_thread();
    *(uintptr_t*)destination = frame->value;
    frame->value = 0;
}

static void root_destroy(uintptr_t context)
{
    require_root_thread();
    ++root_destructions;
    activation_destroy(context);
}

static int check_execution_roots(void)
{
    const RootConfiguration configuration = {
        .tasks = 1,
        .timers = 1,
        .capabilities = 39,
    };
    const ActivationDescriptor descriptor = {
        .storage = { { 21 }, sizeof(RootFrame), _Alignof(RootFrame) },
        .completion = offsetof(RootFrame, value),
        .completion_bytes = sizeof(uintptr_t),
        .completion_align = _Alignof(uintptr_t),
        .states = 2,
        .state = root_state,
        .resume = root_resume,
        .cancel = root_cancel,
        .broadcast = root_broadcast,
        .resolve = root_resolve,
        .move_completion = root_move,
        .destroy = root_destroy,
    };
    unsigned worker_attempts = atomic_load(&worker_create_attempts);

    if (
        bray_runtime_initialization() != 0 ||
        bray_runtime_main_thread_lane_drive() != 1 ||
        bray_runtime_main_thread_lane_startup(configuration) != 0
    )
        return 249;

    root_thread = bray_runtime_current_native_thread_identity();

    for (enum RootMode mode = ROOT_SUCCESS; mode <= ROOT_CANCELLED; ++mode)
    {
        CleanupLease lease = { 0 };

        if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0)
            return 249;

        RootFrame* frame = bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);
        InactiveActivation token = { lease, &descriptor };
        ActivationRecords records = { 0 };

        if (frame == NULL)
            return 249;

        bray_runtime_activation_initialization(&token, (uintptr_t)frame, &records);
        frame->value = 101;
        frame->mode = mode;
        frame->resumes = 0;
        frame->lexical_cleanup = 0;

        if (bray_runtime_activation_publication(&token) != 0)
            return 249;

        RunOutcome outcome = { 0 };
        RootAdmission admitted = bray_runtime_root_execution((uintptr_t)&token, &outcome);
        unsigned destructions = root_destructions;

        if (admitted.status != 0 || admitted.task == 0)
            return 249;

        if (mode == ROOT_CANCELLED)
        {
            if (
                bray_runtime_main_thread_lane_drive() != 0 ||
                frame->resumes != 1 ||
                frame->lexical_cleanup != 0 ||
                bray_runtime_root_cancellation_request(admitted.task) != 0
            )
                return 249;
        }

        outcome = bray_runtime_root_terminal_observation(admitted.task);

        if (frame->lexical_cleanup != 1 || root_destructions != destructions)
            return 249;

        if (mode == ROOT_PANICKED)
        {
            if (outcome.state != 2 || bray_runtime_panic_report_destruction(&outcome.report) != 0)
                return 249;
        }
        else if (mode == ROOT_CANCELLED)
        {
            if (outcome.state != 1)
                return 249;
        }
        else if (outcome.state != 0 || *(uintptr_t*)outcome.payload != 101 || frame->resumes != 2)
            return 249;

        if (
            bray_runtime_root_completion_resolution(admitted.task) != 0 ||
            root_destructions != destructions + 1
        )
            return 249;
    }

    if (
        atomic_load(&worker_create_attempts) != worker_attempts ||
        shutdown_product() != 0 ||
        atomic_load(&live_allocations) != 0
    )
        return 249;

    return 0;
}
