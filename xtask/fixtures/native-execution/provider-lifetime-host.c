#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "provider-host.h"

#if defined(_WIN32)
#include <windows.h>
#else
#include <pthread.h>
#endif

struct provider_outcome;
struct provider_report;

typedef struct provider_report
{
    uint32_t source[4];
    uint64_t version;
    uint8_t namespace_identity[32];
    uint32_t cause;
    uint32_t primary_present;
    uintptr_t message;
    size_t message_length;
    uint32_t (*copy_message)(uintptr_t, size_t, uint8_t*, size_t);
    void (*release_message)(uintptr_t, size_t, struct provider_outcome*);
    provider_reference provider;
    size_t head;
    size_t tail;
    size_t count;
    size_t reserved;
    uint32_t (*consume)(struct provider_report*, uint32_t);
} provider_report;

_Static_assert(sizeof(provider_report) == 152, "provider report native layout");
_Static_assert(sizeof(product_host_descriptor) == 80, "provider descriptor native layout");
_Static_assert(sizeof(provider_domain) == 64, "resident domain native layout");

typedef void (*produce_report)(provider_report*);
typedef int32_t* (*state_address)(void);

typedef struct
{
    produce_report produce;
    provider_report* destination;
} producer_context;

#if defined(_WIN32)
static DWORD WINAPI producer_thread(LPVOID value)
{
    producer_context* context = (producer_context*)value;

    context->produce(context->destination);

    return 0;
}
#else
static void* producer_thread(void* value)
{
    producer_context* context = (producer_context*)value;

    context->produce(context->destination);

    return NULL;
}
#endif

static void produce_on_thread(produce_report produce, provider_report* destination)
{
    producer_context context = {produce, destination};

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, producer_thread, &context, 0, NULL);

    assert(thread != NULL && WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0);
    CloseHandle(thread);
#else
    pthread_t thread;

    assert(pthread_create(&thread, NULL, producer_thread, &context) == 0);
    assert(pthread_join(thread, NULL) == 0);
#endif
}

extern uint32_t bray_allocation_probe_begin(void);
extern uint32_t bray_allocation_probe_end(uint32_t);
extern void bray_allocation_probe_snapshot(unsigned*, unsigned*, size_t*, size_t*);
extern provider_report bray_runtime_panic_report_suppression(provider_report*, provider_report*);
typedef struct
{
    size_t head, tail, count;
} provider_records;
extern uint32_t bray_runtime_report_records_admit(size_t, provider_records*);

static provider_report take_primary(provider_report* report)
{
    provider_report primary = *report;

    primary.head = 0;
    primary.tail = 0;
    primary.count = 0;
    primary.reserved = 0;
    report->primary_present = 0;
    report->message = 0;
    report->message_length = 0;
    report->copy_message = NULL;
    report->release_message = NULL;
    report->provider = (provider_reference){0};

    return primary;
}

static int check_report(const char* path, uint64_t* previous_epoch, unsigned abnormal, const char* normal, const char* failing, const char* state_symbol)
{
    void* library = provider_host_open(path);

    assert(library != NULL);

    const product_host_descriptor* descriptor =
        (const product_host_descriptor*)provider_host_symbol(library, "bray_linked_product_host");
    produce_report produce = (produce_report)provider_host_symbol(library,
        abnormal ? failing : normal);
    state_address state = (state_address)provider_host_symbol(library, state_symbol);

    assert(descriptor != NULL && produce != NULL && state != NULL);
    assert(descriptor->execution == NULL);
    assert(descriptor->provider != NULL && descriptor->activation != NULL);

    provider_host_owner owner = {library, ATOMIC_VAR_INIT(1), (uintptr_t)descriptor, 0};
    provider_binding binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&owner, &provider_host_callbacks}, NULL, 0
    };
    provider_observation formed = {0};
    unsigned allocations_before, live_before;
    size_t bytes_before, retained_before;

    bray_allocation_probe_snapshot(&allocations_before, &live_before, &bytes_before, &retained_before);

    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &formed) == 0);
    assert(formed.observation.status == 0 && formed.observation.state == 1);
    assert(formed.handle.epoch > *previous_epoch);
    assert(descriptor->static_count >= 1);
    assert(formed.observation.initialized_statics == descriptor->static_count);

    unsigned allocations_after, live_after;
    size_t bytes_after, retained_after;

    bray_allocation_probe_snapshot(&allocations_after, &live_after, &bytes_after, &retained_after);
    printf("formation epoch=%llu allocations=%u bytes=%zu retained=%u\n",
        (unsigned long long)formed.handle.epoch, allocations_after - allocations_before,
        bytes_after - bytes_before, live_after - live_before);

    *previous_epoch = formed.handle.epoch;

    provider_observation duplicate = {0};

    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &duplicate) == 0);
    assert(duplicate.observation.status == 0 && duplicate.handle.epoch == formed.handle.epoch);
    assert(provider_host_references((uintptr_t)&owner) == 2);

    int32_t* live = state();
    provider_report report = {0};

    produce_on_thread(produce, &report);
    assert(*live == 739391 && report.primary_present == 1 && report.count == 1);
    assert(report.provider.callbacks != NULL);
    assert(report.consume != NULL && (void*)report.consume == provider_host_domain.consumer);

    char history[64] = {0};

    assert(report.message_length < sizeof(history));
    assert(report.copy_message(report.message, 0, (uint8_t*)history, report.message_length) == 0);
    assert(strcmp(history, "retained provider report") == 0);

    provider_host_release((uintptr_t)&owner);

    provider_observation closing = {0};

    assert(provider_host_domain.control(formed.handle.descriptor, formed.handle.epoch, 5, &closing) == 0);
    assert(closing.observation.status == 1 && closing.observation.state == 2);
    assert(closing.observation.cleaned_statics == 0 && *live == 739391 && owner.closes == 0);

    provider_observation rejected = {0};

    assert(provider_host_domain.control(formed.handle.descriptor, formed.handle.epoch, 3, &rejected) == 0);
    assert(rejected.observation.status == 2);

    // Transfer the primary into a separate owner. The remaining typed incident
    // must retain the entire provider after that primary has been consumed.
    provider_report primary = take_primary(&report);

    uint32_t before = bray_allocation_probe_begin();

    // Existing owners can retain disposal authority after entry closure. Known
    // roots use their stable cell without registry lookup or allocation.
    for (unsigned clone = 0; clone < 1024; clone += 1)
    {
        primary.provider.callbacks->retain(primary.provider.context);
        primary.provider.callbacks->release(primary.provider.context);
    }

    assert(primary.consume(&primary, 0) == 0);
    assert(owner.closes == 0 && *live == 739391);
    uint32_t consumed = report.consume(&report, 0);
    uint32_t attempts = bray_allocation_probe_end(before);

    assert(consumed == (abnormal ? 5 : 0));
    assert(abnormal || attempts == 0);
    assert(owner.closes == 1);
    assert(strcmp(history, "retained provider report") == 0);

    provider_observation stale = {0};

    assert(provider_host_domain.control(formed.handle.descriptor, formed.handle.epoch, 6, &stale) == 0);
    assert(stale.observation.status == 3);

    return 0;
}

