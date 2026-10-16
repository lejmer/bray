#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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
static atomic_bool reenter_deallocation;
static unsigned deallocation_reentries;
static unsigned deallocation_reentry_status;
static void reenter_pool_on_release(void);
static atomic_uint recursive_attachment_status;
static atomic_uint recursive_attachment_leave_status;
extern uint32_t bray_runtime_thread_attachment_enter(void);
extern uint32_t bray_runtime_thread_attachment_leave(void);

#ifdef _WIN32
static atomic_bool reject_key_creation;
static atomic_bool reject_key_destruction;
static atomic_uint key_creation_attempts;
static atomic_uint key_destruction_attempts;
static atomic_uint live_keys;
static atomic_bool reenter_key_creation;
static atomic_uint recursive_key_status;

DWORD WINAPI FlsAlloc(PFLS_CALLBACK_FUNCTION callback)
{
    atomic_fetch_add(&key_creation_attempts, 1);

    if (atomic_load(&reject_key_creation))
    {
        SetLastError(ERROR_NOT_ENOUGH_MEMORY);
        return FLS_OUT_OF_INDEXES;
    }

    if (atomic_exchange(&reenter_key_creation, 0))
        atomic_store(&recursive_key_status, bray_runtime_thread_attachment_enter());

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
    {
        uint32_t status = bray_runtime_thread_attachment_enter();
        atomic_store(&recursive_attachment_status, status);

        if (status == 0)
            atomic_store(&recursive_attachment_leave_status, bray_runtime_thread_attachment_leave());
    }

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
    {
        atomic_fetch_sub(&live_allocations, 1);
        if (atomic_exchange(&reenter_deallocation, 0))
            reenter_pool_on_release();
    }

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
    uint32_t (*consume)(PanicReport*, uint32_t);
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

extern uint32_t bray_runtime_panic_reporting(PanicReport*);
extern uint32_t bray_runtime_panic_report_destruction(PanicReport*);

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
typedef struct { uintptr_t records; const void* descriptors; uintptr_t count; } CleanupBundle;

extern void bray_runtime_outgoing_admission(const CleanupBundle* bundle, RunOutcome* outcome);
extern uintptr_t bray_runtime_outgoing_activation(const void* descriptor, void* lease);
extern void bray_runtime_outgoing_retirement(uintptr_t record, const void* lease, RunOutcome* outcome);
extern void bray_runtime_outgoing_discharge(const CleanupBundle* bundle);

static void reenter_pool_on_release(void)
{
    CleanupBundle empty = { 0 };
    RunOutcome outcome = { 0 };
    bray_runtime_outgoing_admission(&empty, &outcome);
    deallocation_reentries += 1;
    deallocation_reentry_status = outcome.state;
    if (outcome.state == 0)
        bray_runtime_outgoing_discharge(&empty);
}

extern PanicReport bray_runtime_panic_report_suppression(PanicReport* primary, PanicReport* incident);

typedef uint32_t (*ReportConsumer)(PanicReport*, uint32_t);

extern uint32_t bray_runtime_report_consumer(PanicReport*, uint32_t);

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

    bray_runtime_outgoing_admission(&(CleanupBundle){ 1, NULL, 0 }, &admitted);

    if (admitted.state != 0)
        abort();

    uintptr_t record = bray_runtime_outgoing_activation(NULL, NULL);

    bray_runtime_outgoing_retirement(record, NULL, &admitted);
    bray_runtime_outgoing_discharge(&(CleanupBundle){ 1, NULL, 0 });
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

static unsigned streamed_releases;
static unsigned streamed_chunks;

static uint32_t copy_streamed_message(uintptr_t message, uintptr_t offset, uint8_t* destination, uintptr_t length)
{
    if (length > 512 || offset + length > 16385) abort();
    memcpy(destination, (const uint8_t*)message + offset, length);
    ++streamed_chunks;
    return 0;
}

static void release_streamed_message(uintptr_t message, uintptr_t length, RunOutcome* outcome)
{
    (void)message;
    if (length != 16385 || outcome->state != 0) abort();
    ++streamed_releases;
}

extern uint32_t bray_runtime_entry_failure_reporting(uintptr_t, uintptr_t);
typedef struct {
    uint64_t ordinal;
    uint64_t task;
    uint8_t frame[32];
    uint32_t state;
} ReportSegment;
extern uint32_t bray_runtime_report_segment_consumer(ReportSegment*, PanicReport*);

static int stream_without_allocation(void)
{
    uint8_t message[16385];
    for (size_t index = 0; index < sizeof(message); ++index)
        message[index] = (uint8_t)('a' + index % 26);

    PanicReport report = {
        .message = (uintptr_t)message,
        .message_length = sizeof(message),
        .copy_message = copy_streamed_message,
        .release_message = release_streamed_message,
        .consume = bray_runtime_report_consumer,
    };
    unsigned before = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);
    uint32_t status = bray_runtime_panic_reporting(&report);
    // An unknown error object is opaque, even when its address cannot be read.
    uint32_t entry = bray_runtime_entry_failure_reporting(1, SIZE_MAX);
    ReportSegment origin = { .ordinal = 7, .task = 19, .state = 23 };
    memset(origin.frame, 5, sizeof(origin.frame));
    PanicReport cleanup = { .cause = 1, .consume = bray_runtime_report_consumer };
    uint32_t segment = bray_runtime_report_segment_consumer(&origin, &cleanup);
    atomic_store(&reject_allocations, 0);
    if (status != 0 || entry != 0 || segment != 0 || streamed_releases != 1 || streamed_chunks != 33 ||
        atomic_load(&allocation_attempts) != before) return 100;
    return 0;
}

