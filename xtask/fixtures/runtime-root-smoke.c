typedef struct
{
    uintptr_t tasks;
    uintptr_t timers;
    uint32_t capabilities;
    const void* invoke;
    const void* admit_context;
    const void* release_context;
    uintptr_t scope_enter;
    uintptr_t scope_finish;
} RootConfiguration;

typedef struct
{
    uint32_t status;
    uint64_t task;
} RootAdmission;

typedef struct
{
    uintptr_t storage;
} RootOwner;

typedef struct
{
    _Alignas(16) unsigned char storage[128];
} RootScope;

extern uint32_t bray_runtime_main_thread_lane_startup(RootConfiguration configuration);
extern uint32_t bray_runtime_main_thread_lane_drive(void);
extern RootAdmission bray_runtime_root_execution(uintptr_t frame, RunOutcome* outcome);
extern RunOutcome bray_runtime_root_terminal_observation(uint64_t task);
extern uint32_t bray_runtime_root_completion_resolution(uint64_t task);
extern uint32_t bray_runtime_root_cancellation_request(uint64_t task);
extern uint32_t bray_runtime_execution_owner_current(RootOwner* output);
extern void bray_runtime_execution_owner_clone(const RootOwner* owner, RootOwner* output);
extern uint32_t bray_runtime_execution_owner_release(RootOwner* owner, const void* scope);
extern uint32_t bray_runtime_execution_owner_worker(const RootOwner* owner);
extern uint64_t bray_runtime_execution_owner_scope_enter(
    void* storage,
    uintptr_t bytes,
    uintptr_t alignment,
    const RootOwner* owner
);
extern uint32_t bray_runtime_execution_scope_finish(void* storage);
extern void bray_runtime_activation_callback(ActivationInvocation* invocation);
extern void bray_runtime_activation_action_readiness(ActivationInvocation* invocation);
extern void bray_runtime_inactive_frame_broadcast(
    InactiveActivation* token,
    void (*invoke)(ActivationInvocation*),
    void (*ready)(ActivationInvocation*),
    RunOutcome* outcome
);
extern void bray_runtime_inactive_frame_resolution(
    InactiveActivation* token,
    void (*invoke)(ActivationInvocation*),
    void (*ready)(ActivationInvocation*),
    RunOutcome* outcome
);
extern void bray_runtime_awaited_frame_composition(
    InactiveActivation frame,
    uint32_t state,
    uintptr_t destination,
    uint8_t disposition
);

enum RootMode
{
    ROOT_SUCCESS,
    ROOT_PANICKED,
    ROOT_CANCELLED,
    ROOT_CHILD,
    ROOT_REJECTED_BLOCKING,
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
static unsigned root_child_destructions;

static int enter_root_owner(RootScope* scope, const RootOwner* owner)
{
    return bray_runtime_execution_owner_scope_enter(
        scope->storage,
        sizeof(*scope),
        _Alignof(RootScope),
        owner
    ) == (UINT64_C(1) << 32);
}

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
    RootFrame* frame = (RootFrame*)context;
    (void)state;
    (void)operation;
    (void)argument;
    require_root_thread();

    return (ActivationState){
        .affinity = 2,
        .lanes = frame->mode == ROOT_REJECTED_BLOCKING ? 5 : 4,
    };
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

    if (frame->mode == ROOT_CHILD)
    {
        frame->value = 7;
        frame->lexical_cleanup = 1;
        *progress = (ActivationProgress) {
            .kind = 1,
            .state = 1,
            .payload = (uintptr_t)&frame->value,
        };
        return;
    }

    if (
        frame->mode == ROOT_SUCCESS &&
        frame->resumes == 1
    )
    {
        *progress = (ActivationProgress){ .kind = 5, .state = 1 };
        return;
    }

    if (
        frame->mode == ROOT_SUCCESS &&
        frame->resumes == 2
    )
    {
        const ActivationDescriptor* descriptor = frame->header.descriptor;
        CleanupLease lease = { 0 };
        ActivationRecords records = { 0 };

        if (bray_runtime_cleanup_storage_admission(&descriptor->storage, &lease) != 0)
            abort();

        RootFrame* child = bray_runtime_cleanup_storage_activation(&lease, &descriptor->storage);
        InactiveActivation token = { lease, descriptor };

        if (child == NULL)
            abort();

        bray_runtime_activation_initialization(&token, (uintptr_t)child, &records);
        child->value = 0;
        child->mode = ROOT_CHILD;
        child->resumes = 0;
        child->lexical_cleanup = 0;

        if (bray_runtime_activation_publication(&token) != 0)
            abort();

        bray_runtime_awaited_frame_composition(token, 1, (uintptr_t)&frame->value, 0);
        *progress = (ActivationProgress){ .kind = 0, .state = 1 };
        return;
    }

