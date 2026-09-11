#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <pthread.h>
#endif

typedef struct RunOutcome
{
    uint32_t state;
    uintptr_t payload;
} RunOutcome;

typedef struct ProductHostObservation
{
    uint32_t status;
    uint32_t state;
    uintptr_t active_entries;
    uintptr_t external_roots;
    uintptr_t retirement_roots;
    uintptr_t thread_attachments;
    uintptr_t initialized_statics;
    uintptr_t cleaned_statics;
    uintptr_t cleanup_incidents;
    uint8_t last_incident[32];
} ProductHostObservation;

typedef struct SourceAnchor
{
    uint32_t present;
    uint32_t source;
    uint32_t start;
    uint32_t end;
    uint64_t version;
} SourceAnchor;

typedef struct PanicReportCallbacks
{
    void* report;
    void* destroy;
    void* construct_cleanup;
    void* suppress;
} PanicReportCallbacks;

typedef struct CleanupIncident
{
    uintptr_t payload;
    uint8_t type_identity[32];
    SourceAnchor source;
    void* report;
    void* destroy;
    PanicReportCallbacks panics;
} CleanupIncident;

typedef struct StaticFinalizer
{
    uint32_t execution;
    uint32_t reserved;
    uintptr_t result_size;
    uintptr_t result_alignment;
    void* start;
    void* resolve;
    PanicReportCallbacks panics;
} StaticFinalizer;

typedef struct CleanupRegistration
{
    void* product;
    uint8_t static_identity[32];
    void* prepare;
    StaticFinalizer finalizer;
    void* destroy;
    void* detach;
} CleanupRegistration;

typedef struct ShutdownRace
{
    RunOutcome outcome;
} ShutdownRace;

static _Thread_local uintptr_t cleanup_shield_depth;
static _Atomic uintptr_t cleanup_shield_entries;
static _Atomic uintptr_t cleanup_shield_exits;

// This synchronous host has no cancellation source. Retain nesting checks at its cleanup boundary.
void bray_runtime_cleanup_shield_enter(void)
{
    cleanup_shield_depth += 1;
    atomic_fetch_add_explicit(&cleanup_shield_entries, 1, memory_order_relaxed);
}

void bray_runtime_cleanup_shield_leave(void)
{
    if (cleanup_shield_depth == 0)
        abort();

    cleanup_shield_depth -= 1;
    atomic_fetch_add_explicit(&cleanup_shield_exits, 1, memory_order_relaxed);
}

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);
static uint32_t substrate_initialized;

uint32_t bray_runtime_substrate_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity)
{
    (void)worker_capacity;
    (void)timer_capacity;

    if (substrate_initialized != 0)
        return 2;

    substrate_initialized = 1;

    return 0;
}

uint32_t bray_runtime_substrate_shutdown(void)
{
    if (substrate_initialized == 0)
        return 1;

    substrate_initialized = 0;

    return 0;
}

ProductHostObservation bray_runtime_substrate_product_host_control(
    void* descriptor,
    uint32_t operation
)
{
    (void)descriptor;
    (void)operation;

    ProductHostObservation observation = {0};

    return observation;
}

extern uint64_t bray_runtime_thread_attachment_identity(void* descriptor);
extern uint32_t bray_runtime_thread_static_cleanup_registration(void* registration);

uint64_t bray_runtime_substrate_thread_attachment_identity(void* descriptor)
{
    (void)descriptor;
    return bray_runtime_thread_attachment_identity(NULL);
}

uint32_t bray_runtime_substrate_thread_static_cleanup_registration(void* registration)
{
    CleanupRegistration bootstrap_registration = *(CleanupRegistration*)registration;
    bootstrap_registration.product = NULL;

    return bray_runtime_thread_static_cleanup_registration(&bootstrap_registration);
}

typedef void (*SynchronousCallback)(uintptr_t context, RunOutcome* outcome);