static int consume_after_allocator_failure(void)
{
    RunOutcome admission = {0};

    bray_runtime_outgoing_admission(&(CleanupBundle){ 2, NULL, 0 }, &admission);

    if (admission.state != 0)
        return 10;

    uintptr_t first = bray_runtime_outgoing_activation(NULL, NULL);
    uintptr_t second = bray_runtime_outgoing_activation(NULL, NULL);

    atomic_store(&reject_allocations, 1);

    for (unsigned index = 0; index < 8; ++index)
    {
        RunOutcome rejected = {0};

        bray_runtime_outgoing_admission(&(CleanupBundle){ 64, NULL, 0 }, &rejected);

        if (rejected.state != 2 || rejected.report.cause != 4)
            return 11;

        bray_runtime_panic_report_destruction(&rejected.report);
    }

    RunOutcome primary = {.state = 2, .report = panic_report()};
    RunOutcome incident = {.state = 2, .report = panic_report()};

    if (primary.report.cause != 4 || incident.report.cause != 4)
        return 12;

    unsigned before = atomic_load(&allocation_attempts);

    bray_runtime_outgoing_retirement(first, NULL, &primary);
    bray_runtime_outgoing_retirement(second, NULL, &incident);
    bray_runtime_outgoing_discharge(&(CleanupBundle){ 2, NULL, 0 });

    PanicReport report = bray_runtime_panic_report_suppression(&primary.report, &incident.report);

    if (
        bray_runtime_panic_reporting(&report) != 0 ||
        bray_runtime_panic_report_destruction(&report) != 0 ||
        before != atomic_load(&allocation_attempts)
    )
        return 13;

    PanicReport reentrant = {.release_message = reentrant_release, .consume = bray_runtime_report_consumer};

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
        atomic_load(&live_keys) != 0)
        return 33;

    // Failed acquisition returns every reservation. Free pool capacity remains
    // reusable until shutdown, which must reclaim it without allocating.
    unsigned rollback_allocations = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);
    atomic_store(&reenter_deallocation, 1);

    if (bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&allocation_attempts) != rollback_allocations ||
        atomic_load(&live_allocations) != initial_live_allocations ||
        deallocation_reentries != 1 || deallocation_reentry_status != 0)
        return 33;

    atomic_store(&reject_allocations, 0);

    atomic_store(&reject_key_creation, 0);
