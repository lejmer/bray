#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(_WIN32)
#include <windows.h>
#else
#include <pthread.h>
#endif

typedef struct RunOutcome {
    uint32_t state;
    uintptr_t payload;
} RunOutcome;

typedef struct ProductHostObservation {
    uint32_t status;
    uint32_t state;
    uintptr_t active_entries;
    uintptr_t external_roots;
    uintptr_t thread_attachments;
    uintptr_t initialized_statics;
    uintptr_t cleaned_statics;
    uintptr_t cleanup_incidents;
    uint8_t last_incident[32];
} ProductHostObservation;

typedef struct SourceAnchor {
    uint32_t present;
    uint32_t source;
    uint32_t start;
    uint32_t end;
    uint64_t version;
} SourceAnchor;

typedef struct CleanupIncident {
    uintptr_t payload;
    uint8_t type_identity[32];
    SourceAnchor source;
    void *report;
    void *destroy;
} CleanupIncident;

typedef struct StaticFinalizer {
    uint32_t execution;
    uint32_t reserved;
    uintptr_t result_size;
    uintptr_t result_alignment;
    void *start;
    void *resolve;
} StaticFinalizer;

typedef struct CleanupRegistration {
    void *product;
    uint8_t static_identity[32];
    void *prepare;
    StaticFinalizer finalizer;
    void *destroy;
    void *detach;
} CleanupRegistration;

typedef struct ShutdownRace {
    RunOutcome outcome;
} ShutdownRace;

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);
static uint32_t substrate_initialized;
static atomic_uint cleanup_shield_balance;

// The isolated bootstrap host has no cancellation delivery. Check that cleanup shields balance.
void bray_runtime_cleanup_shield_enter(void) {
    atomic_fetch_add_explicit(&cleanup_shield_balance, 1, memory_order_relaxed);
}

void bray_runtime_cleanup_shield_leave(void) {
    if (atomic_fetch_sub_explicit(&cleanup_shield_balance, 1, memory_order_relaxed) == 0) {
        abort();
    }
}

uint32_t bray_runtime_substrate_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity) {
    (void)worker_capacity;
    (void)timer_capacity;

    if (substrate_initialized != 0) {
        return 2;
    }

    substrate_initialized = 1;
    return 0;
}

uint32_t bray_runtime_substrate_shutdown(void) {
    if (substrate_initialized == 0) {
        return 1;
    }

    substrate_initialized = 0;
    return 0;
}

ProductHostObservation bray_runtime_substrate_product_host_control(
    void *descriptor,
    uint32_t operation
) {
    (void)descriptor;
    (void)operation;

    ProductHostObservation observation = {0};
    return observation;
}

extern uint64_t bray_runtime_thread_attachment_identity(void *descriptor);
extern uint32_t bray_runtime_thread_static_cleanup_registration(void *registration);

uint64_t bray_runtime_substrate_thread_attachment_identity(void *descriptor) {
    (void)descriptor;
    return bray_runtime_thread_attachment_identity(NULL);
}

uint32_t bray_runtime_substrate_thread_static_cleanup_registration(void *registration) {
    CleanupRegistration bootstrap_registration = *(CleanupRegistration *)registration;
    bootstrap_registration.product = NULL;

    return bray_runtime_thread_static_cleanup_registration(&bootstrap_registration);
}

typedef void (*SynchronousCallback)(uintptr_t context, RunOutcome *outcome);

RunOutcome bray_runtime_substrate_synchronous_root_execution(
    void *callback,
    uintptr_t context,
    void (*cleanup)(void)
) {
    RunOutcome outcome = {3, 4};
    ((SynchronousCallback)callback)(context, &outcome);
    cleanup();
    return outcome;
}

RunOutcome bray_runtime_substrate_foreign_callback_execution(
    void *callback,
    uintptr_t context,
    void (*cleanup)(void)
) {
    RunOutcome outcome = {3, 4};
    ((SynchronousCallback)callback)(context, &outcome);
    cleanup();
    return outcome;
}

uint32_t bray_runtime_substrate_native_thread_execution(
    void *operation,
    uintptr_t context,
    void *cancellation,
    uintptr_t cancellation_context,
    uintptr_t *panic_payload,
    void (*cleanup)(void)
) {
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
    const uint8_t *message,
    uintptr_t message_length
) {
    (void)source_identity;
    (void)source_version;

    if (cause > 2 || source_present > 1 || source_start > source_end) {
        return 3;
    }

    if (message == NULL && message_length != 0) {
        return 3;
    }

    return 0;
}

