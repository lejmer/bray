#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#define FlsAlloc SystemFlsAlloc
#define FlsFree SystemFlsFree
#include <windows.h>
#undef FlsAlloc
#undef FlsFree
#else
#include <pthread.h>
#include <sched.h>
#endif

static atomic_bool reject_allocations;
static atomic_uint allocation_attempts;
static atomic_uint live_allocations;
static atomic_size_t allocated_bytes;
static atomic_bool reenter_allocation;
static atomic_uint recursive_attachment_status;

#ifdef _WIN32
static atomic_bool reject_key_creation;
static atomic_bool reject_key_destruction;
static atomic_uint key_creation_attempts;
static atomic_uint key_destruction_attempts;
static atomic_uint live_keys;

DWORD WINAPI FlsAlloc(PFLS_CALLBACK_FUNCTION callback)
{
    atomic_fetch_add(&key_creation_attempts, 1);

    if (atomic_load(&reject_key_creation))
    {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FLS_OUT_OF_INDEXES;
    }

    DWORD (WINAPI *allocate)(PFLS_CALLBACK_FUNCTION) =
        (DWORD (WINAPI *)(PFLS_CALLBACK_FUNCTION))GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "FlsAlloc");

    DWORD key = allocate(callback);

    if (key != FLS_OUT_OF_INDEXES)
        atomic_fetch_add(&live_keys, 1);

    return key;
}

BOOL WINAPI FlsFree(DWORD key)
{
    atomic_fetch_add(&key_destruction_attempts, 1);

    if (atomic_load(&reject_key_destruction))
    {
        SetLastError(ERROR_BUSY);
        return FALSE;
    }

    BOOL (WINAPI *release)(DWORD) =
        (BOOL (WINAPI *)(DWORD))GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "FlsFree");

    BOOL released = release(key);

    if (released)
        atomic_fetch_sub(&live_keys, 1);

    return released;
}

DWORD (WINAPI *__imp_FlsAlloc)(PFLS_CALLBACK_FUNCTION) = FlsAlloc;
BOOL (WINAPI *__imp_FlsFree)(DWORD) = FlsFree;
#endif

extern uint32_t bray_runtime_thread_attachment_enter(void);
extern uint32_t bray_runtime_thread_attachment_leave(void);
extern uint32_t bray_runtime_thread_attachment_cleanup(uint32_t (*callback)(uint64_t));
extern uint32_t bray_runtime_thread_attachment_mark_main(void);
extern uint64_t bray_runtime_current_native_thread_identity(void);
extern uint64_t bray_runtime_main_native_thread_identity(void);

static void* allocate_report_memory(size_t bytes, size_t alignment)
{
    atomic_fetch_add(&allocation_attempts, 1);

    if (atomic_exchange(&reenter_allocation, 0))
        atomic_store(&recursive_attachment_status, bray_runtime_thread_attachment_enter());

    if (atomic_load(&reject_allocations))
        return NULL;

    if (alignment > _Alignof(max_align_t))
        abort();

    void* allocation = malloc(bytes);

    if (allocation != NULL)
    {
        atomic_fetch_add(&live_allocations, 1);
        atomic_fetch_add(&allocated_bytes, bytes);
    }

    return allocation;
}

#ifdef _WIN32
void* _aligned_malloc(size_t bytes, size_t alignment)
{
    return allocate_report_memory(bytes, alignment);
}

void _aligned_free(void* memory)
{
    if (memory != NULL)
        atomic_fetch_sub(&live_allocations, 1);

    free(memory);
}
#else
void* aligned_alloc(size_t alignment, size_t bytes)
{
    return allocate_report_memory(bytes, alignment);
}
#endif

