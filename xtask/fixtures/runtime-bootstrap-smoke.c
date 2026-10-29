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
#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#endif

static atomic_bool reenter_allocation;
static atomic_bool reenter_deallocation;
static unsigned deallocation_reentries;
static unsigned deallocation_reentry_status;
static void reenter_pool_on_release(void);
static atomic_uint recursive_attachment_status;
static atomic_uint recursive_attachment_leave_status;
extern uint32_t bray_runtime_thread_attachment_enter(void);
extern uint32_t bray_runtime_thread_attachment_leave(void);

#if defined(_WIN32) || defined(__linux__)
static atomic_bool reject_key_creation;
static atomic_uint reject_key_creation_at;
static atomic_bool reject_key_destruction;
static atomic_uint key_creation_attempts;
static atomic_uint key_destruction_attempts;
static atomic_uint live_keys;
static atomic_bool reenter_key_creation;
static atomic_uint recursive_key_status;

#ifdef _WIN32
DWORD WINAPI FlsAlloc(PFLS_CALLBACK_FUNCTION callback)
{
    unsigned attempt = atomic_fetch_add(&key_creation_attempts, 1) + 1;

    if (atomic_load(&reject_key_creation) || attempt == atomic_load(&reject_key_creation_at))
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
#else
int pthread_key_create(pthread_key_t* key, void (*destructor)(void*))
{
    unsigned attempt = atomic_fetch_add(&key_creation_attempts, 1) + 1;

    if (atomic_load(&reject_key_creation) || attempt == atomic_load(&reject_key_creation_at))
    {
        return EAGAIN;
    }

    if (atomic_exchange(&reenter_key_creation, 0))
    {
        atomic_store(&recursive_key_status, bray_runtime_thread_attachment_enter());
    }

    int (*create)(pthread_key_t*, void (*)(void*)) =
        (int (*)(pthread_key_t*, void (*)(void*)))dlsym(RTLD_NEXT, "pthread_key_create");

    if (create == NULL)
    {
        abort();
    }

    int status = create(key, destructor);

    if (status == 0)
    {
        atomic_fetch_add(&live_keys, 1);
    }

    return status;
}

int pthread_key_delete(pthread_key_t key)
{
    atomic_fetch_add(&key_destruction_attempts, 1);

    if (atomic_load(&reject_key_destruction))
    {
        return EBUSY;
    }

    int (*release)(pthread_key_t) = (int (*)(pthread_key_t))dlsym(RTLD_NEXT, "pthread_key_delete");

    if (release == NULL)
    {
        abort();
    }

    int status = release(key);

    if (status == 0)
    {
        atomic_fetch_sub(&live_keys, 1);
    }

    return status;
}
#endif
#endif

extern uint32_t bray_runtime_thread_attachment_enter(void);
extern uint32_t bray_runtime_thread_attachment_leave(void);
extern uint32_t bray_runtime_thread_attachment_cleanup(uint32_t (*callback)(uint64_t));
extern uint32_t bray_runtime_thread_attachment_mark_main(void);
extern uint64_t bray_runtime_current_native_thread_identity(void);
extern uint64_t bray_runtime_main_native_thread_identity(void);

static void before_allocation(void)
{
    if (atomic_exchange(&reenter_allocation, 0))
    {
        uint32_t status = bray_runtime_thread_attachment_enter();
        atomic_store(&recursive_attachment_status, status);

        if (status == 0)
            atomic_store(&recursive_attachment_leave_status, bray_runtime_thread_attachment_leave());
    }
}

static void before_release(void)
{
    if (atomic_exchange(&reenter_deallocation, 0))
        reenter_pool_on_release();
}



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
    uint32_t primary_present;
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
typedef struct
{
    uintptr_t records;
    const void* descriptors;
    uintptr_t count;
} CleanupBundle;

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
    if (length > 512 || offset + length > 16385)
        abort();

    memcpy(destination, (const uint8_t*)message + offset, length);

    ++streamed_chunks;

    return 0;
}

static void release_streamed_message(uintptr_t message, uintptr_t length, RunOutcome* outcome)
{
    (void)message;

    if (length != 16385 || outcome->state != 0)
        abort();

    ++streamed_releases;
}

extern uint32_t bray_runtime_entry_failure_reporting(uintptr_t, uintptr_t);

typedef struct
{
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
        .primary_present = 1,
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

    ReportSegment origin = {
        .ordinal = 7,
        .task = 19,
        .state = 23
    };

    memset(origin.frame, 5, sizeof(origin.frame));

    PanicReport cleanup = {
        .cause = 1,
        .primary_present = 1,
        .consume = bray_runtime_report_consumer
    };

    uint32_t segment = bray_runtime_report_segment_consumer(&origin, &cleanup);

    atomic_store(&reject_allocations, 0);

    if (
        status != 0 ||
        entry != 0 ||
        segment != 0 ||
        streamed_releases != 1 ||
        streamed_chunks != 33 ||
        atomic_load(&allocation_attempts) != before
    )
        return 100;

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

        // Earlier attachment cases can leave reusable report capacity. Force
        // growth and prove that this case actually exercises allocator refusal.
        unsigned attempts = atomic_load(&allocation_attempts);
        bray_runtime_outgoing_admission(&(CleanupBundle){ 1024 * 1024, NULL, 0 }, &rejected);

        if (
            rejected.state != 2 || rejected.report.cause != 4 ||
            atomic_load(&allocation_attempts) == attempts
        )
        {
            fprintf(stderr, "allocator failure admission: state=%u cause=%u attempts=%u\n",
                rejected.state, rejected.report.cause, atomic_load(&allocation_attempts) - attempts);
            return 11;
        }

        bray_runtime_panic_report_destruction(&rejected.report);
    }

    RunOutcome primary = {
        .state = 2,
        .report = panic_report()
    };
    RunOutcome incident = {
        .state = 2,
        .report = panic_report()
    };

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

    PanicReport reentrant = {
        .primary_present = 1,
        .release_message = reentrant_release,
        .consume = bray_runtime_report_consumer,
    };

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
        !same_thread ||
        identity == 0 ||
        identity != cleanup_attachment ||
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
        fprintf(
            stderr,
            "attachment enter failed: status=%u allocations=%u recursive_status=%u\n",
            entered,
            atomic_load(&allocation_attempts),
            atomic_load(&recursive_attachment_status)
        );

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

#if defined(_WIN32) || defined(__linux__)
static int check_partial_host_formation(void)
{
    unsigned creation_before = atomic_load(&key_creation_attempts);
    unsigned destruction_before = atomic_load(&key_destruction_attempts);

    atomic_store(&reject_key_creation_at, creation_before + 2);
    atomic_store(&reject_key_destruction, 1);

    if (
        bray_runtime_thread_attachment_enter() != 4 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&live_keys) != 1 ||
        atomic_load(&key_creation_attempts) != creation_before + 2
    )
    {
        return 33;
    }

    // A partial host retains the first key and rejects new attachment admission.
    if (
        bray_runtime_thread_attachment_enter() != 4 ||
        atomic_load(&key_creation_attempts) != creation_before + 2 ||
        bray_runtime_structured_shutdown() != 4 ||
        atomic_load(&key_destruction_attempts) != destruction_before + 1 ||
        atomic_load(&live_keys) != 1
    )
    {
        return 33;
    }

    atomic_store(&reject_key_destruction, 0);
    atomic_store(&reject_key_creation_at, 0);

    if (
        bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&key_destruction_attempts) != destruction_before + 2 ||
        atomic_load(&live_keys) != 0
    )
    {
        return 33;
    }

    return 0;
}
#endif