    if (frame->mode == ROOT_SUCCESS)
    {
        RunOutcome child = { 0 };
        bray_runtime_frame_completion_move(&frame->header, &child);

        if (
            child.state != 0 ||
            frame->value != 7
        )
            abort();

        frame->value = 101;
    }

    frame->lexical_cleanup = 1;

    if (frame->mode == ROOT_PANICKED)
        *progress = (ActivationProgress){ .kind = 3, .state = 1, .report = panic_report() };
    else
        *progress = (ActivationProgress) {
            .kind = 1,
            .state = 1,
            .payload = (uintptr_t)&frame->value,
        };
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
    RootFrame* frame = (RootFrame*)context;
    require_root_thread();

    if (frame->mode == ROOT_CHILD)
        ++root_child_destructions;
    else
        ++root_destructions;

    activation_destroy(context);
}

static int check_execution_roots(void)
{
    const RootConfiguration configuration = {
        .tasks = 1,
        .timers = 1,
        .capabilities = 63,
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

    RootOwner retained = { 0 };
    RootOwner duplicate = { 0 };
    RootScope scope;

    if (bray_runtime_execution_owner_current(&retained) != 0)
        return 249;

    bray_runtime_execution_owner_clone(&retained, &duplicate);

    if (
        !enter_root_owner(&scope, &duplicate) ||
        bray_runtime_main_thread_lane_drive() != 6 ||
        bray_runtime_execution_owner_worker(&duplicate) != 0 ||
        bray_runtime_execution_owner_release(&duplicate, scope.storage) != 0 ||
        bray_runtime_execution_scope_finish(scope.storage) != 0
    )
        return 249;

    CleanupLease rejected_lease = { 0 };
    ActivationRecords rejected_records = { 0 };
    RunOutcome rejected_outcome = { 0 };

    if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &rejected_lease) != 0)
        return 249;

    RootFrame* rejected_frame = bray_runtime_cleanup_storage_activation(
        &rejected_lease,
        &descriptor.storage
    );
    InactiveActivation rejected_token = { rejected_lease, &descriptor };

    if (rejected_frame == NULL)
        return 249;

    bray_runtime_activation_initialization(
        &rejected_token,
        (uintptr_t)rejected_frame,
        &rejected_records
    );
    rejected_frame->mode = ROOT_REJECTED_BLOCKING;
    rejected_frame->resumes = 0;

    if (bray_runtime_activation_publication(&rejected_token) != 0)
        return 249;

    RootAdmission rejected = bray_runtime_root_execution(
        (uintptr_t)&rejected_token,
        &rejected_outcome
    );
    unsigned before_rejection_disposal = root_destructions;

    if (
        rejected.status == 0 ||
        rejected.task != 0 ||
        rejected_frame->resumes != 0
    )
        return 249;

    bray_runtime_inactive_frame_broadcast(
        &rejected_token,
        bray_runtime_activation_callback,
        bray_runtime_activation_action_readiness,
        &rejected_outcome
    );
    bray_runtime_inactive_frame_resolution(
        &rejected_token,
        bray_runtime_activation_callback,
        bray_runtime_activation_action_readiness,
        &rejected_outcome
    );

    if (
        rejected_outcome.state != 0 ||
        root_destructions != before_rejection_disposal + 1
    )
        return 249;

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
        unsigned child_destructions = root_child_destructions;

        if (
            admitted.status != 0 ||
            admitted.task == 0
        )
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

        if (
            frame->lexical_cleanup != 1 ||
            root_destructions != destructions
        )
            return 249;

        if (mode == ROOT_PANICKED)
        {
            if (
                outcome.state != 2 ||
                bray_runtime_panic_report_destruction(&outcome.report) != 0
            )
                return 249;
        }
        else if (mode == ROOT_CANCELLED)
        {
            if (outcome.state != 1)
                return 249;
        }
        else if (
            outcome.state != 0 ||
            *(uintptr_t*)outcome.payload != 101 ||
            frame->resumes != 3 ||
            root_child_destructions != child_destructions + 1
        )
            return 249;

        if (
            bray_runtime_root_completion_resolution(admitted.task) != 0 ||
            root_destructions != destructions + 1
        )
            return 249;
    }

    if (
        bray_runtime_execution_shutdown() != 0 ||
        bray_runtime_main_thread_lane_drive() != 1 ||
        bray_runtime_thread_attachment_enter() != 0 ||
        !enter_root_owner(&scope, &retained) ||
        bray_runtime_main_thread_lane_drive() != 3 ||
        bray_runtime_execution_owner_release(&retained, scope.storage) != 0 ||
        bray_runtime_execution_scope_finish(scope.storage) != 0 ||
        bray_runtime_thread_attachment_leave() != 0 ||
        atomic_load(&worker_create_attempts) != worker_attempts ||
        shutdown_product() != 0 ||
        atomic_load(&live_allocations) != 0
    )
        return 249;

    return 0;
}