typedef struct ProductHostObservation
{
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

typedef struct SourceAnchor
{
    uint32_t present;
    uint32_t source;
    uint32_t start;
    uint32_t end;
    uint64_t version;
    uint8_t namespace[32];
} SourceAnchor;

typedef struct PanicReport PanicReport;
typedef struct RunOutcome RunOutcome;

struct PanicReport
{
    SourceAnchor source;
    uint32_t cause;
    uintptr_t message;
    uintptr_t message_length;
    uint32_t (*copy_message)(uintptr_t, uintptr_t, uint8_t*, uintptr_t);
    void (*release_message)(uintptr_t, uintptr_t, RunOutcome*);
    uintptr_t head;
    uintptr_t tail;
    uintptr_t count;
    uintptr_t reserved;
    uint32_t (*consume)(PanicReport*, _Bool);
};

struct RunOutcome
{
    uint32_t state;
    uintptr_t payload;
    PanicReport report;
};

_Static_assert(sizeof(PanicReport) == 136, "native report layout");
_Static_assert(sizeof(RunOutcome) == 152, "native outcome layout");

extern uint32_t bray_runtime_initialization(uintptr_t worker_capacity, uintptr_t timer_capacity);

static uint32_t substrate_initialized;
static atomic_uint cleanup_shield_balance;

// The isolated bootstrap host has no cancellation delivery. Check that cleanup shields balance.
void bray_runtime_cleanup_shield_enter(void)
{
    atomic_fetch_add_explicit(&cleanup_shield_balance, 1, memory_order_relaxed);
}

void bray_runtime_cleanup_shield_leave(void)
{
    if (atomic_fetch_sub_explicit(&cleanup_shield_balance, 1, memory_order_relaxed) == 0)
        abort();
}

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

ProductHostObservation bray_runtime_product_host_control(void* descriptor, uint32_t operation)
{
    (void)descriptor;
    (void)operation;

    ProductHostObservation observation = {0};

    return observation;
}

void bray_runtime_current_run_cancellation_propagation(void)
{
    abort();
}

// Reporting remains a host service. The Bray provider owns consumption and release.
typedef struct PanicPrimary
{
    SourceAnchor source;
    uint32_t cause;
    uintptr_t message;
    uintptr_t message_length;
    uint32_t (*copy_message)(uintptr_t, uintptr_t, uint8_t*, uintptr_t);
    void (*release_message)(uintptr_t, uintptr_t, RunOutcome*);
} PanicPrimary;

uint32_t bray_runtime_substrate_report_primary(const PanicPrimary* primary)
{
    return primary->cause > 4 ||
        primary->source.present > 1 ||
        primary->source.start > primary->source.end ||
        (primary->message_length != 0 && primary->message == 0) ? 3 : 0;
}

uint32_t bray_runtime_panic_reporting(PanicReport* report)
{
    return report->consume == NULL ? 0 : report->consume(report, 1);
}

uint32_t bray_runtime_panic_report_destruction(PanicReport* report)
{
    return report->consume == NULL ? 0 : report->consume(report, 0);
}

extern PanicReport bray_runtime_panic_report_construction(
    uint32_t cause,
    uint32_t source_present,
    uint32_t source_identity,
    uint32_t source_start,
    uint32_t source_end,
    uint64_t source_version,
    uint64_t source_namespace_word_0,
    uint64_t source_namespace_word_1,
    uint64_t source_namespace_word_2,
    uint64_t source_namespace_word_3,
    const uint8_t* message,
    uintptr_t message_length
);

extern uint32_t bray_runtime_structured_shutdown(void);
extern void bray_runtime_outgoing_admission(uintptr_t count, RunOutcome* outcome);
extern uintptr_t bray_runtime_outgoing_activation(void);
extern void bray_runtime_outgoing_retirement(uintptr_t record, RunOutcome* outcome);
extern void bray_runtime_outgoing_discharge(uintptr_t count);
extern PanicReport bray_runtime_panic_report_suppression(PanicReport* primary, PanicReport* incident);

typedef uint32_t (*ReportConsumer)(PanicReport*, _Bool);

extern ReportConsumer bray_runtime_report_consumer(void);

static PanicReport* reentrant_report;
static unsigned reentrant_releases;

static void reentrant_release(uintptr_t message, uintptr_t length, RunOutcome* outcome)
{
    (void)message;
    (void)length;
    (void)outcome;

    ++reentrant_releases;

    if (bray_runtime_panic_report_destruction(reentrant_report) != 0)
        abort();

    RunOutcome admitted = {0};

    bray_runtime_outgoing_admission(1, &admitted);

    if (admitted.state != 0)
        abort();

    uintptr_t record = bray_runtime_outgoing_activation();

    bray_runtime_outgoing_retirement(record, &admitted);
    bray_runtime_outgoing_discharge(1);
}

static PanicReport panic_report(void)
{
    static const uint8_t message[] = "bootstrap panic";

    return bray_runtime_panic_report_construction(
        0,
        0,
        0,
        0,
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

void bray_runtime_panic_propagation(PanicReport* report)
{
    (void)report;

    abort();
}

static int consume_after_allocator_failure(void)
{
    RunOutcome admission = {0};

    bray_runtime_outgoing_admission(2, &admission);

    if (admission.state != 0)
        return 10;

    uintptr_t first = bray_runtime_outgoing_activation();
    uintptr_t second = bray_runtime_outgoing_activation();

    atomic_store(&reject_allocations, 1);

    for (unsigned index = 0; index < 8; ++index)
    {
        RunOutcome rejected = {0};

        bray_runtime_outgoing_admission(64, &rejected);

        if (rejected.state != 2 || rejected.report.cause != 4)
            return 11;

        bray_runtime_panic_report_destruction(&rejected.report);
    }

    RunOutcome primary = {.state = 2, .report = panic_report()};
    RunOutcome incident = {.state = 2, .report = panic_report()};

    if (primary.report.cause != 4 || incident.report.cause != 4)
        return 12;

    unsigned before = atomic_load(&allocation_attempts);

    bray_runtime_outgoing_retirement(first, &primary);
    bray_runtime_outgoing_retirement(second, &incident);
    bray_runtime_outgoing_discharge(2);

    PanicReport report = bray_runtime_panic_report_suppression(&primary.report, &incident.report);

    if (
        bray_runtime_panic_reporting(&report) != 0 ||
        bray_runtime_panic_report_destruction(&report) != 0 ||
        before != atomic_load(&allocation_attempts)
    )
        return 13;

    PanicReport reentrant = {.release_message = reentrant_release, .consume = bray_runtime_report_consumer()};

    reentrant_report = &reentrant;

    if (
        bray_runtime_panic_report_destruction(&reentrant) != 0 ||
        reentrant_releases != 1 ||
        before != atomic_load(&allocation_attempts)
    )
        return 14;

    reentrant_report = NULL;
    atomic_store(&reject_allocations, 0);

    return 0;
}

static atomic_uint attachment_cleanup_calls;
static atomic_uint attachment_cleanup_failures;
static uint64_t previous_attachment;
static _Thread_local uint64_t cleanup_attachment;

#ifdef _WIN32
static _Thread_local DWORD attachment_native_thread;
#else
static _Thread_local pthread_t attachment_native_thread;
#endif

static uint32_t attachment_cleanup(uint64_t identity)
{
    int same_thread;

#ifdef _WIN32
    same_thread = attachment_native_thread == GetCurrentThreadId();
#else
    same_thread = pthread_equal(attachment_native_thread, pthread_self());
#endif

    if (
        !same_thread || identity == 0 || identity != cleanup_attachment ||
        bray_runtime_current_native_thread_identity() != 0 ||
        bray_runtime_thread_attachment_enter() != 4
    )
        atomic_fetch_add(&attachment_cleanup_failures, 1);

    atomic_fetch_add(&attachment_cleanup_calls, 1);
    return 1;
}

static int attach_with_cleanup(void)
{
    uint32_t entered = bray_runtime_thread_attachment_enter();

    if (entered != 0)
    {
        fprintf(stderr, "attachment enter failed: status=%u allocations=%u recursive_status=%u\n",
            entered, atomic_load(&allocation_attempts), atomic_load(&recursive_attachment_status));
        return 20;
    }

    uint64_t identity = bray_runtime_current_native_thread_identity();

    if (identity == 0 || identity == previous_attachment)
        return 21;

    previous_attachment = identity;
    cleanup_attachment = identity;

#ifdef _WIN32
    attachment_native_thread = GetCurrentThreadId();
#else
    attachment_native_thread = pthread_self();
#endif

    if (
        bray_runtime_thread_attachment_enter() != 0 ||
        bray_runtime_current_native_thread_identity() != identity ||
        bray_runtime_thread_attachment_cleanup(attachment_cleanup) != 0 ||
        bray_runtime_thread_attachment_cleanup(attachment_cleanup) != 4
    )
        return 22;

    return 0;
}

#ifdef _WIN32
static DWORD WINAPI attached_native_thread(void* context)
#else
static void* attached_native_thread(void* context)
#endif
{
    int* status = context;

    *status = attach_with_cleanup();
    return 0;
}

static int check_primitive_attachment(void)
{
#ifdef _WIN32
    unsigned initial_live_allocations = atomic_load(&live_allocations);
    atomic_store(&reject_key_creation, 1);

    if (bray_runtime_thread_attachment_enter() != 4 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&key_creation_attempts) != 1 ||
        atomic_load(&live_keys) != 0 ||
        atomic_load(&live_allocations) != initial_live_allocations)
        return 33;

    atomic_store(&reject_key_creation, 0);
#endif

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_thread_attachment_enter() != 4 || bray_runtime_current_native_thread_identity() != 0)
        return 23;

    atomic_store(&reject_allocations, 0);
    atomic_store(&reenter_allocation, 1);
    unsigned startup_allocations = atomic_load(&allocation_attempts);
    size_t startup_bytes = atomic_load(&allocated_bytes);

    if (bray_runtime_thread_attachment_enter() != 0)
        return 29;

    printf("bootstrap startup: allocations=%u bytes=%zu\n",
        atomic_load(&allocation_attempts) - startup_allocations,
        atomic_load(&allocated_bytes) - startup_bytes);

    unsigned empty_exit_allocations = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);

    if (bray_runtime_thread_attachment_leave() != 0 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&allocation_attempts) != empty_exit_allocations)
        return 30;

    atomic_store(&reject_allocations, 0);

    if (bray_runtime_thread_attachment_enter() != 0)
        return 31;

    uint64_t rejected_owner_identity = bray_runtime_current_native_thread_identity();
    atomic_store(&reject_allocations, 1);
    uint32_t rejected_registration = bray_runtime_thread_attachment_cleanup(attachment_cleanup);

    if (rejected_registration != 4 ||
        bray_runtime_current_native_thread_identity() != rejected_owner_identity ||
        bray_runtime_thread_attachment_leave() != 0 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&attachment_cleanup_calls) != 0)
    {
        fprintf(stderr, "failed cleanup registration changed attachment state: status=%u\n", rejected_registration);
        return 32;
    }

    atomic_store(&reject_allocations, 0);

    int status = attach_with_cleanup();

    if (status != 0)
        return status;

    if (
        atomic_load(&recursive_attachment_status) != 4 ||
        bray_runtime_thread_attachment_mark_main() != 0 ||
        bray_runtime_main_native_thread_identity() != previous_attachment ||
        bray_runtime_thread_attachment_leave() != 0 ||
        bray_runtime_current_native_thread_identity() != previous_attachment
    )
        return 24;

    unsigned before = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);

    for (unsigned lookup = 0; lookup < 1000; ++lookup)
        if (bray_runtime_current_native_thread_identity() != previous_attachment)
            return 25;

    if (
        before != atomic_load(&allocation_attempts) ||
        bray_runtime_thread_attachment_leave() != 1 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        bray_runtime_main_native_thread_identity() != 0 ||
        atomic_load(&attachment_cleanup_calls) != 1
    )
        return 26;

    atomic_store(&reject_allocations, 0);

    status = 0;