extern RunOutcome bray_runtime_synchronous_root_execution(void *callback, uintptr_t context);
extern RunOutcome bray_runtime_foreign_callback_execution(void *callback, uintptr_t context);
extern uint64_t bray_runtime_thread_attachment_identity(void *descriptor);
extern uint32_t bray_runtime_thread_static_cleanup_registration(void *registration);
extern uintptr_t bray_runtime_panic_report_construction(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    const uint8_t *message,
    uintptr_t message_length
);
extern uint32_t bray_runtime_panic_reporting(uintptr_t report);
extern uint32_t bray_runtime_panic_report_destruction(uintptr_t report);
extern uint32_t bray_runtime_structured_shutdown(void);
extern uint64_t bray_runtime_bootstrap_thread_static_probe(void);
extern uint64_t bray_runtime_bootstrap_thread_static_cleanup_observation(void);

static _Atomic uint32_t thread_ready;
static _Atomic uint32_t thread_release;
static uint32_t cleanup_order[3];
static uint32_t cleanup_count;
static uint32_t cleanup_incident_reports;
static uint32_t cleanup_incident_destroys;

static void completed_callback(uintptr_t context, RunOutcome *outcome) {
    outcome->state = 0;
    outcome->payload = context;
}

static void cancelled_callback(uintptr_t context, RunOutcome *outcome) {
    (void)context;
    outcome->state = 1;
    outcome->payload = 0;
}

static uintptr_t panic_report(void) {
    static const uint8_t message[] = "bootstrap panic";

    return bray_runtime_panic_report_construction(
        0,
        0,
        0,
        0,
        0,
        0,
        message,
        sizeof(message) - 1
    );
}

static void panicked_callback(uintptr_t context, RunOutcome *outcome) {
    (void)context;
    outcome->state = 2;
    outcome->payload = panic_report();
}

static void static_transition(void) {
}

static uint32_t report_cleanup_incident(uintptr_t payload) {
    if (payload != 2) {
        abort();
    }

    cleanup_incident_reports += 1;
    return 0;
}

static void destroy_cleanup_incident(uintptr_t payload) {
    if (payload != 2) {
        abort();
    }

    cleanup_incident_destroys += 1;
}

static uint32_t record_cleanup(uint32_t ordinal, uintptr_t destination) {
    cleanup_order[cleanup_count++] = ordinal;

    if (ordinal == 2) {
        CleanupIncident incident = {0};
        incident.payload = 2;
        incident.report = (void *)&report_cleanup_incident;
        incident.destroy = (void *)&destroy_cleanup_incident;
        *(CleanupIncident *)destination = incident;

        return 1;
    }

    return 0;
}

static uint32_t first_cleanup(uintptr_t destination) {
    return record_cleanup(1, destination);
}

static uint32_t second_cleanup(uintptr_t destination) {
    return record_cleanup(2, destination);
}

static uint32_t third_cleanup(uintptr_t destination) {
    return record_cleanup(3, destination);
}

static uint32_t resolve_cleanup(uintptr_t completed, uintptr_t destination) {
    (void)completed;
    (void)destination;
    return 0;
}

static CleanupRegistration cleanup_registration(uint8_t identity, void *start) {
    CleanupRegistration registration = {0};
    registration.static_identity[0] = identity;
    registration.prepare = (void *)&static_transition;
    registration.finalizer.execution = 1;
    registration.finalizer.result_size = sizeof(CleanupIncident);
    registration.finalizer.result_alignment = _Alignof(CleanupIncident);
    registration.finalizer.start = start;
    registration.finalizer.resolve = (void *)&resolve_cleanup;
    registration.destroy = (void *)&static_transition;
    registration.detach = (void *)&static_transition;

    return registration;
}

static int exercise_thread_attachment(void) {
    uint64_t identity = bray_runtime_thread_attachment_identity(NULL);

    if (identity == 0 || bray_runtime_thread_attachment_identity(NULL) != identity) {
        return 1;
    }

    if (bray_runtime_bootstrap_thread_static_probe() != 46) {
        return 2;
    }

    void *starts[] = {(void *)&first_cleanup, (void *)&second_cleanup, (void *)&third_cleanup};

    for (uint8_t index = 0; index < 3; index += 1) {
        CleanupRegistration registration = cleanup_registration(index + 1, starts[index]);

        if (bray_runtime_thread_static_cleanup_registration(&registration) != 0) {
            return 3;
        }
    }

    atomic_store_explicit(&thread_ready, 1, memory_order_release);

    while (atomic_load_explicit(&thread_release, memory_order_acquire) == 0) {
    }

    return 0;
}

#if defined(_WIN32)
static DWORD WINAPI thread_entry(LPVOID context) {
    (void)context;
    return (DWORD)exercise_thread_attachment();
}

static DWORD WINAPI shutdown_race_entry(LPVOID context) {
    ShutdownRace *race = (ShutdownRace *)context;
    race->outcome = bray_runtime_synchronous_root_execution((void *)&completed_callback, 97);
    return 0;
}
#else
static void *thread_entry(void *context) {
    (void)context;
    return (void *)(uintptr_t)exercise_thread_attachment();
}