#endif

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_thread_attachment_enter() != 4 || bray_runtime_current_native_thread_identity() != 0)
        return 23;

    atomic_store(&reject_allocations, 0);
    atomic_store(&reenter_allocation, 1);
#ifdef _WIN32
    atomic_store(&reenter_key_creation, 1);
#endif
    unsigned startup_allocations = atomic_load(&allocation_attempts);
    size_t startup_bytes = atomic_load(&allocated_bytes);

    if (bray_runtime_thread_attachment_enter() != 0)
        return 29;

    printf("bootstrap startup: allocations=%u bytes=%zu\n",
        atomic_load(&allocation_attempts) - startup_allocations,
        atomic_load(&allocated_bytes) - startup_bytes);

#ifdef _WIN32
    if (atomic_load(&recursive_key_status) != 4)
        return 34;
#endif

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

    // The cumulative allocation byte count exceeds twice the pool's record count.
    // Reserve that much capacity so registration must grow instead of using warm records.
    CleanupBundle held = { atomic_load(&allocated_bytes) + 1, NULL, 0 };
    RunOutcome reserved = { 0 };
    bray_runtime_outgoing_admission(&held, &reserved);
    if (reserved.state != 0) return 32;

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

    bray_runtime_outgoing_discharge(&held);
    atomic_store(&reject_allocations, 0);

    int status = attach_with_cleanup();

    if (status != 0)
        return status;

    if (
        atomic_load(&recursive_attachment_status) != 0 ||
        atomic_load(&recursive_attachment_leave_status) != 0 ||
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

    bray_runtime_outgoing_admission(&(CleanupBundle){ 1, NULL, 0 }, &outcome);

    if (outcome.state != 0)
        return 43;

    uintptr_t record = bray_runtime_outgoing_activation(NULL, NULL);
    unsigned before = atomic_load(&allocation_attempts);

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_structured_shutdown() != 4)
        return 44;

    bray_runtime_outgoing_retirement(record, NULL, &outcome);
    bray_runtime_outgoing_discharge(&(CleanupBundle){ 1, NULL, 0 });

    if (bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&allocation_attempts) != before || atomic_load(&live_allocations) != 0)
        return 45;

    atomic_store(&reject_allocations, 0);

    // A new epoch must allocate a usable pool after the previous one released its backing.
    if (bray_runtime_initialization(1, 1) != 0)
        return 46;

    bray_runtime_outgoing_admission(&(CleanupBundle){ 1, NULL, 0 }, &outcome);

    if (outcome.state != 0)
        return 47;

    record = bray_runtime_outgoing_activation(NULL, NULL);
    bray_runtime_outgoing_retirement(record, NULL, &outcome);
    bray_runtime_outgoing_discharge(&(CleanupBundle){ 1, NULL, 0 });

    if (bray_runtime_structured_shutdown() != 0 || atomic_load(&live_allocations) != 0)
        return 48;

    return 0;
}

typedef struct {
    unsigned char identity[32];
    uintptr_t bytes;
    uintptr_t alignment;
} CleanupShape;

typedef struct {
    uintptr_t domain;
    uintptr_t shape;
    uintptr_t slot;
    uint64_t epoch;
    uintptr_t address;
} CleanupLease;

extern uint32_t bray_runtime_cleanup_storage_admission(const CleanupShape*, CleanupLease*);
extern unsigned char* bray_runtime_cleanup_storage_activation(const CleanupLease*, const CleanupShape*);
extern uint32_t bray_runtime_cleanup_storage_release(CleanupLease*);
extern void bray_runtime_cleanup_storage_claim(const CleanupShape*, CleanupLease*);