#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, attached_native_thread, &status, 0, NULL);

    if (thread == NULL || WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0)
        return 27;

    CloseHandle(thread);
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, attached_native_thread, &status) != 0 || pthread_join(thread, NULL) != 0)
        return 27;
#endif

    if (
        status != 0 || atomic_load(&attachment_cleanup_calls) != 2 ||
        atomic_load(&attachment_cleanup_failures) != 0 ||
        before != atomic_load(&allocation_attempts)
    )
    {
        fprintf(stderr, "native attachment exit failed: status=%d cleanups=%u failures=%u allocations=%u->%u\n",
            status, atomic_load(&attachment_cleanup_calls), atomic_load(&attachment_cleanup_failures),
            before, atomic_load(&allocation_attempts));
        return 28;
    }

    return 0;
}

enum { concurrent_threads = 8 };
static atomic_uint concurrent_ready;
static atomic_bool concurrent_release;

typedef struct ConcurrentAttachment
{
    uint64_t identity;
    int status;
} ConcurrentAttachment;

#ifdef _WIN32
static DWORD WINAPI concurrent_attachment(void* context)
#else
static void* concurrent_attachment(void* context)
#endif
{
    ConcurrentAttachment* attachment = context;

#ifdef _WIN32
    attachment_native_thread = GetCurrentThreadId();
#else
    attachment_native_thread = pthread_self();
#endif

    attachment->status = bray_runtime_thread_attachment_enter();
    attachment->identity = bray_runtime_current_native_thread_identity();
    cleanup_attachment = attachment->identity;

    if (attachment->status == 0)
        attachment->status = bray_runtime_thread_attachment_cleanup(attachment_cleanup);

    atomic_fetch_add_explicit(&concurrent_ready, 1, memory_order_release);

    while (!atomic_load_explicit(&concurrent_release, memory_order_acquire))
    {
#ifdef _WIN32
        Sleep(0);
#else
        sched_yield();
#endif
    }

    if (bray_runtime_current_native_thread_identity() != attachment->identity)
        attachment->status = 1;

    return 0;
}

