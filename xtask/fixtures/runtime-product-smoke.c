#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

typedef struct
{
    uint32_t status;
    uint32_t state;
    size_t active_entries;
    size_t external_roots;
    size_t thread_attachments;
    size_t initialized_statics;
    size_t cleaned_statics;
    size_t cleanup_incidents;
    uint8_t last_incident[32];
} HostObservation;

typedef struct
{
    uint8_t bytes[32];
} StaticIdentity;

typedef struct
{
    uint32_t execution;
    void (*admission)(uint32_t, struct RunOutcome*);
    uint32_t (*start)(size_t, struct RunOutcome*);
    uint32_t (*resolve)(size_t, size_t, struct RunOutcome*);
} StaticFinalizer;

typedef struct
{
    uint32_t abi_version;
    uint32_t duration;
    StaticIdentity identity;
    uint64_t order;
    size_t storage;
    size_t (*access)(void);
    void (*prepare)(void);
    StaticFinalizer finalizer;
    void (*destroy)(uint32_t, struct RunOutcome*);
    void (*detach)(void);
    StaticIdentity (*dependency)(size_t);
    size_t dependency_count;
} StaticEntry;

struct ProductExecution;

typedef struct
{
    uint32_t abi_version;
    uint32_t asynchronous;
    uint8_t identity[32];
    StaticEntry (*static_entry)(size_t);
    size_t static_count;
    uint32_t (*execution)(uint32_t, struct ProductExecution*);
    void* activation;
    const void* provider;
} HostDescriptor;



static unsigned product_visits;
static unsigned product_destroyed;
static unsigned product_node;
static unsigned product_sequence;
static unsigned product_failure;

static size_t product_access(void)
{
    return 1;
}

static void product_prepare_first(void)
{
    product_node = 0;
}

static void product_prepare_second(void)
{
    product_node = 1;
}

static void product_detach(void)
{}

static void product_admit(uint32_t operation, RunOutcome* outcome)
{
    (void)operation;
    *outcome = (RunOutcome){0};
}

static void product_admit_second(uint32_t operation, RunOutcome* outcome)
{
    product_admit(operation, outcome);

    if (operation == 0 && product_failure == 1)
    {
        outcome->state = 2;
        outcome->report = panic_report();
    }
}

static uint32_t product_finalize(size_t destination, RunOutcome* outcome)
{
    (void)destination;
    *outcome = (RunOutcome){0};
    product_sequence = product_sequence * 10 + product_node + 1;

    if (product_node == 0 && product_failure == 2)
    {
        outcome->state = 2;
        outcome->report = panic_report();
    }

    return 0;
}

static uint32_t product_resolve(size_t value, size_t destination, RunOutcome* outcome)
{
    (void)value;
    (void)destination;
    *outcome = (RunOutcome){0};

    return 0;
}

static void product_destroy(uint32_t abandoned, RunOutcome* outcome)
{
    assert(abandoned == (uint32_t)(product_node == 0 && product_failure == 2));
    ++product_destroyed;
    product_sequence = product_sequence * 10 + product_node + 3;
    *outcome = (RunOutcome){0};

    if (product_node == 0 && product_failure == 2)
    {
        outcome->state = 2;
        outcome->report = panic_report();
    }
}

static StaticIdentity product_dependency(size_t ordinal)
{
    (void)ordinal;
    abort();
}

static StaticEntry product_entry(size_t ordinal)
{
    assert(ordinal < 2);
    ++product_visits;

    StaticEntry entry = {
        2, 0, {{1}}, ordinal, 1, product_access,
        ordinal == 0 ? product_prepare_first : product_prepare_second,
        {1, ordinal == 0 ? product_admit : product_admit_second, product_finalize, product_resolve},
        product_destroy, product_detach, product_dependency, 0
    };

    entry.identity.bytes[0] = (uint8_t)(ordinal + 1);

    return entry;
}

static const HostDescriptor teardown_products[] = {
    {2, 0, {1}, product_entry, 2, NULL},
    {2, 0, {2}, product_entry, 2, NULL},
    {2, 0, {3}, product_entry, 2, NULL},
};

extern uint32_t bray_runtime_product_host_control(uintptr_t, uint32_t, HostObservation*);

