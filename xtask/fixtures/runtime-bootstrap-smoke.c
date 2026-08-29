#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
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

typedef struct CleanupRegistration {
    void *callback;
    uintptr_t context;
} CleanupRegistration;

typedef struct PlatformStatus {
    uint32_t category;
    uint32_t reserved;
    int64_t native_code;
} PlatformStatus;

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);
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
extern uint32_t bray_runtime_structured_shutdown(void);

static _Atomic uint32_t thread_ready;
static _Atomic uint32_t thread_release;
static uint32_t cleanup_order[3];
static uint32_t cleanup_count;

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

static void cleanup_callback(uintptr_t context, RunOutcome *outcome) {
    cleanup_order[cleanup_count++] = (uint32_t)context;

    if (context == 2) {
        panicked_callback(context, outcome);
        return;
    }

    completed_callback(context, outcome);
}

static int exercise_thread_attachment(void) {
    uint64_t identity = bray_runtime_thread_attachment_identity(NULL);

    if (identity == 0 || bray_runtime_thread_attachment_identity(NULL) != identity) {
        return 1;
    }

    for (uintptr_t context = 1; context <= 3; context += 1) {
        CleanupRegistration registration = {(void *)&cleanup_callback, context};

        if (bray_runtime_thread_static_cleanup_registration(&registration) != 0) {
            return 2;
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
#else
static void *thread_entry(void *context) {
    (void)context;
    return (void *)(uintptr_t)exercise_thread_attachment();
}
#endif

static PlatformStatus platform_success(void) {
    PlatformStatus status = {0, 0, 0};
    return status;
}

static PlatformStatus platform_failure(int64_t native_code) {
    PlatformStatus status = {1, 0, native_code};
    return status;
}

PlatformStatus bray_platform_thread_storage_create(void *destructor, uint64_t *key) {
#if defined(_WIN32)
    DWORD native_key = FlsAlloc((PFLS_CALLBACK_FUNCTION)destructor);

    if (native_key == FLS_OUT_OF_INDEXES) {
        return platform_failure((int64_t)GetLastError());
    }

    *key = (uint64_t)native_key;
#else
    pthread_key_t native_key;
    int result = pthread_key_create(&native_key, (void (*)(void *))destructor);

    if (result != 0) {
        return platform_failure((int64_t)result);
    }

    *key = (uint64_t)native_key;
#endif

    return platform_success();
}

PlatformStatus bray_platform_thread_storage_load(uint64_t key, void **value) {
#if defined(_WIN32)
    if (key > UINT32_MAX) {
        return platform_failure(-1);
    }

    SetLastError(ERROR_SUCCESS);
    *value = FlsGetValue((DWORD)key);

    if (*value == NULL && GetLastError() != ERROR_SUCCESS) {
        return platform_failure((int64_t)GetLastError());
    }
#else
    *value = pthread_getspecific((pthread_key_t)key);
#endif

    return platform_success();
}

PlatformStatus bray_platform_thread_storage_store(uint64_t key, void *value) {
#if defined(_WIN32)
    if (key > UINT32_MAX) {
        return platform_failure(-1);
    }

    if (!FlsSetValue((DWORD)key, value)) {
        return platform_failure((int64_t)GetLastError());
    }
#else
    int result = pthread_setspecific((pthread_key_t)key, value);

    if (result != 0) {
        return platform_failure((int64_t)result);
    }
#endif

    return platform_success();
}

PlatformStatus bray_platform_thread_storage_destroy(uint64_t key) {
#if defined(_WIN32)
    if (key > UINT32_MAX) {
        return platform_failure(-1);
    }

    if (!FlsFree((DWORD)key)) {
        return platform_failure((int64_t)GetLastError());
    }
#else
    int result = pthread_key_delete((pthread_key_t)key);

    if (result != 0) {
        return platform_failure((int64_t)result);
    }
#endif

    return platform_success();
}

void bray_runtime_current_run_cancellation_propagation(void) {
    abort();
}

void bray_runtime_panic_propagation(uintptr_t report) {
    (void)report;
    abort();
}

void bray_runtime_product_host_control(void) {
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

    RunOutcome foreign = bray_runtime_foreign_callback_execution(
        (void *)&completed_callback,
        73
    );

    if (foreign.state != 0 || foreign.payload != 73) {
        return 6;
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

    if (
        cleanup_count != 3
        || cleanup_order[0] != 3
        || cleanup_order[1] != 2
        || cleanup_order[2] != 1
    ) {
        return 11;
    }

    if (bray_runtime_structured_shutdown() != 0 || bray_runtime_structured_shutdown() != 1) {
        return 12;
    }

    return 0;
}