RunOutcome bray_runtime_substrate_synchronous_root_execution(
    void* callback,
    uintptr_t context,
    void (*cleanup)(void)
)
{
    RunOutcome outcome = {3, 4};
    ((SynchronousCallback)callback)(context, &outcome);

    cleanup();

    return outcome;
}

RunOutcome bray_runtime_substrate_foreign_callback_execution(
    void* callback,
    uintptr_t context,
    void (*cleanup)(void)
)
{
    RunOutcome outcome = {3, 4};
    ((SynchronousCallback)callback)(context, &outcome);

    cleanup();

    return outcome;
}

uint32_t bray_runtime_substrate_native_thread_execution(
    void* operation,
    uintptr_t context,
    void* cancellation,
    uintptr_t cancellation_context,
    uintptr_t* panic_payload,
    void (*cleanup)(void)
)
{
    (void)operation;
    (void)context;
    (void)cancellation;
    (void)cancellation_context;

    *panic_payload = 0;

    cleanup();

    return 0;
}

uint32_t bray_runtime_substrate_panic_reporting(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t* message,
    uintptr_t message_length,
    uint32_t (*copy_message)(const uint8_t*, uint8_t*, uintptr_t)
)
{
    (void)source_identity;
    (void)source_version;

    if (cause > 4 || source_present > 1 || source_start > source_end)
        return 3;

    if (message == NULL && message_length != 0)
        return 3;

    if (message_length != 0)
    {
        if (copy_message == NULL)
            return 3;

        uint8_t* copy = malloc(message_length);

        if (copy == NULL)
            return 5;

        uint32_t status = copy_message(message, copy, message_length);

        if (status == 0 && memcmp(copy, message, message_length) != 0)
            status = 3;

        free(copy);

        if (status != 0)
            return status;
    }

    return 0;
}

extern RunOutcome bray_runtime_synchronous_root_execution(void* callback, uintptr_t context);
extern RunOutcome bray_runtime_foreign_callback_execution(void* callback, uintptr_t context);
extern uint64_t bray_runtime_thread_attachment_identity(void* descriptor);
extern uint32_t bray_runtime_thread_static_cleanup_registration(void* registration);
extern uintptr_t bray_runtime_panic_report_construction(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t* message,
    uintptr_t message_length
);
extern uint32_t bray_runtime_panic_reporting(uintptr_t report);
extern uint32_t bray_runtime_panic_report_observation(uintptr_t report);
extern uint32_t bray_runtime_panic_report_destruction(uintptr_t report);
extern uintptr_t bray_runtime_panic_report_suppression(uintptr_t primary, uintptr_t incident);
extern uintptr_t bray_runtime_cleanup_incident_construction(const CleanupIncident* incident);
extern uint32_t bray_runtime_structured_shutdown(void);
extern uint64_t bray_runtime_bootstrap_thread_static_probe(void);
extern uint64_t bray_runtime_bootstrap_thread_static_cleanup_observation(void);

static _Atomic uint32_t thread_ready;
static _Atomic uint32_t thread_release;
static uint32_t cleanup_order[3];
static uint32_t cleanup_count;
static uint32_t cleanup_incident_reports;
static uint32_t cleanup_incident_destroys;

static void completed_callback(uintptr_t context, RunOutcome* outcome)
{
    outcome->state = 0;
    outcome->payload = context;
}

static void cancelled_callback(uintptr_t context, RunOutcome* outcome)
{
    (void)context;

    outcome->state = 1;
    outcome->payload = 0;
}

static uintptr_t panic_report(void)
{
    static const uint8_t message[] = "bootstrap panic";

    return bray_runtime_panic_report_construction(0, 0, 0, 0, 0, 0, message, sizeof(message) - 1);
}

static void panicked_callback(uintptr_t context, RunOutcome* outcome)
{
    (void)context;

    outcome->state = 2;
    outcome->payload = panic_report();
}

static void static_transition(void)
{
}

static uint32_t report_cleanup_incident(const CleanupIncident* incident)
{
    if (incident->payload != 2)
        abort();

    cleanup_incident_reports += 1;

    return 0;
}