static HostObservation product_control(unsigned selected, uint32_t operation)
{
    HostObservation result;

    assert(bray_runtime_product_host_control(
        (uintptr_t)&teardown_products[selected], operation, &result
    ) == 0);

    return result;
}

static unsigned interrupted_admitted;
static unsigned interrupted_released;

static void interrupted_admission(uint32_t operation, RunOutcome* outcome)
{
    product_admit(operation, outcome);

    if (operation == 1)
    {
        ++interrupted_released;
    }
    else if (operation == 0 && ++interrupted_admitted == 1)
    {
        atomic_store(&reject_allocations, 1);
    }
}

static StaticEntry interrupted_entry(size_t ordinal)
{
    ++product_visits;

    StaticEntry entry = {
        2, 0, {{0}}, ordinal, 1, product_access, product_prepare_first,
        {1, interrupted_admission, product_finalize, product_resolve},
        product_destroy, product_detach, product_dependency, 0
    };

    uint32_t identity = (uint32_t)ordinal + 1;
    memcpy(entry.identity.bytes, &identity, sizeof(identity));

    return entry;
}

static const HostDescriptor interrupted_product = {
    2, 0, {4}, interrupted_entry, 4096, NULL
};

static void check_interrupted_formation(void)
{
    product_failure = 0;
    product_visits = product_destroyed = product_sequence = 0;

    HostObservation result = {0};
    uint32_t native = bray_runtime_product_host_control(
        (uintptr_t)&interrupted_product, 0, &result
    );

    atomic_store(&reject_allocations, 0);

    assert(native == 0 && result.status == 5 && result.state == 4);
    assert(interrupted_admitted > 0 && interrupted_admitted < 4096);
    assert(result.initialized_statics == interrupted_admitted);
    assert(result.cleaned_statics == interrupted_admitted);
    assert(product_destroyed == interrupted_admitted);
    assert(interrupted_released == 0);

    unsigned visits = product_visits;
    assert(bray_runtime_product_host_control(
        (uintptr_t)&interrupted_product, 5, &result
    ) == 0);
    assert(result.status == 5 && result.state == 4);
    assert(product_visits == visits && product_destroyed == interrupted_admitted);
}

static void check_product_teardown(void)
{
    size_t bytes_before = atomic_load(&allocated_bytes);
    unsigned blocks_before = atomic_load(&live_allocations);
    HostObservation result = product_control(0, 0);

    assert(result.status == 0 && result.initialized_statics == 2 && product_visits == 2);

    size_t formation_bytes = atomic_load(&allocated_bytes) - bytes_before;
    unsigned formation_blocks = atomic_load(&live_allocations) - blocks_before;

    result = product_control(0, 12);
    assert(result.status == 3 && result.thread_attachments == 0);

    result = product_control(0, 3);
    assert(result.status == 0 && result.external_roots == 1);

    unsigned before = bray_allocation_probe_begin();

    result = product_control(0, 5);
    assert(result.status == 1 && result.state == 2 && product_destroyed == 0);

    result = product_control(0, 4);

    unsigned attempts = bray_allocation_probe_end(before);

    assert(result.status == 2 && result.cleaned_statics == 2);
    assert(product_destroyed == 2 && product_visits == 2 && attempts == 0);
    assert(product_sequence == 1324);

    result = product_control(0, 5);
    assert(result.status == 2 && product_destroyed == 2 && product_visits == 2);

    printf(
        "product_teardown formation_bytes=%zu formation_blocks=%u statics=2 "
        "descriptor_visits=%u destructions=%u post_admission_allocations=%u\n",
        formation_bytes, formation_blocks, product_visits, product_destroyed, attempts
    );

    product_failure = 1;
    product_visits = product_destroyed = product_sequence = 0;
    result = product_control(1, 0);

    assert(result.status == 7 && result.state == 4);
    assert(result.initialized_statics == 1 && result.cleaned_statics == 1);
    assert(product_visits == 2 && product_destroyed == 1 && product_sequence == 13);

    result = product_control(1, 0);
    assert(result.status == 7 && product_visits == 2 && product_destroyed == 1);

    product_failure = 2;
    product_visits = product_destroyed = product_sequence = 0;
    result = product_control(2, 0);
    assert(result.status == 0);

    result = product_control(2, 5);
    assert(result.status == 4 && result.state == 3 && result.cleanup_incidents == 2);
    assert(result.cleaned_statics == 2 && product_destroyed == 2 && product_sequence == 1324);
    assert(result.last_incident[0] == 1 && product_visits == 2);

    check_interrupted_formation();
}