static int check_primitive_attachment(void)
{
#if defined(_WIN32) || defined(__linux__)
    unsigned initial_live_allocations = atomic_load(&live_allocations);
    atomic_store(&reject_key_creation, 1);

    if (
        bray_runtime_thread_attachment_enter() != 4 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&key_creation_attempts) != 1 ||
        atomic_load(&live_keys) != 0
    )
        return 33;

    // Failed acquisition returns every reservation. Free pool capacity remains
    // reusable until shutdown, which must reclaim it without allocating.
    unsigned rollback_allocations = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);
    atomic_store(&reenter_deallocation, 1);

    if (
        bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&allocation_attempts) != rollback_allocations ||
        atomic_load(&live_allocations) != initial_live_allocations ||
        deallocation_reentries != 1 || deallocation_reentry_status != 0
    )
        return 33;

    atomic_store(&reject_allocations, 0);

    atomic_store(&reject_key_creation, 0);

    int partial_status = check_partial_host_formation();

    if (partial_status != 0)
    {
        return partial_status;
    }
#endif

    atomic_store(&reject_allocations, 1);

    if (bray_runtime_thread_attachment_enter() != 4 || bray_runtime_current_native_thread_identity() != 0)
        return 23;

    atomic_store(&reject_allocations, 0);
    atomic_store(&reenter_allocation, 1);
#if defined(_WIN32) || defined(__linux__)
    atomic_store(&reenter_key_creation, 1);
#endif
    unsigned startup_allocations = atomic_load(&allocation_attempts);
    size_t startup_bytes = atomic_load(&allocated_bytes);

    if (bray_runtime_thread_attachment_enter() != 0)
        return 29;

    printf(
        "bootstrap startup: allocations=%u bytes=%zu\n",
        atomic_load(&allocation_attempts) - startup_allocations,
        atomic_load(&allocated_bytes) - startup_bytes
    );

#if defined(_WIN32) || defined(__linux__)
    if (atomic_load(&recursive_key_status) != 4)
        return 34;
#endif

    unsigned empty_exit_allocations = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);

    if (
        bray_runtime_thread_attachment_leave() != 0 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&allocation_attempts) != empty_exit_allocations
    )
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

    if (reserved.state != 0)
        return 32;

    atomic_store(&reject_allocations, 1);
    uint32_t rejected_registration = bray_runtime_thread_attachment_cleanup(attachment_cleanup);

    if (
        rejected_registration != 4 ||
        bray_runtime_current_native_thread_identity() != rejected_owner_identity ||
        bray_runtime_thread_attachment_leave() != 0 ||
        bray_runtime_current_native_thread_identity() != 0 ||
        atomic_load(&attachment_cleanup_calls) != 0
    )
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
    {
        if (bray_runtime_current_native_thread_identity() != previous_attachment)
            return 25;
    }

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
        fprintf(
            stderr,
            "native attachment exit failed: status=%d cleanups=%u failures=%u allocations=%u->%u\n",
            status,
            atomic_load(&attachment_cleanup_calls), atomic_load(&attachment_cleanup_failures),
            before,
            atomic_load(&allocation_attempts)
        );

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
        {
            if (attachments[index].identity == attachments[other].identity)
                status = 39;
        }
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

    if (
        atomic_load(&attachment_cleanup_calls) != before + concurrent_threads ||
        atomic_load(&attachment_cleanup_failures) != 0
    )
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

    uint32_t shutdown_status = bray_runtime_structured_shutdown();
    unsigned after = atomic_load(&allocation_attempts);
    unsigned live = atomic_load(&live_allocations);

    if (shutdown_status != 1 || after != before || live != 0)
    {
        fprintf(stderr, "pool shutdown: status=%u allocation_attempts=%u expected_attempts=%u live=%u\n",
            shutdown_status, after, before, live);
        return 45;
    }

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

typedef struct
{
    unsigned char identity[32];
    uintptr_t bytes;
    uintptr_t alignment;
} CleanupShape;