static uintptr_t destroy_cleanup_incident(uintptr_t payload)
{
    if (payload != 2)
        abort();

    cleanup_incident_destroys += 1;

    return 0;
}

typedef struct OwnedError
{
    uint32_t identity;
    uint32_t report_status;
} OwnedError;

static uint32_t owned_error_events[16];
static uint32_t owned_error_event_count;

static uint32_t report_owned_error(const CleanupIncident* incident)
{
    OwnedError* error = (OwnedError*)incident->payload;
    owned_error_events[owned_error_event_count++] = error->identity;

    return error->report_status;
}

static uintptr_t destroy_owned_error(uintptr_t payload)
{
    OwnedError* error = (OwnedError*)payload;
    uint32_t identity = error->identity;
    owned_error_events[owned_error_event_count++] = 100 + error->identity;

    free(error);

    return identity == 7 ? panic_report() : 0;
}

static PanicReportCallbacks panic_callbacks(void)
{
    PanicReportCallbacks callbacks = {
        (void*)&bray_runtime_panic_report_observation,
        (void*)&bray_runtime_panic_report_destruction,
        (void*)&bray_runtime_cleanup_incident_construction,
        (void*)&bray_runtime_panic_report_suppression,
    };

    return callbacks;
}

static uintptr_t owned_error_report(uint32_t identity, uint32_t report_status)
{
    OwnedError* error = malloc(sizeof(*error));

    if (error == NULL)
        abort();

    error->identity = identity;
    error->report_status = report_status;

    CleanupIncident incident = {0};

    incident.payload = (uintptr_t)error;
    incident.type_identity[0] = 7;
    incident.source.present = 1;
    incident.source.source = 9;
    incident.source.start = 2;
    incident.source.end = 5;
    incident.source.version = 11;
    incident.report = (void*)&report_owned_error;
    incident.destroy = (void*)&destroy_owned_error;
    incident.panics = panic_callbacks();

    return bray_runtime_cleanup_incident_construction(&incident);
}

static int verify_owned_cleanup_reports(void)
{
    uintptr_t primary = panic_report();

    uintptr_t report = bray_runtime_panic_report_suppression(primary, owned_error_report(1, 0));
    report = bray_runtime_panic_report_suppression(report, owned_error_report(2, 0));

    if (report != primary || bray_runtime_panic_reporting(report) != 0)
        return 0;

    report = bray_runtime_panic_report_suppression(panic_report(), owned_error_report(3, 0));

    if (bray_runtime_panic_report_destruction(report) != 0)
        return 0;

    report = bray_runtime_panic_report_suppression(owned_error_report(4, 5), owned_error_report(5, 0));

    if (bray_runtime_panic_reporting(report) != 5)
        return 0;

    report = owned_error_report(6, 0);

    if (
        bray_runtime_panic_report_observation(report) != 0 ||
        bray_runtime_panic_report_observation(report) != 0 ||
        bray_runtime_panic_report_destruction(report) != 0
    )
        return 0;

    if (bray_runtime_panic_reporting(owned_error_report(7, 0)) != 0)
        return 0;

    const uint32_t expected[] = {1, 2, 101, 102, 103, 4, 5, 104, 105, 6, 6, 106, 7, 107};

    if (owned_error_event_count != sizeof(expected) / sizeof(expected[0]))
        return 0;

    for (size_t index = 0; index < sizeof(expected) / sizeof(expected[0]); index++)
        if (owned_error_events[index] != expected[index])
            return 0;

    return 1;
}

static uint32_t record_cleanup(uint32_t ordinal, uintptr_t destination)
{
    cleanup_order[cleanup_count++] = ordinal;

    if (ordinal == 2)
    {
        CleanupIncident incident = {0};

        incident.payload = 2;
        incident.report = (void*)&report_cleanup_incident;
        incident.destroy = (void*)&destroy_cleanup_incident;
        incident.panics = panic_callbacks();

        *(CleanupIncident*)destination = incident;

        return 1;
    }

    return 0;
}