typedef struct
{
    uintptr_t product;
    StaticIdentity identity;
    void (*prepare)(void);
    StaticFinalizer finalizer;
    void (*destroy)(uint32_t, RunOutcome*);
    void (*detach)(void);
} ThreadRegistration;

extern uint64_t bray_runtime_thread_attachment_identity(const HostDescriptor*);
extern uint32_t bray_runtime_thread_static_cleanup_registration(const ThreadRegistration*);

static StaticEntry thread_product_entry(size_t ordinal)
{
    StaticEntry entry = product_entry(ordinal);

    entry.duration = 1;

    return entry;
}

static void reject_thread_backing(uint32_t operation, RunOutcome* outcome)
{
    assert(operation == 0);

    *outcome = (RunOutcome){0};
    outcome->state = 3;
}

static void check_thread_product_teardown(void)
{
    static const HostDescriptor descriptor = {2, 0, {5}, thread_product_entry, 2, NULL};
    HostObservation result;

    product_failure = 0;
    product_visits = product_destroyed = product_sequence = 0;

    assert(bray_runtime_thread_attachment_enter() == 0);
    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 0, &result) == 0);
    assert(result.status == 0 && product_visits == 2);

    size_t bytes_before = atomic_load(&allocated_bytes);
    unsigned blocks_before = atomic_load(&live_allocations);
    uint64_t epoch = bray_runtime_thread_attachment_identity(&descriptor);

    assert(epoch != 0 && product_visits == 2);

    size_t membership_bytes = atomic_load(&allocated_bytes) - bytes_before;
    unsigned membership_blocks = atomic_load(&live_allocations) - blocks_before;

    // Register in the opposite order to the descriptor's cleanup order.
    for (size_t index = 2; index != 0; --index)
    {
        StaticEntry entry = thread_product_entry(index - 1);

        ThreadRegistration registration = {
            (uintptr_t)&descriptor, entry.identity, entry.prepare,
            entry.finalizer, entry.destroy, entry.detach
        };

        if (index == 2)
        {
            registration.finalizer.admission = reject_thread_backing;
            assert(bray_runtime_thread_static_cleanup_registration(&registration) == 9);
            assert(product_destroyed == 0 && product_sequence == 0);

            registration.finalizer.admission = product_admit;
        }

        assert(bray_runtime_thread_static_cleanup_registration(&registration) == 0);
    }

    unsigned visits = product_visits;
    unsigned before = bray_allocation_probe_begin();

    for (unsigned index = 0; index < 256; ++index)
    {
        assert(bray_runtime_thread_attachment_identity(&descriptor) == epoch);
    }

    unsigned warm_allocations = bray_allocation_probe_end(before);

    assert(warm_allocations == 0 && product_visits == visits);

    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 9, &result) == 0);
    assert(result.thread_attachments == 1);
    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 9, &result) == 0);
    assert(result.thread_attachments == 1);
    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 5, &result) == 0);
    assert(result.status == 1);

    before = bray_allocation_probe_begin();
    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 10, &result) == 0);
    assert(result.thread_attachments == 1 && product_destroyed == 0);
    assert(bray_runtime_product_host_control((uintptr_t)&descriptor, 10, &result) == 0);

    unsigned cleanup_allocations = bray_allocation_probe_end(before);

    assert(result.status == 2 && result.thread_attachments == 0);
    assert(product_destroyed == 2 && product_sequence == 1324 && product_visits == visits);
    assert(cleanup_allocations == 0);
    assert(bray_runtime_thread_attachment_identity(&descriptor) == 0);
    assert(bray_runtime_thread_attachment_leave() == 0);

    printf(
        "tls_domain membership_bytes=%zu membership_blocks=%u statics=2 "
        "warm_accesses=256 warm_allocations=%u cleanup_allocations=%u\n",
        membership_bytes, membership_blocks, warm_allocations, cleanup_allocations
    );
}