static int check_concurrent_attachments(void)
{
    ConcurrentAttachment attachments[concurrent_threads] = {0};
    unsigned before = atomic_load(&attachment_cleanup_calls);

#ifdef _WIN32
    HANDLE threads[concurrent_threads];
#else
    pthread_t threads[concurrent_threads];
#endif

    for (unsigned index = 0; index < concurrent_threads; ++index)
    {
#ifdef _WIN32
        threads[index] = CreateThread(NULL, 0, concurrent_attachment, &attachments[index], 0, NULL);

        if (threads[index] == NULL)
            return 36;
#else
        if (pthread_create(&threads[index], NULL, concurrent_attachment, &attachments[index]) != 0)
            return 36;
#endif
    }

    while (atomic_load_explicit(&concurrent_ready, memory_order_acquire) != concurrent_threads)
    {
#ifdef _WIN32
        Sleep(0);
#else
        sched_yield();
#endif
    }

    int status = bray_runtime_structured_shutdown() == 4 ? 0 : 37;

    for (unsigned index = 0; index < concurrent_threads; ++index)
    {
        if (attachments[index].status != 0 || attachments[index].identity == 0)
            status = 38;

        for (unsigned other = 0; other < index; ++other)
            if (attachments[index].identity == attachments[other].identity)
                status = 39;
    }

    atomic_store_explicit(&concurrent_release, 1, memory_order_release);

    for (unsigned index = 0; index < concurrent_threads; ++index)
    {
#ifdef _WIN32
        if (WaitForSingleObject(threads[index], INFINITE) != WAIT_OBJECT_0)
            status = 40;

        CloseHandle(threads[index]);
#else
        if (pthread_join(threads[index], NULL) != 0)
            status = 40;
#endif

        if (attachments[index].status != 0)
            status = 38;
    }

    if (atomic_load(&attachment_cleanup_calls) != before + concurrent_threads ||
        atomic_load(&attachment_cleanup_failures) != 0)
        status = 41;

    return status;
}

