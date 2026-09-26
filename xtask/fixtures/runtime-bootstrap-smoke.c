#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static atomic_bool reject_allocations;
static atomic_uint allocation_attempts;

static void* allocate_report_memory(size_t bytes, size_t alignment)
{
    atomic_fetch_add(&allocation_attempts, 1);

    if (atomic_load(&reject_allocations))
        return NULL;

    if (alignment > _Alignof(max_align_t))
        abort();

    return malloc(bytes);
}

#ifdef _WIN32
void* _aligned_malloc(size_t bytes, size_t alignment)
{
    return allocate_report_memory(bytes, alignment);
}

void _aligned_free(void* memory)
{
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
    uint8_t package[32];
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
    uint64_t source_package_0,
    uint64_t source_package_1,
    uint64_t source_package_2,
    uint64_t source_package_3,
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

int main(void)
{
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

    if (atomic_load_explicit(&cleanup_shield_balance, memory_order_relaxed) != 0)
        return 7;

    return 0;
}