static int check_cleanup_backing(void)
{
    CleanupShape shape = { { 1 }, 65537, 4096 };
    CleanupShape wrong = { { 2 }, 65537, 4096 };
    CleanupLease lease = { 0 };

    if (bray_runtime_cleanup_storage_admission(&shape, &lease) != 0)
        return 60;

    CleanupLease occupied = lease;
    if (bray_runtime_cleanup_storage_admission(&shape, &lease) != 3 ||
        memcmp(&occupied, &lease, sizeof(lease)) != 0)
        return 67;

    atomic_store(&reject_allocations, 1);
    unsigned attempts = atomic_load(&allocation_attempts);

    if (bray_runtime_cleanup_storage_activation(&lease, &wrong) != NULL)
        return 61;

    unsigned char* backing = bray_runtime_cleanup_storage_activation(&lease, &shape);

    if (backing == NULL || (uintptr_t)backing % 4096 != 0 ||
        bray_runtime_cleanup_storage_activation(&lease, &shape) != NULL)
        return 62;

    backing[0] = 7;
    backing[65536] = 9;
    CleanupLease stale = lease;

    if (bray_runtime_cleanup_storage_release(&lease) != 0 ||
        bray_runtime_cleanup_storage_release(&lease) != 3 ||
        bray_runtime_cleanup_storage_admission(&shape, &lease) != 0)
        return 63;

    if (bray_runtime_cleanup_storage_activation(&stale, &shape) != NULL ||
        bray_runtime_cleanup_storage_release(&stale) != 3)
        return 64;

    backing = bray_runtime_cleanup_storage_activation(&lease, &shape);

    if (backing == NULL || backing[0] != 7 || backing[65536] != 9 ||
        bray_runtime_cleanup_storage_release(&lease) != 0 ||
        atomic_load(&allocation_attempts) != attempts)
        return 65;

    CleanupLease preserved = lease;

    if (bray_runtime_cleanup_storage_admission(&wrong, &lease) == 0 ||
        memcmp(&preserved, &lease, sizeof(lease)) != 0)
        return 66;

    atomic_store(&reject_allocations, 0);
    return 0;
}

typedef struct { uint32_t affinity; uint32_t lanes; } ActivationState;
typedef struct { uint32_t kind; uint32_t state; uintptr_t payload; PanicReport report; } ActivationProgress;
typedef struct {
    CleanupShape storage;
    uintptr_t header, completion, completion_bytes, completion_align;
    uint32_t states;
    ActivationState (*state)(uintptr_t, uint32_t);
    void (*resume)(ActivationProgress*, uintptr_t);
    void (*cancel)(ActivationProgress*, uintptr_t);
    void (*broadcast)(ActivationProgress*, uintptr_t);
    void (*resolve)(ActivationProgress*, uintptr_t, uint32_t);
    void (*move_completion)(uintptr_t, uintptr_t);
    void (*destroy)(uintptr_t);
} ActivationDescriptor;
typedef struct { CleanupLease lease; const ActivationDescriptor* descriptor; } InactiveActivation;
typedef struct { CleanupLease lease; const ActivationDescriptor* descriptor; uintptr_t context; } ActivationHeaderPrefix;

typedef struct { uintptr_t head, tail, count; } ActivationRecords;
typedef struct {
    CleanupLease lease;
    const ActivationDescriptor* descriptor;
    uintptr_t context, parent, child, current, broadcast_next;
    uint64_t origin, retained_origin;
    uint32_t state, phase, retained_affinity, retained_lanes, broadcasted;
    ActivationProgress progress;
    ActivationRecords incidents, outgoing;
} ActivationHeader;
typedef void (*ActivationCallback)(const ActivationDescriptor*, uintptr_t, uint32_t,
    uintptr_t, ActivationProgress*, ActivationRecords*);
extern uintptr_t bray_runtime_activation_entry(const InactiveActivation*, uint64_t);
extern uint32_t bray_runtime_activation_dispatch(ActivationHeader*, ActivationCallback,
    uint32_t, uintptr_t, ActivationProgress*);