static int check_pool_retirement(void)
{
    if (bray_runtime_initialization(1, 1) != 0)
        return 42;

    RunOutcome outcome = {0};

    bray_runtime_outgoing_admission(1, &outcome);

    if (outcome.state != 0)
        return 43;

    uintptr_t record = bray_runtime_outgoing_activation();
    unsigned before = atomic_load(&allocation_attempts);

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_structured_shutdown() != 4)
        return 44;

    bray_runtime_outgoing_retirement(record, &outcome);
    bray_runtime_outgoing_discharge(1);

    if (bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&allocation_attempts) != before || atomic_load(&live_allocations) != 0)
        return 45;

    atomic_store(&reject_allocations, 0);

    // A new epoch must allocate a usable pool after the previous one released its backing.
    if (bray_runtime_initialization(1, 1) != 0)
        return 46;

    bray_runtime_outgoing_admission(1, &outcome);

    if (outcome.state != 0)
        return 47;

    record = bray_runtime_outgoing_activation();
    bray_runtime_outgoing_retirement(record, &outcome);
    bray_runtime_outgoing_discharge(1);

    if (bray_runtime_structured_shutdown() != 0 || atomic_load(&live_allocations) != 0)
        return 48;

    return 0;
}

extern void* bray_runtime_cleanup_storage_admission(uintptr_t bytes, uintptr_t alignment);
extern void bray_runtime_cleanup_storage_release(void* pointer, uintptr_t bytes, uintptr_t alignment);