static void *shutdown_race_entry(void *context) {
    ShutdownRace *race = (ShutdownRace *)context;
    race->outcome = bray_runtime_synchronous_root_execution((void *)&completed_callback, 97);
    return NULL;
}
#endif

void bray_runtime_current_run_cancellation_propagation(void) {
    abort();
}

void bray_runtime_panic_propagation(uintptr_t report) {
    (void)report;
    abort();
}

int main(void) {
    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2) {
        return 1;
    }

    RunOutcome completed = bray_runtime_synchronous_root_execution(
        (void *)&completed_callback,
        41
    );

    if (completed.state != 0 || completed.payload != 41) {
        return 2;
    }

    RunOutcome cancelled = bray_runtime_synchronous_root_execution(
        (void *)&cancelled_callback,
        0
    );

    if (cancelled.state != 1 || cancelled.payload != 0) {
        return 3;
    }

    RunOutcome panicked = bray_runtime_synchronous_root_execution(
        (void *)&panicked_callback,
        0
    );

    if (panicked.state != 2 || panicked.payload == 0) {
        return 4;
    }

    if (bray_runtime_panic_reporting(panicked.payload) != 0) {
        return 5;
    }

    if (bray_runtime_panic_report_destruction(panic_report()) != 0) {
        return 6;
    }

    RunOutcome foreign = bray_runtime_foreign_callback_execution(
        (void *)&completed_callback,
        73
    );

    if (foreign.state != 0 || foreign.payload != 73) {
        return 7;
    }

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, thread_entry, NULL, 0, NULL);

    if (thread == NULL) {
        return 7;
    }
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, thread_entry, NULL) != 0) {
        return 7;
    }
#endif

    while (atomic_load_explicit(&thread_ready, memory_order_acquire) == 0) {
    }

    if (bray_runtime_structured_shutdown() != 4) {
        return 8;
    }

    atomic_store_explicit(&thread_release, 1, memory_order_release);

#if defined(_WIN32)
    if (WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0) {
        return 9;
    }

    DWORD thread_result = 0;

    if (!GetExitCodeThread(thread, &thread_result) || thread_result != 0 || !CloseHandle(thread)) {
        return 10;
    }
#else
    void *thread_result = NULL;

    if (pthread_join(thread, &thread_result) != 0 || thread_result != NULL) {
        return 10;
    }
#endif

    if (cleanup_count != 3) {
        return 11;
    }

    if (cleanup_order[0] != 3 || cleanup_order[1] != 2 || cleanup_order[2] != 1) {
        return 13;
    }

    if (cleanup_incident_reports != 1 || cleanup_incident_destroys != 1) {
        return 14;
    }

    uint64_t bootstrap_cleanup = bray_runtime_bootstrap_thread_static_cleanup_observation();

    if (bootstrap_cleanup != 211717) {
        fprintf(stderr, "bootstrap cleanup observation: %llu\n", (unsigned long long)bootstrap_cleanup);
        return 15;
    }

    if (bray_runtime_structured_shutdown() != 0 || bray_runtime_structured_shutdown() != 1) {
        return 12;
    }

    for (uint32_t iteration = 0; iteration < 128; iteration += 1) {
        if (bray_runtime_initialization(1, 1) != 0) {
            return 16;
        }

        ShutdownRace race = {0};

#if defined(_WIN32)
        HANDLE racing_thread = CreateThread(NULL, 0, shutdown_race_entry, &race, 0, NULL);

        if (racing_thread == NULL) {
            return 17;
        }
#else
        pthread_t racing_thread;

        if (pthread_create(&racing_thread, NULL, shutdown_race_entry, &race) != 0) {
            return 17;
        }
#endif

        uint32_t shutdown = bray_runtime_structured_shutdown();

        if (shutdown != 0 && shutdown != 4) {
            return 18;
        }

#if defined(_WIN32)
        if (WaitForSingleObject(racing_thread, INFINITE) != WAIT_OBJECT_0 || !CloseHandle(racing_thread)) {
            return 19;
        }
#else
        if (pthread_join(racing_thread, NULL) != 0) {
            return 19;
        }
#endif

        if (
            !(
                (race.outcome.state == 0 && race.outcome.payload == 97)
                || (race.outcome.state == 3 && race.outcome.payload == 4)
            )
        ) {
            return 20;
        }

        if (shutdown == 4 && bray_runtime_structured_shutdown() != 0) {
            return 21;
        }
    }

    if (bray_runtime_structured_shutdown() != 1) {
        return 22;
    }

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0) {
        return 23;
    }

    return 0;
}