static uint32_t first_cleanup(uintptr_t destination, uintptr_t* outcome)
{
    *outcome = 1;

    return record_cleanup(1, destination);
}

static uint32_t second_cleanup(uintptr_t destination, uintptr_t* outcome)
{
    *outcome = 0;

    return record_cleanup(2, destination);
}

static uint32_t third_cleanup(uintptr_t destination, uintptr_t* outcome)
{
    *outcome = panic_report();

    return record_cleanup(3, destination);
}

static uint32_t resolve_cleanup(uintptr_t completed, uintptr_t destination, uintptr_t* outcome)
{
    (void)completed;
    (void)destination;

    *outcome = 0;

    return 0;
}

static uintptr_t static_cleanup(void)
{
    return 0;
}

static CleanupRegistration cleanup_registration(uint8_t identity, void* start)
{
    CleanupRegistration registration = {0};

    registration.static_identity[0] = identity;
    registration.prepare = (void*)&static_transition;
    registration.finalizer.execution = 1;
    registration.finalizer.result_size = sizeof(CleanupIncident);
    registration.finalizer.result_alignment = _Alignof(CleanupIncident);
    registration.finalizer.start = start;
    registration.finalizer.resolve = (void*)&resolve_cleanup;
    registration.finalizer.panics = panic_callbacks();
    registration.destroy = (void*)&static_cleanup;
    registration.detach = (void*)&static_transition;

    return registration;
}

static int exercise_thread_attachment(void)
{
    uint64_t identity = bray_runtime_thread_attachment_identity(NULL);

    if (identity == 0 || bray_runtime_thread_attachment_identity(NULL) != identity)
        return 1;

    if (bray_runtime_bootstrap_thread_static_probe() != 46)
        return 2;

    void* starts[] = {(void*)&first_cleanup, (void*)&second_cleanup, (void*)&third_cleanup};

    for (uint8_t index = 0; index < 3; index += 1)
    {
        CleanupRegistration registration = cleanup_registration(index + 1, starts[index]);

        if (bray_runtime_thread_static_cleanup_registration(&registration) != 0)
            return 3;
    }

    atomic_store_explicit(&thread_ready, 1, memory_order_release);

    while (atomic_load_explicit(&thread_release, memory_order_acquire) == 0)
    {
    }

    return 0;
}

#if defined(_WIN32)
static DWORD WINAPI thread_entry(LPVOID context)
{
    (void)context;

    return (DWORD)exercise_thread_attachment();
}

static DWORD WINAPI shutdown_race_entry(LPVOID context)
{
    ShutdownRace* race = (ShutdownRace*)context;

    race->outcome = bray_runtime_synchronous_root_execution((void*)&completed_callback, 97);

    return 0;
}
#else
static void* thread_entry(void* context)
{
    (void)context;

    return (void*)(uintptr_t)exercise_thread_attachment();
}

static void* shutdown_race_entry(void* context)
{
    ShutdownRace* race = (ShutdownRace*)context;

    race->outcome = bray_runtime_synchronous_root_execution((void*)&completed_callback, 97);

    return NULL;
}
#endif

void bray_runtime_current_run_cancellation_propagation(void)
{
    abort();
}

void bray_runtime_panic_propagation(uintptr_t report)
{
    (void)report;

    abort();
}