#ifdef BRAY_ACTIVATION_NATIVE_CALLBACK
extern void bray_runtime_activation_callback(const ActivationDescriptor*, uintptr_t, uint32_t,
    uintptr_t, ActivationProgress*, ActivationRecords*);
#endif

static unsigned activation_calls;

static void activation_callback(const ActivationDescriptor* descriptor, uintptr_t context,
    uint32_t operation, uintptr_t argument, ActivationProgress* progress, ActivationRecords* records)
{
    ActivationHeader* header = (ActivationHeader*)context;
    ++activation_calls;
    if (header->descriptor != descriptor || header->outgoing.count != 0 || records->count != 0)
        abort();
    (void)argument;
    header->broadcasted = 1;
    *progress = (ActivationProgress){ .kind = operation == 3 ? 0 : 1, .state = 7 };
}

static int check_activation_dispatch(ActivationHeader* header, ActivationCallback callback)
{
    unsigned attempts = atomic_load(&allocation_attempts);
    activation_calls = 0;
    for (uint32_t operation = 0; operation != 4; ++operation) {
        ActivationProgress progress = { 0 };
        if (bray_runtime_activation_dispatch(header, callback, operation,
                3, &progress) != 0 ||
            header->broadcasted != 1 || header->outgoing.count != 0 ||
            progress.kind != (operation == 3 ? 0 : 1) || progress.state != 7)
            return 85;
    }
    ActivationProgress untouched = { .kind = 13 };
    if (bray_runtime_activation_dispatch(header, callback, 4, 0, &untouched) != 3 ||
        untouched.kind != 13 || activation_calls != 4 || atomic_load(&allocation_attempts) != attempts)
        return 86;
    return 0;
}

extern uint32_t bray_runtime_activation_publication(const InactiveActivation*);

static ActivationState activation_state(uintptr_t context, uint32_t state)
{
    (void)context;
    (void)state;
    return (ActivationState){ 0 };
}

static ActivationState activation_restricted_state(uintptr_t context, uint32_t state)
{
    (void)context;
    (void)state;
    return (ActivationState){ .lanes = 1 };
}

static void activation_complete(ActivationProgress* result, uintptr_t context)
{
    ActivationHeader* header = (ActivationHeader*)context;
    if (header->outgoing.count != 0)
        abort();
    ++activation_calls;
    header->broadcasted = 1;
    *result = (ActivationProgress){ .kind = 1, .state = 7 };
}

static void activation_resolve(ActivationProgress* result, uintptr_t context, uint32_t exit)
{
    if (exit != 3)
        abort();
    activation_complete(result, context);
    result->kind = 0;
}

static void activation_move(uintptr_t context, uintptr_t destination)
{
    (void)context;
    (void)destination;
}

static void activation_destroy(uintptr_t context) { (void)context; }

// Fixture descriptors require no lane capability. Keep the token slot and its
// provider local to this synchronous call, and consume only an accepted token.
static uintptr_t activate_fixture(InactiveActivation* token, uint64_t origin)
{
    if (token->descriptor == NULL || origin == 0 || origin != bray_runtime_current_native_thread_identity())
        return 0;
    ActivationState state = token->descriptor->state(token->lease.address, 0);
    if (state.affinity != 0 || state.lanes != 0)
        return 0;
    InactiveActivation original = *token;
    uintptr_t active = bray_runtime_activation_entry(token, origin);
    if (memcmp(token, &original, sizeof(original)) != 0)
        abort();
    if (active != 0)
        *token = (InactiveActivation){ 0 };
    return active;
}