typedef struct
{
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

    if (
        bray_runtime_cleanup_storage_admission(&shape, &lease) != 3 ||
        memcmp(&occupied, &lease, sizeof(lease)) != 0
    )
        return 67;

    atomic_store(&reject_allocations, 1);
    unsigned attempts = atomic_load(&allocation_attempts);

    if (bray_runtime_cleanup_storage_activation(&lease, &wrong) != NULL)
        return 61;

    unsigned char* backing = bray_runtime_cleanup_storage_activation(&lease, &shape);

    if (
        backing == NULL || (uintptr_t)backing % 4096 != 0 ||
        bray_runtime_cleanup_storage_activation(&lease, &shape) != NULL
    )
        return 62;

    backing[0] = 7;
    backing[65536] = 9;
    CleanupLease stale = lease;

    if (
        bray_runtime_cleanup_storage_release(&lease) != 0 ||
        bray_runtime_cleanup_storage_release(&lease) != 3 ||
        bray_runtime_cleanup_storage_admission(&shape, &lease) != 0
    )
        return 63;

    if (
        bray_runtime_cleanup_storage_activation(&stale, &shape) != NULL ||
        bray_runtime_cleanup_storage_release(&stale) != 3
    )
        return 64;

    backing = bray_runtime_cleanup_storage_activation(&lease, &shape);

    if (
        backing == NULL ||
        backing[0] != 7 ||
        backing[65536] != 9 ||
        bray_runtime_cleanup_storage_release(&lease) != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 65;

    CleanupLease preserved = lease;

    if (
        bray_runtime_cleanup_storage_admission(&wrong, &lease) == 0 ||
        memcmp(&preserved, &lease, sizeof(lease)) != 0
    )
        return 66;

    atomic_store(&reject_allocations, 0);

    return 0;
}

typedef struct
{
    uint32_t affinity;
    uint32_t lanes;
} ActivationState;
typedef struct
{
    uint32_t kind;
    uint32_t state;
    uintptr_t payload;
    PanicReport report;
} ActivationProgress;

typedef struct
{
    CleanupShape storage;
    uintptr_t header, completion, completion_bytes, completion_align;
    uint32_t states;
    ActivationState (*state)(uintptr_t, uint32_t, uint32_t, uintptr_t);
    void (*resume)(ActivationProgress*, uintptr_t);
    void (*cancel)(ActivationProgress*, uintptr_t);
    void (*broadcast)(ActivationProgress*, uintptr_t);
    void (*resolve)(ActivationProgress*, uintptr_t, uint32_t);
    void (*move_completion)(uintptr_t, uintptr_t);
    void (*destroy)(uintptr_t);
} ActivationDescriptor;

typedef struct
{
    CleanupLease lease;
    const ActivationDescriptor* descriptor;
} InactiveActivation;
typedef struct
{
    CleanupLease lease;
    const ActivationDescriptor* descriptor;
    uintptr_t context;
} ActivationHeaderPrefix;

typedef struct
{
    uintptr_t head, tail, count;
} ActivationRecords;

typedef struct
{
    CleanupLease lease;
    const ActivationDescriptor* descriptor;
    uintptr_t context, parent, child, current, broadcast_next;
    uint64_t origin, retained_origin;
    uint32_t state, phase, retained_affinity, retained_lanes, broadcasted;
    ActivationProgress progress;
    ActivationRecords incidents, outgoing;
    uintptr_t destination;
    ActivationProgress incoming;
    uint32_t disposition;
} ActivationHeader;

typedef struct
{
    uintptr_t address;
    unsigned char identity[32];
    uint32_t state;
    ActivationState requirements;
    uint64_t origin;
} ActivationExecution;

typedef uint32_t (*ActivationReady)(const ActivationExecution*);

typedef void (*ActivationCallback)(
    const ActivationExecution*,
    const ActivationDescriptor*,
    uintptr_t,
    uint32_t,
    uintptr_t,
    ActivationProgress*,
    ActivationRecords*
);

extern uint32_t bray_runtime_activation_snapshot(const ActivationHeader*, uint32_t, uint32_t, uintptr_t, ActivationExecution*);
extern uint32_t bray_runtime_activation_inspection(const InactiveActivation*, uint32_t, uint32_t, uintptr_t, ActivationExecution*);
extern uintptr_t bray_runtime_activation_entry(const InactiveActivation*, uint64_t);
extern uintptr_t bray_runtime_activation_composition(ActivationHeader*, const InactiveActivation*, uint32_t, uint64_t, uintptr_t, uint8_t, ActivationReady);
extern uint32_t bray_runtime_activation_dispatch(
    ActivationHeader*,
    ActivationCallback,
    ActivationReady,
    uint32_t,
    uintptr_t,
    ActivationProgress*,
    ActivationRecords*,
    ActivationRecords*
);

static unsigned activation_calls;
static unsigned activation_queries;

// This fixture's attached cooperative lane admits no blocking/compute or main
// work. Origin-affine callbacks must use this exact native thread attachment.
static uint32_t activation_ready(const ActivationExecution* execution)
{
    return execution->address != 0 &&
        execution->requirements.lanes == 0 &&
        execution->requirements.affinity <= 1 &&
        execution->origin == bray_runtime_current_native_thread_identity() ? 0 : 6;
}

static void activation_callback(
    const ActivationExecution* execution,
    const ActivationDescriptor* descriptor,
    uintptr_t context,
    uint32_t operation,
    uintptr_t argument,
    ActivationProgress* progress,
    ActivationRecords* records
)
{
    ActivationHeader* header = (ActivationHeader*)context;

    if (operation != 4)
        ++activation_calls;

    if (
        execution->address != (uintptr_t)header ||
        memcmp(execution->identity, descriptor->storage.identity, 32) != 0 ||
        execution->state != header->state ||
        execution->origin != header->origin ||
        (operation != 5 && header->descriptor != descriptor) ||
        header->outgoing.count != 0 ||
        records->count != 0
    )
        abort();

    if (operation == 4)
    {
        descriptor->move_completion(context, argument);
        *progress = (ActivationProgress){ .kind = 1 };

        return;
    }

    if (operation == 5)
    {
        uintptr_t bytes = descriptor->storage.bytes;
        *progress = (ActivationProgress){ .kind = 1 };

        descriptor->destroy(context);

        if (descriptor->storage.bytes != bytes || execution->address != context)
            abort();

        return;
    }

    header->broadcasted = 1;

    *progress = (ActivationProgress){ .kind = operation == 3 ? 0 : 1, .state = 7 };
}

static void activation_move_failure(
    const ActivationExecution* execution,
    const ActivationDescriptor* descriptor,
    uintptr_t context,
    uint32_t operation,
    uintptr_t argument,
    ActivationProgress* progress,
    ActivationRecords* records
)
{
    (void)descriptor;
    (void)records;

    if (operation != 4 || execution->address != context || argument == 0)
        abort();

    // A failed mover has not consumed or written either side of the transfer.
    *progress = (ActivationProgress){
        .kind = 3,
        .report = { .cause = 1, .primary_present = 1, .consume = bray_runtime_report_consumer },
    };
}

static int check_activation_deferral(ActivationHeader* header, ActivationCallback callback)
{
    ActivationRecords incidents = { 0 };

    ActivationHeader original = *header;
    unsigned attempts = atomic_load(&allocation_attempts);
    unsigned calls = activation_calls;
    uintptr_t destination = 0;

    for (uint32_t operation = 0; operation != 6; ++operation)
    {
        header->retained_lanes = 1;
        header->phase = operation == 5 ? 4 : operation == 4 ? 3 : 1;
        header->progress.kind = 1;
        ActivationHeader before = *header;
        ActivationProgress progress = { .kind = 13 };
        ActivationRecords remaining = { .count = 17 };
        unsigned queries = activation_queries;
        uintptr_t argument = operation == 4 ? (uintptr_t)&destination : 3;

        if (
            bray_runtime_activation_dispatch(
                header,
                callback,
                activation_ready,
                operation,
                argument,
                &progress,
                &remaining,
                &incidents
            ) != 6 ||
            memcmp(header, &before, sizeof(before)) != 0 ||
            progress.kind != 13 ||
            remaining.count != 17
            || incidents.count != 0 ||
            destination != 0 ||
            activation_queries != queries + 1 ||
            activation_calls != calls ||
            atomic_load(&allocation_attempts) != attempts
        )
            return 131;
    }

    *header = original;
    return 0;
}

static int check_activation_dispatch(ActivationHeader* header, ActivationCallback callback)
{
    ActivationRecords incidents = { 0 };

    unsigned attempts = atomic_load(&allocation_attempts);
    activation_calls = 0;

    int deferred = check_activation_deferral(header, callback);

    if (deferred != 0)
        return deferred;

    for (uint32_t operation = 0; operation != 4; ++operation)
    {
        ActivationProgress progress = { 0 };
        ActivationRecords remaining = { 0 };

        if (
            bray_runtime_activation_dispatch(
                header,
                callback,
                activation_ready,
                operation,
                3,
                &progress,
                &remaining,
                &incidents
            ) != 0 ||
            remaining.count != 0 ||
            header->broadcasted != 1 ||
            header->outgoing.count != 0 ||
            progress.kind != (operation == 3 ? 0 : 1) ||
            progress.state != 7
        )
            return 85;
    }

    ActivationProgress untouched = { .kind = 13 };
    ActivationRecords untouched_records = { .count = 17 };

    if (
        bray_runtime_activation_dispatch(
            header,
            callback,
            activation_ready,
            6,
            0,
            &untouched,
            &untouched_records,
            &incidents
        ) != 3 ||
        bray_runtime_activation_dispatch(
            header,
            callback,
            activation_ready,
            3,
            7,
            &untouched,
            &untouched_records,
            &incidents
        ) != 3 ||
        untouched_records.count != 17 ||
        incidents.count != 0 ||
        untouched.kind != 13 ||
        activation_calls != 4 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 86;

    // The result remains resident until an exact, separately owned destination
    // exists. Dispatch commits ownership only after the mover returns normally.
    uintptr_t* source = (uintptr_t*)(header->lease.address + header->descriptor->completion);
    uintptr_t destination = 0;

    ActivationProgress moved = { 0 };
    ActivationRecords remaining = { 0 };

    *source = 7919;

    header->phase = 3;
    header->progress.kind = 1;

    if (
        bray_runtime_activation_dispatch(
            header,
            activation_move_failure,
            activation_ready,
            4,
            (uintptr_t)&destination,
            &moved, &remaining, &incidents
        ) != 0 ||
        moved.kind != 3 ||
        header->phase != 3 ||
        header->progress.kind != 1 ||
        *source != 7919 ||
        destination != 0 ||
        remaining.count != 0
    )
        return 118;

    if (bray_runtime_report_consumer(&moved.report, 0) != 0)
        return 119;

    if (
        bray_runtime_activation_dispatch(
            header,
            callback,
            activation_ready,
            4,
            (uintptr_t)&destination,
            &moved,
            &remaining,
            &incidents
        ) != 0 ||
        moved.kind != 1 ||
        destination != 7919 ||
        *source != 0 ||
        header->phase != 4 ||
        remaining.count != 0 ||
        activation_calls != 5 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 117;

    header->phase = 1;
    header->progress = (ActivationProgress){ 0 };
    return 0;
}

extern uint32_t bray_runtime_activation_publication(const InactiveActivation*);

static ActivationState activation_state(uintptr_t context, uint32_t state, uint32_t operation, uintptr_t argument)
{
    const ActivationHeader* header = (const ActivationHeader*)context;

    if (header->phase > 5 || state >= header->descriptor->states)
        abort();

    // Inspection must release the pool lock before entering provider code.
    if (bray_runtime_cleanup_storage_activation(&header->lease, &header->descriptor->storage) != NULL)
        abort();

    ++activation_queries;

    if (state == 1)
        return (ActivationState){ .affinity = 1, .lanes = 1 };

    if (state == 2)
        return (ActivationState){ .affinity = 2, .lanes = 2 };

    if (state == 3)
        return (ActivationState){ .lanes = operation == 3 && argument == 3 ? 1 : 0 };

    if (state == 5)
        return (ActivationState){ .affinity = 3 };

    if (state == 6)
        return (ActivationState){ .lanes = 8 };

    return (ActivationState){ 0 };
}

static ActivationState activation_restricted_state(
    uintptr_t context,
    uint32_t state,
    uint32_t operation,
    uintptr_t argument
)
{
    ActivationState result = activation_state(context, state, operation, argument);
    result.lanes = 1;

    return result;
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
    ActivationHeader* header = (ActivationHeader*)context;
    uintptr_t* source = (uintptr_t*)(header->lease.address + header->descriptor->completion);

    ++activation_calls;
    *(uintptr_t*)destination = *source;
    *source = 0;
}

static void activation_destroy(uintptr_t context)
{
    ActivationHeader* header = (ActivationHeader*)context;
    CleanupLease lease = header->lease;
    uintptr_t bytes = header->descriptor->storage.bytes;

    memset(header, 0xa5, bytes);

    if (bray_runtime_cleanup_storage_release(&lease) != 0)
        abort();
}

// Fixture descriptors require no lane capability. Keep the token slot and its
// provider local to this synchronous call, and consume only an accepted token.
static uintptr_t activate_fixture(InactiveActivation* token, uint64_t origin)
{
    if (token->descriptor == NULL || origin == 0 || origin != bray_runtime_current_native_thread_identity())
        return 0;

    ActivationExecution state = { 0 };

    if (
        bray_runtime_activation_inspection(token, 0, 0, 0, &state) != 0 ||
        state.requirements.affinity != 0 ||
        state.requirements.lanes != 0
    )
        return 0;

    InactiveActivation original = *token;
    uintptr_t active = bray_runtime_activation_entry(token, origin);

    if (memcmp(token, &original, sizeof(original)) != 0)
        abort();

    if (active != 0)
        *token = (InactiveActivation){ 0 };

    return active;
}

static int check_activation_snapshot(ActivationHeader* header)
{
    ActivationExecution execution = { 0 };
    uint64_t origin = header->origin;

    if (
        bray_runtime_activation_snapshot(header, header->state, 0, 0, &execution) != 0 ||
        memcmp(execution.identity, header->descriptor->storage.identity, 32) != 0 ||
        execution.address != (uintptr_t)header ||
        execution.state != 0 ||
        execution.requirements.affinity != 0 ||
        execution.requirements.lanes != 0 ||
        execution.origin != origin
    )
        return 103;

    ActivationExecution unmodified = execution;
    size_t queries = activation_queries;

    if (
        bray_runtime_activation_snapshot(header, 0, 6, 0, &execution) != 3 ||
        bray_runtime_activation_snapshot(header, 0, 3, 7, &execution) != 3 ||
        memcmp(&unmodified, &execution, sizeof(execution)) != 0 ||
        activation_queries != queries
    )
        return 131;

    if (
        bray_runtime_activation_snapshot(header, 0, 3, 6, &execution) != 0 ||
        activation_queries != queries + 1
    )
        return 131;

    header->state = 1;
    header->retained_affinity = 2;
    header->retained_lanes = 2;
    header->retained_origin = origin;

    if (
        bray_runtime_activation_snapshot(header, header->state, 0, 0, &execution) != 0 ||
        execution.state != 1 ||
        execution.requirements.affinity != 1 ||
        execution.requirements.lanes != 7 ||
        execution.origin != origin
    )
        return 104;

    header->state = 2;
    header->retained_affinity = 1;
    header->retained_lanes = 1;

    if (
        bray_runtime_activation_snapshot(header, header->state, 0, 0, &execution) != 0 ||
        execution.state != 2 ||
        execution.requirements.affinity != 1 ||
        execution.requirements.lanes != 7 ||
        execution.origin != origin
    )
        return 105;

    header->state = 1;
    header->retained_origin = origin == 1 ? 2 : 1;
    ActivationExecution before = execution;

    if (
        bray_runtime_activation_snapshot(header, header->state, 0, 0, &execution) != 3 ||
        memcmp(&before, &execution, sizeof(before)) != 0
    )
        return 106;

    header->retained_affinity = 0;
    header->retained_lanes = 0;
    header->retained_origin = 0;

    for (uint32_t state = 5; state <= 8; ++state)
    {
        if (state == 7)
            continue;

        header->state = state;

        if (
            bray_runtime_activation_snapshot(header, header->state, 0, 0, &execution) != 3 ||
            memcmp(&before, &execution, sizeof(before)) != 0
        )
            return 107;
    }

    header->state = 0;

    if (
        bray_runtime_activation_snapshot(header, 3, 3, 0, &execution) != 0 ||
        execution.requirements.lanes != 0 ||
        bray_runtime_activation_snapshot(header, 3, 3, 3, &execution) != 0 ||
        execution.requirements.lanes != 1
    )
        return 130;

    return 0;
}

static uint32_t composition_ready(const ActivationExecution* execution)
{
    return execution->address != 0 &&
        execution->requirements.affinity == 1 &&
        execution->requirements.lanes == 1 &&
        execution->origin == bray_runtime_current_native_thread_identity() ? 0 : 6;
}

static int check_activation_composition(ActivationHeader* parent, InactiveActivation* token)
{
    ActivationRecords incidents = { 0 };
    ActivationExecution continuation = { 0 };
    ActivationExecution child = { 0 };

    uint64_t origin = bray_runtime_current_native_thread_identity();

    // This native fixture retains both slots and permits blocking execution on
    // its current thread. Only the requested continuation requires that lane.
    if (
        bray_runtime_activation_snapshot(parent, 1, 0, 0, &continuation) != 0 ||
        bray_runtime_activation_inspection(token, 0, 0, 0, &child) != 0 ||
        continuation.requirements.affinity != 1 ||
        continuation.requirements.lanes != 1 ||
        continuation.origin != origin ||
        child.requirements.affinity != 0 ||
        child.requirements.lanes != 0
    )
        return 108;

    unsigned attempts = atomic_load(&allocation_attempts);

    InactiveActivation original = *token;

    uintptr_t address = bray_runtime_activation_composition(
        parent,
        token,
        1,
        origin,
        (uintptr_t)&parent->incoming.payload,
        0,
        composition_ready
    );

    if (
        address == 0 ||
        memcmp(token, &original, sizeof(original)) != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 109;

    ActivationHeader* attached = (ActivationHeader*)address;
    *token = (InactiveActivation){ 0 };

    if (
        parent->child != address ||
        parent->state != 0 ||
        attached->parent != (uintptr_t)parent ||
        attached->phase != 1 ||
        attached->retained_affinity != 1 ||
        attached->retained_lanes != 1 ||
        attached->retained_origin != origin
    )
        return 110;

    // No source callback has entered this fixture child. End the fixture-owned
    // relationship before testing its consuming provider callback.
    parent->child = 0;
    attached->parent = 0;
    attached->retained_affinity = 0;
    attached->retained_lanes = 0;
    attached->retained_origin = 0;
    attached->phase = 4;
    ActivationProgress retired = { 0 };
    ActivationRecords remaining = { 0 };

    if (
        bray_runtime_activation_dispatch(
            attached,
            activation_callback,
            activation_ready,
            5,
            0,
            &retired,
            &remaining,
            &incidents
        ) != 0 ||
        retired.kind != 1 ||
        remaining.count != 0 ||
        incidents.count != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 111;

    return 0;
}

extern void bray_runtime_activation_initialization(const InactiveActivation*, uintptr_t, ActivationRecords*);

extern uint32_t bray_runtime_activation_step(
    ActivationHeader*,
    ActivationCallback,
    ActivationReady,
    uint8_t, uint8_t,
    ActivationProgress*,
    ActivationRecords*,
    ActivationRecords*
);

extern void bray_runtime_frame_completion_move(ActivationHeader*, RunOutcome*);

typedef struct
{
    ActivationHeader header;
    uintptr_t value;
    InactiveActivation next;
} ChainFrame;

enum ChainMode
{
    CHAIN_COMPLETE,
    CHAIN_CANCEL,
    CHAIN_ABANDON,
    CHAIN_LATE_CANCEL,
    CHAIN_ABANDON_RESULT,
    CHAIN_RESOLVER_FAILURE,
    CHAIN_MOVER_FAILURE,
    CHAIN_COMPLETE_AFTER_BROADCAST,
};

static enum ChainMode chain_mode;
static unsigned chain_disposals;
static unsigned chain_resumes;
static unsigned chain_cancellations;
static unsigned chain_releases;
static unsigned chain_broadcasts;
static unsigned chain_moves;
static unsigned chain_yield;
static unsigned chain_abandoning;
static uintptr_t chain_report_records[2];
static unsigned chain_reports_emitted;
static unsigned chain_reports_released;
static uintptr_t chain_stack_low;
static uintptr_t chain_stack_high;

static ActivationState chain_state(uintptr_t context, uint32_t state, uint32_t operation, uintptr_t argument)
{
    (void)context;
    (void)argument;

    if (state >= 3 || (chain_abandoning != 0 && state == 0 && operation == 0))
        abort();

    return (ActivationState){ 0 };
}

static void chain_resolve(ActivationProgress*, uintptr_t, uint32_t);

static void chain_resume(ActivationProgress* progress, uintptr_t context)
{
    ChainFrame* frame = (ChainFrame*)context;

    if (frame->header.state == 2)
    {
        chain_resolve(progress, context, 3);
        return;
    }

    ++chain_resumes;

    if (frame->header.state == 0 && frame->next.descriptor != NULL)
    {
        if (
            bray_runtime_activation_composition(
                &frame->header,
                &frame->next,
                1,
                frame->header.origin,
                (uintptr_t)&frame->value,
                0,
                activation_ready
            ) == 0
        )
            abort();

        frame->next = (InactiveActivation){ 0 };
        *progress = (ActivationProgress){ .state = 1 };

        return;
    }

    if (frame->header.incoming.kind != 0)
    {
        RunOutcome incoming = { 0 };
        bray_runtime_frame_completion_move(&frame->header, &incoming);

        if (incoming.state != 0 || incoming.payload != (uintptr_t)&frame->value)
            abort();

        ++frame->value;
    }
    else if (chain_yield != 0 && frame->header.state == 0)
    {
        *progress = (ActivationProgress){ .kind = 5, .state = 1 };
        return;
    }

    *progress = (ActivationProgress){ .kind = 1, .state = 1 };
}

static void chain_cancel(ActivationProgress* progress, uintptr_t context)
{
    ChainFrame* frame = (ChainFrame*)context;
    ++chain_cancellations;

    if (frame->header.incoming.kind != 0)
    {
        RunOutcome incoming = { 0 };
        bray_runtime_frame_completion_move(&frame->header, &incoming);

        if (incoming.state != 1)
            abort();
    }

    *progress = (ActivationProgress){
        .kind = chain_mode == CHAIN_COMPLETE_AFTER_BROADCAST ? 1 : 2,
        .state = 1,
    };
}

static void chain_broadcast(ActivationProgress* progress, uintptr_t context)
{
    (void)context;
    ++chain_broadcasts;
    *progress = (ActivationProgress){ .kind = 1 };
}

static void chain_resolve(ActivationProgress* progress, uintptr_t context, uint32_t exit)
{
    ChainFrame* frame = (ChainFrame*)context;

    if (frame->next.descriptor != NULL)
    {
        if (
            exit != 3 ||
            bray_runtime_activation_composition(
                &frame->header,
                &frame->next,
                2,
                frame->header.origin,
                (uintptr_t)&frame->value,
                1,
                activation_ready
            ) == 0
        )
            abort();

        frame->next = (InactiveActivation){ 0 };
        *progress = (ActivationProgress){ .state = 2 };

        return;
    }

    if (frame->header.incoming.kind != 0)
    {
        RunOutcome incoming = { 0 };
        bray_runtime_frame_completion_move(&frame->header, &incoming);

        if (exit != 3 || incoming.state != 1)
            abort();
    }

    if (chain_mode >= CHAIN_ABANDON_RESULT && exit == 3)
    {
        // Disposal must follow broadcast, and a failed mover must retain its source.
        if (chain_broadcasts != (chain_mode == CHAIN_COMPLETE_AFTER_BROADCAST ? 2U : 1U) || frame->value != 1)
            abort();

        ++chain_disposals;
    }

    *progress = (ActivationProgress){ .kind = 1 };
}

static void chain_move(uintptr_t context, uintptr_t destination)
{
    ChainFrame* frame = (ChainFrame*)context;
    ++chain_moves;
    *(uintptr_t*)destination = frame->value;
}

static void chain_destroy(uintptr_t context)
{
    ++chain_releases;
    activation_destroy(context);
}

static void chain_release_report(uintptr_t message, uintptr_t length, RunOutcome* outcome)
{
    if (message != chain_reports_released + 1 || length != 0 || outcome->state != 0)
        abort();

    ++chain_reports_released;
}

static void chain_callback(
    const ActivationExecution* execution,
    const ActivationDescriptor* descriptor,
    uintptr_t context,
    uint32_t operation,
    uintptr_t argument,
    ActivationProgress* progress,
    ActivationRecords* records
)
{
    volatile unsigned char stack_marker = 0;
    uintptr_t stack = (uintptr_t)&stack_marker;

    (void)execution;

    if (records->count != 0)
        abort();

    if (stack < chain_stack_low)
        chain_stack_low = stack;

    if (stack > chain_stack_high)
        chain_stack_high = stack;

    *progress = (ActivationProgress){ .kind = 1 };

    if (
        (
            operation == 3 && argument == 0 &&
            (
                chain_mode == CHAIN_RESOLVER_FAILURE ||
                chain_mode == CHAIN_COMPLETE_AFTER_BROADCAST
            )
        ) ||
        (operation == 4 && chain_mode == CHAIN_MOVER_FAILURE)
    )
    {
        *progress = (ActivationProgress){
            .kind = 3,
            .report = { .cause = 1, .primary_present = 1, .consume = bray_runtime_report_consumer },
        };

        return;
    }

    switch (operation)
    {
        case 0: descriptor->resume(progress, context); break;
        case 1: descriptor->cancel(progress, context); break;
        case 2: descriptor->broadcast(progress, context); break;
        case 3: descriptor->resolve(progress, context, (uint32_t)argument); break;
        case 4: descriptor->move_completion(context, argument); break;
        case 5: descriptor->destroy(context); break;
        default: abort();
    }

    if (
        chain_mode == CHAIN_CANCEL &&
        (
            (chain_reports_emitted == 0 && progress->kind == 5) ||
            (chain_reports_emitted == 1 && operation == 3 && progress->kind == 1)
        )
    )
    {
        RunOutcome incident = {
            .state = 2,
            .report = {
                .primary_present = 1,
                .message = chain_reports_emitted + 1,
                .release_message = chain_release_report,
                .consume = bray_runtime_report_consumer,
            },
        };

        bray_runtime_outgoing_retirement(chain_report_records[chain_reports_emitted], NULL, &incident);
        progress->report = incident.report;
        ++chain_reports_emitted;
    }
}

static int check_activation_chain(unsigned depth, enum ChainMode mode)
{
    const ActivationDescriptor descriptor = {
        .storage = { { 12 }, sizeof(ChainFrame), 4096 },
        .completion = offsetof(ChainFrame, value), .completion_bytes = sizeof(uintptr_t),
        .completion_align = _Alignof(uintptr_t), .states = 3,
        .state = chain_state, .resume = chain_resume, .cancel = chain_cancel,
        .broadcast = chain_broadcast, .resolve = chain_resolve,
        .move_completion = chain_move, .destroy = chain_destroy,
    };
    InactiveActivation next = { 0 };

    if (bray_runtime_thread_attachment_enter() != 0)
        return 132;

    for (unsigned i = 0; i < depth; ++i)
    {
        CleanupLease lease = { 0 };

        if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0)
            return 133;

        ChainFrame* frame = (ChainFrame*)bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);
        InactiveActivation token = { lease, &descriptor };
        ActivationRecords records = { 0 };

        bray_runtime_activation_initialization(&token, (uintptr_t)frame, &records);

        frame->value = 1;
        frame->next = next;

        if (bray_runtime_activation_publication(&token) != 0)
            return 134;

        next = token;
    }

    chain_resumes = chain_cancellations = chain_releases = chain_broadcasts = chain_moves = 0;
    chain_mode = mode;
    chain_disposals = 0;
    chain_yield = mode == CHAIN_CANCEL;
    chain_abandoning = 0;
    chain_stack_low = UINTPTR_MAX;
    chain_stack_high = 0;

    chain_reports_emitted = chain_reports_released = 0;

    if (mode == CHAIN_CANCEL)
    {
        RunOutcome admission = { 0 };
        bray_runtime_outgoing_admission(&(CleanupBundle){ 2, NULL, 0 }, &admission);

        if (admission.state != 0)
            return 189;

        for (unsigned index = 0; index < 2; ++index)
            chain_report_records[index] = bray_runtime_outgoing_activation(NULL, NULL);
    }

    atomic_store(&reject_allocations, 1);

    unsigned attempts = atomic_load(&allocation_attempts);

    ActivationHeader* root = (ActivationHeader*)activate_fixture(&next, bray_runtime_current_native_thread_identity());

    if (root == NULL)
        return 135;

    chain_abandoning = mode == CHAIN_ABANDON;

    int suspended = 0;
    unsigned status = 0;

    ActivationProgress progress = { 0 };
    ActivationRecords remaining = { 0 };
    ActivationRecords incidents = { 0 };

    for (unsigned steps = 0; steps < depth * 16 + 16; ++steps)
    {
        status = bray_runtime_activation_step(
            root,
            chain_callback,
            activation_ready,
            suspended != 0 ||
            mode == CHAIN_COMPLETE_AFTER_BROADCAST ||
            (mode == CHAIN_LATE_CANCEL && root->phase != 1),
            mode == CHAIN_ABANDON ||
            (mode == CHAIN_ABANDON_RESULT && root->phase == 2),
            &progress,
            &remaining,
            &incidents
        );

        if (remaining.count != 0 || (mode != CHAIN_CANCEL && incidents.count != 0))
            return 136;

        if (incidents.count != 0)
        {
            PanicReport report = {
                .head = incidents.head,
                .tail = incidents.tail,
                .count = incidents.count,
                .consume = bray_runtime_report_consumer,
            };

            incidents = (ActivationRecords){ 0 };

            if (bray_runtime_report_consumer(&report, 0) != 0)
                return 190;
        }

        if (status == 2)
            break;

        if (status == 1 && mode == CHAIN_CANCEL && !suspended && progress.kind == 5)
            suspended = 1;
        else if (status != 0)
            return 137;
    }

    uintptr_t step_stack_bytes = chain_stack_high - chain_stack_low;

    unsigned expected_kind = mode >= CHAIN_RESOLVER_FAILURE ?
        3U :
        (
            mode == CHAIN_CANCEL || mode == CHAIN_ABANDON ?
            2U :
            1U
        );

    unsigned moved = mode == CHAIN_COMPLETE || mode == CHAIN_LATE_CANCEL;

    unsigned broadcasts = moved ? 0U : mode == CHAIN_COMPLETE_AFTER_BROADCAST ? 2U : depth;

    unsigned resumes = mode == CHAIN_ABANDON || mode == CHAIN_COMPLETE_AFTER_BROADCAST ?
        0U :
        (
            mode == CHAIN_CANCEL ?
            depth :
            depth * 2 - 1
        );

    if (
        status != 2 ||
        chain_releases != depth - 1 ||
        progress.kind != expected_kind ||
        chain_moves != (moved ? depth : 0U) ||
        chain_broadcasts != broadcasts ||
        chain_resumes != resumes ||
        chain_cancellations != (mode == CHAIN_CANCEL || mode == CHAIN_COMPLETE_AFTER_BROADCAST ? depth : 0U) ||
        chain_disposals != (mode >= CHAIN_ABANDON_RESULT ? 1U : 0U) ||
        step_stack_bytes > 16384 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 138;

    if (
        moved && (progress.payload != (uintptr_t)&((ChainFrame*)root)->value ||
        *(uintptr_t*)progress.payload != depth)
    )
        return 139;

    if (progress.kind == 3 && bray_runtime_report_consumer(&progress.report, 0) != 0)
        return 142;

    // The scalar payload is consumed before the observed root is retired.
    progress = (ActivationProgress){ 0 };

    if (
        bray_runtime_activation_dispatch(
            root,
            chain_callback,
            activation_ready,
            5,
            0,
            &progress,
            &remaining,
            &incidents
        ) != 0 ||
        chain_releases != depth ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 140;

    if (mode == CHAIN_CANCEL)
    {
        if (chain_reports_emitted != 2 || chain_reports_released != 2)
            return 191;

        bray_runtime_outgoing_discharge(&(CleanupBundle){ 2, NULL, 0 });
    }

    atomic_store(&reject_allocations, 0);

    if (bray_runtime_thread_attachment_leave() != 0)
        return 141;

    printf(
        "activation chain: depth=%u mode=%d resumes=%u step_stack_bytes=%zu\n",
        depth, mode, chain_resumes, (size_t)step_stack_bytes
    );

    return 0;
}

static int check_activation_publication(int embedded_descriptor, int restricted)
{
    ActivationRecords incidents = { 0 };

    const ActivationDescriptor descriptor = {
        .storage = { { 11 }, 1024, 4096 },
        .completion = sizeof(ActivationHeader), .completion_bytes = sizeof(uintptr_t),
        .completion_align = _Alignof(uintptr_t), .states = 8,
        .state = restricted ? activation_restricted_state : activation_state,
        .resume = activation_complete, .cancel = activation_complete,
        .broadcast = activation_complete, .resolve = activation_resolve,
        .move_completion = activation_move, .destroy = activation_destroy,
    };

    CleanupLease lease = { 0 };
    CleanupLease child_lease = { 0 };
    InactiveActivation child = { 0 };
    int status = 0;

    if (bray_runtime_thread_attachment_enter() != 0)
        return 92;

    if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0)
    {
        status = 80;
        goto cleanup;
    }

    InactiveActivation token = { lease, &descriptor };

    if (bray_runtime_activation_publication(&token) != 3)
    {
        status = 81;
        goto cleanup;
    }

    unsigned char* bytes = bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);

    if (bytes == NULL)
    {
        status = 82;
        goto cleanup;
    }

    memset(bytes, 0, descriptor.storage.bytes);

    if (embedded_descriptor)
    {
        ActivationDescriptor* embedded = (ActivationDescriptor*)(bytes + 512);
        *embedded = descriptor;
        token.descriptor = embedded;
    }

    ActivationHeaderPrefix header = { lease, token.descriptor, lease.address };
    memcpy(bytes, &header, sizeof(header));

    if (!restricted)
    {
        if (bray_runtime_cleanup_storage_admission(&descriptor.storage, &child_lease) != 0)
        {
            status = 112;
            goto cleanup;
        }

        ActivationHeader* child_header = (ActivationHeader*)bray_runtime_cleanup_storage_activation(
            &child_lease,
            &descriptor.storage
        );

        if (child_header == NULL)
        {
            status = 113;
            goto cleanup;
        }

        memset(child_header, 0, descriptor.storage.bytes);
        child = (InactiveActivation){ child_lease, &descriptor };
        child_header->lease = child_lease;
        child_header->descriptor = &descriptor;
        child_header->context = child_lease.address;

        if (bray_runtime_activation_publication(&child) != 0)
        {
            status = 114;
            goto cleanup;
        }
    }

    atomic_store(&reject_allocations, 1);
    unsigned attempts = atomic_load(&allocation_attempts);

    if (
        bray_runtime_activation_publication(&token) != 0 ||
        bray_runtime_activation_publication(&token) != 3
    )
    {
        status = 83;
        goto cleanup;
    }

    uint64_t origin = bray_runtime_current_native_thread_identity();
    activation_queries = 0;

    if (restricted)
    {
        InactiveActivation rejected = token;
        ActivationExecution requirements = { 0 };

        if (
            activate_fixture(&rejected, origin) != 0 ||
            memcmp(&rejected, &token, sizeof(token)) != 0 ||
            bray_runtime_activation_inspection(&token, 0, 0, 0, &requirements) != 0 ||
            requirements.requirements.lanes != 1 ||
            activation_queries != 2 ||
            ((ActivationHeader*)bytes)->phase != 0 ||
            atomic_load(&allocation_attempts) != attempts
        )
            status = 101;

        goto cleanup;
    }

    ActivationExecution inspected = { .requirements = { .affinity = 17, .lanes = 19 } };

    if (
        bray_runtime_activation_inspection(&token, 8, 0, 0, &inspected) != 3 ||
        inspected.requirements.affinity != 17 ||
        inspected.requirements.lanes != 19 ||
        activation_queries != 0 ||
        bray_runtime_activation_inspection(&token, 7, 0, 0, &inspected) != 0 ||
        inspected.requirements.affinity != 0 ||
        inspected.requirements.lanes != 0 ||
        activation_queries != 1
    )
    {
        status = 99;
        goto cleanup;
    }

    for (uint32_t state = 5; state <= 6; ++state)
    {
        ActivationExecution invalid = { .requirements = { .affinity = 17, .lanes = 19 } };

        if (
            bray_runtime_activation_inspection(&token, state, 0, 0, &invalid) != 3 ||
            invalid.requirements.affinity != 17 ||
            invalid.requirements.lanes != 19
        )
        {
            status = 102;
            goto cleanup;
        }
    }

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

    for (unsigned i = 0; i < sizeof(rejected) / sizeof(rejected[0]); ++i)
    {
        InactiveActivation before = rejected[i];

        if (
            activate_fixture(&rejected[i], origin) != 0 ||
            memcmp(&before, &rejected[i], sizeof(before)) != 0
        )
        {
            status = 90;
            goto cleanup;
        }
    }

    if (activation_queries != 3)
    {
        status = 100;
        goto cleanup;
    }

    InactiveActivation entering = token;

    uint64_t wrong_origin = origin == 1 ? 2 : 1;

    if (activate_fixture(&entering, 0) != 0 || activate_fixture(&entering, wrong_origin) != 0 ||
        memcmp(&entering, &token, sizeof(token)) != 0)
    {
        status = 89;
        goto cleanup;
    }

    uintptr_t active = activate_fixture(&entering, origin);

    if (active != (uintptr_t)bytes || entering.lease.address != 0 || entering.descriptor != NULL)
    {
        status = 87;
        goto cleanup;
    }

    if (activate_fixture(&entering, origin) != 0)
    {
        status = 91;
        goto cleanup;
    }

    InactiveActivation duplicate = token;

    if (activate_fixture(&duplicate, origin) != 0 || memcmp(&duplicate, &token, sizeof(token)) != 0)
    {
        status = 88;
        goto cleanup;
    }

    status = check_activation_snapshot((ActivationHeader*)active);

    if (status != 0)
        goto cleanup;

    // Exercise a source continuation and a resumed lifecycle continuation.
    ((ActivationHeader*)active)->phase = embedded_descriptor ? 5 : 1;
    status = check_activation_composition((ActivationHeader*)active, &child);
    ((ActivationHeader*)active)->phase = 1;

    if (status != 0)
        goto cleanup;

    child_lease = (CleanupLease){ 0 };

    int dispatch_status = check_activation_dispatch((ActivationHeader*)active, activation_callback);

    if (dispatch_status != 0)
    {
        status = dispatch_status;
        goto cleanup;
    }

    ((ActivationHeader*)active)->phase = 4;

    ActivationProgress retired = { 0 };
    ActivationRecords remaining = { 0 };

    if (
        bray_runtime_activation_dispatch(
            (ActivationHeader*)active,
            activation_callback,
            activation_ready,
            5,
            0,
            &retired,
            &remaining,
            &incidents
        ) != 0 ||
        retired.kind != 1 ||
        remaining.count != 0 ||
        incidents.count != 0
    )
    {
        status = 96;
        goto cleanup;
    }

    lease = (CleanupLease){ 0 };

    // The old lease remains invalid with a live descriptor supplied for inspection.
    token.descriptor = &descriptor;

    if (
        bray_runtime_activation_publication(&token) != 3 ||
        bray_runtime_cleanup_storage_admission(&descriptor.storage, &lease) != 0
    )
    {
        status = 83;
        goto cleanup;
    }

    unsigned char* reused = bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);

    if (reused != bytes || bray_runtime_activation_publication(&token) != 3)
    {
        status = 84;
        goto cleanup;
    }

    for (uintptr_t i = 0; i < descriptor.storage.bytes; ++i)
    {
        if (reused[i] != 0xa5)
        {
            status = 97;
            goto cleanup;
        }
    }

    if (
        bray_runtime_cleanup_storage_release(&lease) != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
    {
        status = 84;
        goto cleanup;
    }
cleanup:
    if (child_lease.address != 0)
        bray_runtime_cleanup_storage_release(&child_lease);

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

    printf(
        "cleanup bundle: allocations=%u bytes=%zu payload_bytes=%zu alignment=%zu\n",
        atomic_load(&allocation_attempts) - admission_attempts,
        atomic_load(&allocated_bytes) - admission_bytes,
        2 * shapes[0].bytes,
        shapes[0].alignment
    );

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

    if (
        bray_runtime_cleanup_storage_release(&first) != 3 ||
        bytes[0] != 23 || bytes[65536] != 29 ||
        bray_runtime_cleanup_storage_release(&retained) != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
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

typedef struct CleanupIncident CleanupIncident;

struct CleanupIncident
{
    uintptr_t payload;
    unsigned char identity[32];
    SourceAnchor source;
    uint32_t (*report)(uintptr_t, RunOutcome*);
    void (*destroy)(CleanupIncident*, RunOutcome*, RunOutcome*);
    CleanupLease backing;
};

extern PanicReport bray_runtime_cleanup_incident_transfer(const CleanupIncident*, PanicReport);

static const CleanupShape incident_shape = { { 77 }, sizeof(unsigned), _Alignof(unsigned) };
static unsigned incident_destructions;
static unsigned incident_depth;
static unsigned incident_max_depth;
static uint32_t incident_outcome;

static ActivationRecords retain_test_incident(unsigned remaining);

static uint32_t report_test_incident(uintptr_t payload, RunOutcome* outcome)
{
    *outcome = (RunOutcome){ 0 };

    return *(unsigned*)payload == 0 ? 3 : 0;
}

static void destroy_test_incident(CleanupIncident* incident, RunOutcome* destruction, RunOutcome* release)
{
    *destruction = (RunOutcome){ 0 };
    *release = (RunOutcome){ 0 };

    incident_depth += 1;

    if (incident_depth > incident_max_depth)
        incident_max_depth = incident_depth;

    unsigned remaining = *(unsigned*)incident->payload;

    if (
        incident->payload != incident->backing.address ||
        remaining == 0 ||
        bray_runtime_cleanup_storage_release(&incident->backing) != 0
    )
        abort();

    incident_destructions += 1;

    if (remaining > 1)
    {
        ActivationRecords next = retain_test_incident(remaining - 1);

        destruction->state = incident_outcome;
        destruction->report.head = next.head;
        destruction->report.tail = next.tail;
        destruction->report.count = next.count;
    }

    incident_depth -= 1;
}

static ActivationRecords retain_test_incident(unsigned remaining)
{
    CleanupLease backing = { 0 };

    bray_runtime_cleanup_storage_claim(&incident_shape, &backing);
    *(unsigned*)backing.address = remaining;

    CleanupIncident incident = {
        .payload = backing.address,
        .identity = { 77 },
        .report = report_test_incident,
        .destroy = destroy_test_incident,
        .backing = backing
    };

    PanicReport report = { 0 };

    report = bray_runtime_cleanup_incident_transfer(&incident, report);

    return (ActivationRecords){ report.head, report.tail, report.count };
}

static int check_retained_incidents(unsigned count, uint32_t outcome)
{
    CleanupShape shapes[2] = { incident_shape, incident_shape };
    CleanupBundle bundle = { 2, shapes, 2 };
    RunOutcome admission = { 0 };

    bray_runtime_outgoing_admission(&bundle, &admission);

    if (admission.state != 0)
        return 120;

    incident_outcome = outcome;
    incident_destructions = 0;
    incident_depth = 0;
    incident_max_depth = 0;

    unsigned attempts = atomic_load(&allocation_attempts);

    atomic_store(&reject_allocations, 1);

    ActivationRecords records = retain_test_incident(count);

    if (count == 1)
        bray_runtime_outgoing_discharge(&bundle);

    PanicReport report = {
        .head = records.head,
        .tail = records.tail,
        .count = records.count
    };

    PanicReport incoming = { .primary_present = 1, .cause = 1, .consume = bray_runtime_report_consumer };
    PanicReport combined = bray_runtime_panic_report_suppression(&report, &incoming);

    if (
        combined.primary_present != 1 ||
        combined.cause != 1 ||
        combined.count != 1 ||
        report.primary_present != 0 ||
        report.count != 0 ||
        incoming.primary_present != 0
    )
        return 188;

    report = combined;


    uint32_t status = bray_runtime_panic_report_destruction(&report);

    if (count != 1)
        bray_runtime_outgoing_discharge(&bundle);

    atomic_store(&reject_allocations, 0);

    if (
        status != (count == 1 || outcome == 0 ? 0u : 5u) ||
        incident_destructions != count ||
        incident_max_depth != 1 ||
        incident_depth != 0 ||
        atomic_load(&allocation_attempts) != attempts
    )
        return 121;

    return 0;
}

static void check_product_teardown(void);
static void check_thread_product_teardown(void);

int main(void)
{
    allocation_callback = before_allocation;
    release_callback = before_release;

    int attachment_status = check_primitive_attachment();

    if (attachment_status != 0)
        return attachment_status;

    int concurrent_status = check_concurrent_attachments();

    if (concurrent_status != 0)
        return concurrent_status;

#if defined(_WIN32) || defined(__linux__)
    unsigned destruction_before = atomic_load(&key_destruction_attempts);

    atomic_store(&reject_key_destruction, 1);

    if (
        bray_runtime_structured_shutdown() != 4 ||
        atomic_load(&key_destruction_attempts) != destruction_before + 2 ||
        atomic_load(&live_keys) != 2 ||
        bray_runtime_thread_attachment_enter() != 0 ||
        bray_runtime_current_native_thread_identity() == 0 ||
        bray_runtime_thread_attachment_leave() != 0
    )
        return 34;

    atomic_store(&reject_key_destruction, 0);

    if (
        bray_runtime_structured_shutdown() != 1 ||
        atomic_load(&key_destruction_attempts) != destruction_before + 4 ||
        atomic_load(&live_keys) != 0
    )
        return 35;
#endif

    if (bray_runtime_initialization(1, 1) != 0 || bray_runtime_initialization(1, 1) != 2)
        return 1;

    int stream_status = stream_without_allocation();

    if (stream_status != 0)
        return stream_status;

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

    if (bundle_status != 0)
        return bundle_status;

    int incident_status = check_retained_incidents(1, 0);

    if (incident_status == 0)
        incident_status = check_retained_incidents(2048, 2);

    if (incident_status == 0)
        incident_status = check_retained_incidents(2048, 0);

    if (incident_status != 0)
        return incident_status;

    int backing_status = check_cleanup_backing();

    if (backing_status != 0)
        return backing_status;

    int publication_status = check_activation_publication(0, 0);

    if (publication_status == 0)
        publication_status = check_activation_publication(1, 0);

    if (publication_status == 0)
        publication_status = check_activation_publication(0, 1);

    if (publication_status == 0)
        publication_status = check_activation_publication(1, 1);
    if (publication_status != 0) return publication_status;

    for (enum ChainMode mode = CHAIN_LATE_CANCEL; mode <= CHAIN_COMPLETE_AFTER_BROADCAST; ++mode)
    {
        int status = check_activation_chain(1, mode);

        if (status != 0)
            return status;
    }

    const unsigned depths[] = { 32, 2048 };

    for (size_t i = 0; i < sizeof(depths) / sizeof(depths[0]); ++i)
    {
        int chain_status = check_activation_chain(depths[i], CHAIN_COMPLETE);

        if (chain_status == 0)
            chain_status = check_activation_chain(depths[i], CHAIN_CANCEL);

        if (chain_status == 0)
            chain_status = check_activation_chain(depths[i], CHAIN_ABANDON);

        if (chain_status != 0)
            return chain_status;
    }

    check_product_teardown();
    check_thread_product_teardown();

    if (bray_runtime_structured_shutdown() != 0 || atomic_load(&live_allocations) != 0)
        return 77;

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0)
        return 7;

#if defined(_WIN32) || defined(__linux__)
    if (atomic_load(&live_keys) != 0)
        return 36;
#endif

    return 0;
}