int main(void)
{
    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2)
        return 1;

    RunOutcome completed = bray_runtime_synchronous_root_execution((void*)&completed_callback, 41);

    if (completed.state != 0 || completed.payload != 41)
        return 2;

    RunOutcome cancelled = bray_runtime_synchronous_root_execution((void*)&cancelled_callback, 0);

    if (cancelled.state != 1 || cancelled.payload != 0)
        return 3;

    RunOutcome panicked = bray_runtime_synchronous_root_execution((void*)&panicked_callback, 0);

    if (panicked.state != 2 || panicked.payload == 0)
        return 4;

    if (bray_runtime_panic_reporting(panicked.payload) != 0)
        return 5;

    if (bray_runtime_panic_report_destruction(panic_report()) != 0)
        return 6;

    for (uint32_t cause = 3; cause <= 4; ++cause)
    {
        uintptr_t allocation_failure = bray_runtime_panic_report_construction(cause, 0, 0, 0, 0, 0, NULL, 0);

        if (bray_runtime_panic_reporting(allocation_failure) != 0)
            return 6;
    }

    if (!verify_owned_cleanup_reports())
        return 16;

    RunOutcome foreign = bray_runtime_foreign_callback_execution((void*)&completed_callback, 73);

    if (foreign.state != 0 || foreign.payload != 73)
        return 7;

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, thread_entry, NULL, 0, NULL);

    if (thread == NULL)
        return 7;
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, thread_entry, NULL) != 0)
        return 7;
#endif

    while (atomic_load_explicit(&thread_ready, memory_order_acquire) == 0)
    {
    }

    if (bray_runtime_structured_shutdown() != 4)
        return 8;

    atomic_store_explicit(&thread_release, 1, memory_order_release);

#if defined(_WIN32)
    if (WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0)
        return 9;

    DWORD thread_result = 0;

    if (!GetExitCodeThread(thread, &thread_result) || thread_result != 0 || !CloseHandle(thread))
        return 10;
#else
    void* thread_result = NULL;

    if (pthread_join(thread, &thread_result) != 0 || thread_result != NULL)
        return 10;
#endif

    if (cleanup_count != 3)
        return 11;

    if (cleanup_order[0] != 3 || cleanup_order[1] != 2 || cleanup_order[2] != 1)
        return 13;

    if (cleanup_incident_reports != 1 || cleanup_incident_destroys != 1)
        return 14;

    uint64_t bootstrap_cleanup = bray_runtime_bootstrap_thread_static_cleanup_observation();

    if (bootstrap_cleanup != 211717)
    {
        fprintf(
            stderr,
            "bootstrap cleanup observation: %llu\n",
            (unsigned long long)bootstrap_cleanup
        );

        return 15;
    }

    if (bray_runtime_structured_shutdown() != 0 || bray_runtime_structured_shutdown() != 1)
        return 12;

    for (uint32_t iteration = 0; iteration < 128; iteration += 1)
    {
        if (bray_runtime_initialization(1, 1) != 0)
            return 16;

        ShutdownRace race = {0};

#if defined(_WIN32)
        HANDLE racing_thread = CreateThread(NULL, 0, shutdown_race_entry, &race, 0, NULL);

        if (racing_thread == NULL)
            return 17;
#else
        pthread_t racing_thread;

        if (pthread_create(&racing_thread, NULL, shutdown_race_entry, &race) != 0)
            return 17;
#endif

        uint32_t shutdown = bray_runtime_structured_shutdown();

        if (shutdown != 0 && shutdown != 4)
            return 18;

#if defined(_WIN32)
        if (
            WaitForSingleObject(racing_thread, INFINITE) != WAIT_OBJECT_0 ||
            !CloseHandle(racing_thread)
        )
            return 19;
#else
        if (pthread_join(racing_thread, NULL) != 0)
            return 19;
#endif

        if (
            !((race.outcome.state == 0 && race.outcome.payload == 97) ||
            (race.outcome.state == 3 && race.outcome.payload == 4))
        )
            return 20;

        if (shutdown == 4 && bray_runtime_structured_shutdown() != 0)
            return 21;
    }

    if (bray_runtime_structured_shutdown() != 1)
        return 22;

    if (
        cleanup_shield_depth != 0 ||
        atomic_load_explicit(&cleanup_shield_entries, memory_order_relaxed) == 0 ||
        atomic_load_explicit(&cleanup_shield_entries, memory_order_relaxed) !=
            atomic_load_explicit(&cleanup_shield_exits, memory_order_relaxed)
    )
        return 23;

    return 0;
}