static void check_mixed_reports(const char* first, const char* second)
{
    const char* paths[2] = {first, second};
    provider_host_owner owners[2] = {0};
    provider_observation formed[2] = {0};
    provider_report reports[2] = {0};

    for (unsigned index = 0; index < 2; index += 1)
    {
        void* library = provider_host_open(paths[index]);

        assert(library != NULL);

        const product_host_descriptor* descriptor =
            (const product_host_descriptor*)provider_host_symbol(library, "bray_linked_product_host");
        produce_report produce = (produce_report)provider_host_symbol(library, "bray_provider_report");

        assert(descriptor != NULL && produce != NULL);
        owners[index].library = library;
        owners[index].descriptor = (uintptr_t)descriptor;
        atomic_init(&owners[index].references, 1);

        provider_binding binding =
        {
            2, 1, &provider_host_domain,
            {(uintptr_t)&owners[index], &provider_host_callbacks}, NULL, 0
        };

        assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &formed[index]) == 0);
        assert(formed[index].observation.status == 0);

        // Native composition admits its outgoing record before accepting the
        // second primary, just as generated suppression does.
        provider_records reservation = {0};

        if (index == 1)
        {
            assert(bray_runtime_report_records_admit(1, &reservation) == 0);
        }

        produce_on_thread(produce, &reports[index]);

        if (index == 1)
        {
            assert(reports[index].reserved == 0);
            reports[index].reserved = reservation.head;
        }

        provider_host_release((uintptr_t)&owners[index]);

        provider_observation closing = {0};

        assert(provider_host_domain.control((uintptr_t)descriptor, formed[index].handle.epoch, 5, &closing) == 0);
        assert(closing.observation.status == 1 && owners[index].closes == 0);
    }

    assert(formed[0].handle.epoch != formed[1].handle.epoch);

    provider_report combined = bray_runtime_panic_report_suppression(&reports[0], &reports[1]);

    assert(combined.count == 3 && combined.consume != NULL);

    provider_report primary = take_primary(&combined);
    uint32_t before = bray_allocation_probe_begin();

    assert(primary.consume(&primary, 0) == 0);
    assert(owners[0].closes == 0 && owners[1].closes == 0);
    assert(combined.consume(&combined, 0) == 0);
    assert(bray_allocation_probe_end(before) == 0);
    assert(owners[0].closes == 1 && owners[1].closes == 1);

    puts("mixed report providers: both unload through the resident consumer");
}

int main(int count, char** arguments)
{
    assert(count == 2 || count == 3 || count == 5);
    provider_host_initialize();

    uint64_t epoch = 0;
    unsigned retained = 0;
    size_t retained_bytes = 0;

    for (unsigned cycle = 0; cycle < 8; cycle += 1)
    {
        assert(check_report(arguments[1], &epoch, cycle % 2,
            count == 5 ? arguments[2] : "bray_provider_report",
            count == 5 ? arguments[3] : "bray_provider_abnormal_report",
            count == 5 ? arguments[4] : "bray_provider_state") == 0);

        unsigned attempts, live;
        size_t bytes, live_bytes;

        bray_allocation_probe_snapshot(&attempts, &live, &bytes, &live_bytes);

        if (cycle == 1)
        {
            retained = live;
            retained_bytes = live_bytes;
        }
        else if (cycle > 1)
        {
            assert(live == retained);
            assert(live_bytes == retained_bytes);
        }

        printf("retired cycle=%u live=%u retained_bytes=%zu cumulative_allocations=%u bytes=%zu\n",
            cycle, live, live_bytes, attempts, bytes);
    }

    puts("provider lifetime cycles: 8; scheduler bindings: 0");

    if (count == 3)
    {
        check_mixed_reports(arguments[1], arguments[2]);
    }

    return 0;
}