static int check_activation_publication(void)
{
    const ActivationDescriptor descriptor = {
        .storage = { { 11 }, 1024, 4096 }, .completion_align = 1, .states = 8,
        .state = activation_state, .resume = activation_complete, .cancel = activation_complete,
        .broadcast = activation_complete, .resolve = activation_resolve,
        .move_completion = activation_move, .destroy = activation_destroy,
    };
    CleanupLease lease = { 0 };
    int status = 0;
    if (bray_runtime_thread_attachment_enter() != 0)
        return 92;
    if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0)
        { status = 80; goto cleanup; }
    InactiveActivation token = { lease, &descriptor };
    if (bray_runtime_activation_publication(&token) != 3)
        { status = 81; goto cleanup; }
    unsigned char* bytes = bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);
    if (bytes == NULL)
        { status = 82; goto cleanup; }
    memset(bytes, 0, descriptor.storage.bytes);
    ActivationHeaderPrefix header = { lease, &descriptor, lease.address };
    memcpy(bytes, &header, sizeof(header));

    atomic_store(&reject_allocations, 1);
    unsigned attempts = atomic_load(&allocation_attempts);
    if (bray_runtime_activation_publication(&token) != 0 ||
        bray_runtime_activation_publication(&token) != 3)
        { status = 83; goto cleanup; }
    uint64_t origin = bray_runtime_current_native_thread_identity();
    ActivationDescriptor other_descriptor = descriptor;
    ActivationDescriptor restricted_descriptor = descriptor;
    restricted_descriptor.state = activation_restricted_state;
    ActivationDescriptor overlapping_descriptor = descriptor;
    overlapping_descriptor.completion_bytes = 1;
    InactiveActivation rejected[] = {
        token, token, token,
        { lease, &other_descriptor }, { lease, &overlapping_descriptor },
        { lease, &restricted_descriptor },
    };
    rejected[0].lease.epoch += 1;
    rejected[1].lease.address += 1;
    rejected[2].lease.slot = 0;
    for (unsigned i = 0; i < sizeof(rejected) / sizeof(rejected[0]); ++i) {
        InactiveActivation before = rejected[i];
        if (activate_fixture(&rejected[i], origin) != 0 ||
            memcmp(&before, &rejected[i], sizeof(before)) != 0)
            { status = 90; goto cleanup; }
    }
    InactiveActivation entering = token;
    uint64_t wrong_origin = origin == 1 ? 2 : 1;
    if (activate_fixture(&entering, 0) != 0 || activate_fixture(&entering, wrong_origin) != 0 ||
        memcmp(&entering, &token, sizeof(token)) != 0)
        { status = 89; goto cleanup; }
    uintptr_t active = activate_fixture(&entering, origin);
    if (active != (uintptr_t)bytes || entering.lease.address != 0 || entering.descriptor != NULL)
        { status = 87; goto cleanup; }
    if (activate_fixture(&entering, origin) != 0)
        { status = 91; goto cleanup; }
    InactiveActivation duplicate = token;
    if (activate_fixture(&duplicate, origin) != 0 || memcmp(&duplicate, &token, sizeof(token)) != 0)
        { status = 88; goto cleanup; }
    int dispatch_status = check_activation_dispatch((ActivationHeader*)active, activation_callback);
#ifdef BRAY_ACTIVATION_NATIVE_CALLBACK
    if (dispatch_status == 0)
        dispatch_status = check_activation_dispatch((ActivationHeader*)active, bray_runtime_activation_callback);
#endif
    if (dispatch_status != 0)
        { status = dispatch_status; goto cleanup; }
    if (bray_runtime_cleanup_storage_release(&lease) != 0 ||
        bray_runtime_activation_publication(&token) != 3)
        { status = 83; goto cleanup; }
    if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0 ||
        bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage) == NULL ||
        bray_runtime_activation_publication(&token) != 3 ||
        bray_runtime_cleanup_storage_release(&lease) != 0 ||
        atomic_load(&allocation_attempts) != attempts)
        { status = 84; goto cleanup; }
cleanup:
    atomic_store(&reject_allocations, 0);
    if (lease.slot != 0 && bray_runtime_cleanup_storage_release(&lease) != 0 && status == 0)
        status = 94;
    if (bray_runtime_thread_attachment_leave() != 0 && status == 0)
        status = 95;
    return status;
}

