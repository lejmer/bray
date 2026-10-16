#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <time.h>

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
    uint64_t value;
} Owner;

struct RunOutcome;
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
    void (*destroy)(struct RunOutcome*);
    void (*detach)(void);
    StaticIdentity (*dependency)(size_t);
    size_t dependency_count;
} StaticEntry;

typedef struct
{
    uint32_t abi_version;
    uint32_t reserved;
    uint8_t identity[32];
    StaticEntry (*static_entry)(size_t);
    size_t static_count;
} HostDescriptor;

static StaticEntry no_statics(size_t ordinal)
{
    (void)ordinal;
    abort();
}

/* The Bray archive contributes code. This C final-image host owns its descriptor.
   There are no static contributions. The ABI still requires a non-null lookup. */
const HostDescriptor bray_linked_product_host = {
    1, 0, {0x42, 0x52, 0x41, 0x35, 0x35, 0x34}, no_statics, 0
};
extern HostObservation bray_runtime_product_host_control(const void*, uint32_t);
extern uint64_t bray_runtime_current_native_thread_identity(void);
extern uint64_t bray_entry_value(uint64_t);
extern uint64_t bray_entry_nested(uint64_t);
extern Owner bray_entry_create(void);
extern void bray_entry_dispose(Owner);
extern uint64_t bray_entry_returned_owner(void);
extern uint64_t bray_entry_closing(void);

static uint64_t events[4];

void bray_entry_record(uint32_t event)
{
    assert(event < 4);
    assert(bray_runtime_current_native_thread_identity() != 0);
    ++events[event];
}

uint64_t bray_entry_count(uint32_t event)
{
    assert(event < 4);

    return events[event];
}

uint64_t bray_entry_reenter(uint64_t value)
{
    uint64_t identity = bray_runtime_current_native_thread_identity();
    HostObservation before = bray_runtime_product_host_control(&bray_linked_product_host, 6);

    assert(identity != 0 && before.active_entries == 1);

    uint64_t result = bray_entry_value(value);
    HostObservation after = bray_runtime_product_host_control(&bray_linked_product_host, 6);

    assert(bray_runtime_current_native_thread_identity() == identity);
    assert(after.active_entries == 1);

    return result;
}

uint64_t bray_entry_close_inside(void)
{
    HostObservation closing = bray_runtime_product_host_control(&bray_linked_product_host, 5);
    uint64_t before = events[1];

    assert(closing.status == 1 && closing.state == 2 && closing.active_entries == 1);
    assert(bray_entry_value(99) == 0);
    assert(events[1] == before);

    return 73;
}

static void measure_entry(void)
{
    const uint64_t iterations = 100000;

    for (uint64_t index = 0; index < 1000; ++index)
    {
        assert(bray_entry_value(index) == index + 1);
    }

    clock_t begin = clock();

    assert(begin != (clock_t)-1);

    for (uint64_t index = 0; index < iterations; ++index)
    {
        assert(bray_entry_value(index) == index + 1);
    }

    clock_t end = clock();

    assert(end != (clock_t)-1 && end >= begin);

    printf("foreign_entry iterations=100000 clock_seconds=%.9f\n",
           (double)(end - begin) / CLOCKS_PER_SEC);
}

int main(void)
{
    assert(bray_runtime_current_native_thread_identity() == 0);
    assert(bray_entry_value(5) == 6);
    assert(bray_runtime_current_native_thread_identity() == 0);
    assert(bray_entry_nested(9) == 10);
    assert(bray_runtime_current_native_thread_identity() == 0);

    Owner owner = bray_entry_create();

    assert(owner.value == 73 && events[2] == 0 && events[3] == 1);
    assert(bray_runtime_current_native_thread_identity() == 0);
    bray_entry_dispose(owner);
    assert(events[2] == 1);
    assert(bray_entry_returned_owner() == 73);
    assert(events[2] == 2 && events[3] == 2);

    measure_entry();
    assert(bray_runtime_current_native_thread_identity() == 0);

    assert(bray_entry_closing() == 73);

    HostObservation closed = bray_runtime_product_host_control(&bray_linked_product_host, 6);

    assert(closed.status == 2 && closed.state == 3 && closed.active_entries == 0);
    assert(closed.cleanup_incidents == 0);

    uint64_t before = events[1];

    assert(bray_entry_value(99) == 0 && events[1] == before);
    assert(bray_runtime_current_native_thread_identity() == 0);

    return 0;
}