static int check_cleanup_backing(void)
{
    unsigned live = atomic_load(&live_allocations);
    unsigned char* backing = bray_runtime_cleanup_storage_admission(65537, 8);

    if (backing == NULL || (uintptr_t)backing % 8 != 0)
        return 60;

    backing[0] = 7;
    backing[65536] = 9;
    atomic_store(&reject_allocations, 1);

    if (bray_runtime_cleanup_storage_admission(64, 8) != NULL)
        return 61;

    unsigned attempts = atomic_load(&allocation_attempts);

    if (backing[0] != 7 || backing[65536] != 9)
        return 62;

    bray_runtime_cleanup_storage_release(backing, 65537, 8);
    atomic_store(&reject_allocations, 0);

    if (atomic_load(&allocation_attempts) != attempts)
        return 63;

#ifdef _WIN32
    if (atomic_load(&live_allocations) != live)
        return 64;
#else
    (void)live;
#endif

    return 0;
}

int main(void)
{
    int backing_status = check_cleanup_backing();

    if (backing_status != 0)
        return backing_status;

    int attachment_status = check_primitive_attachment();

    if (attachment_status != 0)
        return attachment_status;

    int concurrent_status = check_concurrent_attachments();

    if (concurrent_status != 0)
        return concurrent_status;

#ifdef _WIN32
    atomic_store(&reject_key_destruction, 1);

    if (bray_runtime_structured_shutdown() != 4 ||
        atomic_load(&key_destruction_attempts) != 1 ||
        atomic_load(&live_keys) != 1 ||
        bray_runtime_thread_attachment_enter() != 0 ||
        bray_runtime_current_native_thread_identity() == 0 ||
        bray_runtime_thread_attachment_leave() != 0)
        return 34;

    atomic_store(&reject_key_destruction, 0);

    if (bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&key_destruction_attempts) != 2 ||
        atomic_load(&live_keys) != 0)
        return 35;
#endif

    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2)
        return 1;

    int allocation_status = consume_after_allocator_failure();

    if (allocation_status != 0)
        return allocation_status;

    PanicReport pending = panic_report();

    if (bray_runtime_structured_shutdown() != 4)
        return 2;

    if (bray_runtime_panic_reporting(&pending) != 0)
        return 3;

    PanicReport handled = panic_report();

    if (bray_runtime_panic_report_destruction(&handled) != 0)
        return 4;

    if (bray_runtime_structured_shutdown() != 1 || bray_runtime_structured_shutdown() != 1)
        return 5;

    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_structured_shutdown() != 0)
        return 6;

    int retirement_status = check_pool_retirement();

    if (retirement_status != 0)
        return retirement_status;

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0)
        return 7;

#ifdef _WIN32
    if (atomic_load(&live_keys) != 0 ||
        atomic_load(&key_creation_attempts) != atomic_load(&key_destruction_attempts))
        return 36;
#endif

    return 0;
}