static int check_cleanup_bundle(void)
{
    unsigned admission_attempts = atomic_load(&allocation_attempts);
    size_t admission_bytes = atomic_load(&allocated_bytes);
    CleanupShape shapes[2] = { { { 3 }, 65537, 4096 }, { { 3 }, 65537, 4096 } };
    CleanupBundle bundle = { 2, shapes, 2 };
    RunOutcome outcome = { 0 };
    bray_runtime_outgoing_admission(&bundle, &outcome);
    if (outcome.state != 0)
        return 68;

    printf("cleanup bundle: allocations=%u bytes=%zu payload_bytes=%zu alignment=%zu\n",
        atomic_load(&allocation_attempts) - admission_attempts,
        atomic_load(&allocated_bytes) - admission_bytes, 2 * shapes[0].bytes, shapes[0].alignment);
    atomic_store(&reject_allocations, 1);
    unsigned attempts = atomic_load(&allocation_attempts);
    CleanupLease first = { 0 };
    uintptr_t record = bray_runtime_outgoing_activation(&shapes[0], &first);
    unsigned char* bytes = (unsigned char*)first.address;
    bytes[0] = 11;
    bytes[65536] = 17;
    bray_runtime_outgoing_discharge(&bundle);

    CleanupLease other = { 0 };
    if (bray_runtime_cleanup_storage_admission(&shapes[0], &other) != 0 || other.address == first.address)
        return 69;
    if (bytes[0] != 11 || bytes[65536] != 17 || bray_runtime_cleanup_storage_release(&other) != 0)
        return 70;

    bray_runtime_outgoing_retirement(record, &first, &outcome);
    if (bray_runtime_cleanup_storage_release(&first) != 3 || atomic_load(&allocation_attempts) != attempts)
        return 71;

    bray_runtime_outgoing_admission(&bundle, &outcome);
    if (outcome.state != 0)
        return 73;
    CleanupLease retained = { 0 };
    bray_runtime_cleanup_storage_claim(&shapes[0], &retained);
    bytes = (unsigned char*)retained.address;
    bytes[0] = 23;
    bytes[65536] = 29;
    bray_runtime_outgoing_discharge(&bundle);
    if (bray_runtime_cleanup_storage_release(&first) != 3 || bytes[0] != 23 || bytes[65536] != 29 ||
        bray_runtime_cleanup_storage_release(&retained) != 0 || atomic_load(&allocation_attempts) != attempts)
        return 74;

    CleanupShape invalid[2] = { shapes[0], { { 4 }, 64, 3 } };
    CleanupBundle rejected = { 2, invalid, 2 };
    bray_runtime_outgoing_admission(&rejected, &outcome);
    if (outcome.state != 2 || outcome.report.cause != 4)
        return 72;
    outcome = (RunOutcome){ 0 };
    bray_runtime_outgoing_admission(&bundle, &outcome);
    if (outcome.state != 0)
        return 75;
    bray_runtime_outgoing_discharge(&bundle);
    atomic_store(&reject_allocations, 0);
    return 0;
}

int main(void)
{
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

    int stream_status = stream_without_allocation();
    if (stream_status != 0) return stream_status;

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

    if (bray_runtime_initialization(1, 1) != 0)
        return 76;

    int bundle_status = check_cleanup_bundle();
    if (bundle_status != 0) return bundle_status;
    int backing_status = check_cleanup_backing();

    if (backing_status != 0)
        return backing_status;

    int publication_status = check_activation_publication();
    if (publication_status != 0) return publication_status;

    if (bray_runtime_structured_shutdown() != 0 || atomic_load(&live_allocations) != 0)
        return 77;

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0)
        return 7;

#ifdef _WIN32
    if (atomic_load(&live_keys) != 0 ||
        atomic_load(&key_creation_attempts) != atomic_load(&key_destruction_attempts))
        return 36;
#endif

    return 0;
}
