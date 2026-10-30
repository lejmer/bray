#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "provider-host.h"

#if defined(_WIN32)
#include <windows.h>
#else
#include <pthread.h>
#include <sched.h>
#endif

typedef struct
{
    const product_host_descriptor* descriptor;
    const provider_binding* binding;
    provider_observation result;
} formation_context;

extern uint32_t bray_allocation_probe_begin(void);
extern uint32_t bray_allocation_probe_end(uint32_t);

static void verify_rejected_bindings(const product_host_descriptor* descriptor, provider_host_owner* owner)
{
    provider_binding binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)owner, &provider_host_callbacks}, NULL, 0
    };
    provider_observation rejected = {0};

    binding.services = 0;
    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &rejected) == 0);
    assert(rejected.observation.status == 3);
    binding.services = 1;
    binding.version = 1;
    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &rejected) == 0);
    assert(rejected.observation.status == 3);
    binding.version = 2;
    binding.domain = NULL;
    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &rejected) == 0);
    assert(rejected.observation.status == 3);
    binding.domain = &provider_host_domain;

    uint32_t before = bray_allocation_probe_begin();
    uint32_t status = descriptor->provider->form((uintptr_t)descriptor, &binding, &rejected);
    uint32_t attempts = bray_allocation_probe_end(before);

    assert(status == 0 && rejected.observation.status == 5 && attempts != 0);
    assert(rejected.handle.epoch == 0 && atomic_load(descriptor->activation) == 0);
    assert(provider_host_references((uintptr_t)owner) == 1);
}

static void yield_native(void)
{
#if defined(_WIN32)
    SwitchToThread();
#else
    sched_yield();
#endif
}

static void form_until_published(formation_context* context)
{
    for (unsigned attempt = 0; attempt < 10000; attempt += 1)
    {
        assert(context->descriptor->provider->form(
            (uintptr_t)context->descriptor, context->binding, &context->result) == 0);

        if (context->result.observation.status == 0)
        {
            assert(context->result.observation.state == 1);
            return;
        }

        assert(context->result.observation.status == 5);
        yield_native();
    }

    abort();
}

#if defined(_WIN32)
static DWORD WINAPI formation_thread(LPVOID value)
{
    form_until_published((formation_context*)value);
    return 0;
}
#else
static void* formation_thread(void* value)
{
    form_until_published((formation_context*)value);
    return NULL;
}
#endif

static provider_handle concurrent_formation(const product_host_descriptor* descriptor, provider_host_owner* owner)
{
    provider_binding binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)owner, &provider_host_callbacks}, NULL, 0
    };
    formation_context contexts[4] = {0};

#if defined(_WIN32)
    HANDLE threads[4];
#else
    pthread_t threads[4];
#endif

    for (unsigned index = 0; index < 4; index += 1)
    {
        contexts[index].descriptor = descriptor;
        contexts[index].binding = &binding;

#if defined(_WIN32)
        threads[index] = CreateThread(NULL, 0, formation_thread, &contexts[index], 0, NULL);
        assert(threads[index] != NULL);
#else
        assert(pthread_create(&threads[index], NULL, formation_thread, &contexts[index]) == 0);
#endif
    }

    for (unsigned index = 0; index < 4; index += 1)
    {
#if defined(_WIN32)
        assert(WaitForSingleObject(threads[index], INFINITE) == WAIT_OBJECT_0);
        CloseHandle(threads[index]);
#else
        assert(pthread_join(threads[index], NULL) == 0);
#endif
        assert(contexts[index].result.handle.epoch == contexts[0].result.handle.epoch);
    }

    assert(provider_host_references((uintptr_t)owner) == 2);
    return contexts[0].result.handle;
}

static void verify_two_loads(const char* first_path, const char* second_path)
{
    void* first_library = provider_host_open(first_path);
    void* second_library = provider_host_open(second_path);
    const product_host_descriptor* first =
        (const product_host_descriptor*)provider_host_symbol(first_library, "bray_linked_product_host");
    const product_host_descriptor* second =
        (const product_host_descriptor*)provider_host_symbol(second_library, "bray_linked_product_host");

    assert(first_library != NULL && second_library != NULL && first_library != second_library);
    assert(first != NULL && second != NULL && first != second);
    assert(memcmp(first->identity, second->identity, sizeof(first->identity)) == 0);

    provider_host_owner first_owner = {first_library, ATOMIC_VAR_INIT(1), (uintptr_t)first, 0, NULL};
    provider_host_owner second_owner = {second_library, ATOMIC_VAR_INIT(1), (uintptr_t)second, 0, NULL};

    verify_rejected_bindings(first, &first_owner);

    provider_handle first_handle = concurrent_formation(first, &first_owner);
    provider_binding second_binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&second_owner, &provider_host_callbacks}, &first_handle, 1
    };
    provider_observation second_formed = {0};

    assert(second->provider->form((uintptr_t)second, &second_binding, &second_formed) == 0);
    assert(second_formed.observation.status == 0 && second_formed.handle.epoch != first_handle.epoch);

    // A new edge cannot rebind an existing activation into a dynamic cycle.
    provider_binding cycle =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&first_owner, &provider_host_callbacks}, &second_formed.handle, 1
    };
    provider_observation rejected = {0};

    assert(first->provider->form((uintptr_t)first, &cycle, &rejected) == 0);
    assert(rejected.observation.status == 3);
    assert(provider_host_references((uintptr_t)&first_owner) == 2);

    // A copied descriptor cannot reuse another live activation slot.
    product_host_descriptor alias = *first;
    provider_binding alias_binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&first_owner, &provider_host_callbacks}, NULL, 0
    };

    assert(first->provider->form((uintptr_t)&alias, &alias_binding, &rejected) == 0);
    assert(rejected.observation.status == 3);

    provider_host_release((uintptr_t)&first_owner);
    provider_host_release((uintptr_t)&second_owner);

    provider_observation closed = {0};

    assert(provider_host_domain.control(first_handle.descriptor, first_handle.epoch, 5, &closed) == 0);
    assert(closed.observation.state == 2 && closed.observation.cleaned_statics == 0);

    assert(provider_host_domain.control(second_formed.handle.descriptor, second_formed.handle.epoch, 5, &closed) == 0);
    assert(closed.observation.state == 3);
    assert(first_owner.closes == 1 && second_owner.closes == 0);

    provider_retirement retired = {0};

    assert(provider_host_domain.retire(second_formed.handle.descriptor, second_formed.handle.epoch, &retired) == 0);
    assert(retired.observation.status == 0 && second_owner.closes == 0);
    retired.provider.callbacks->release(retired.provider.context);
    assert(second_owner.closes == 1);
}

static unsigned admitted;
static unsigned rolled_back;
static unsigned destroyed;
static unsigned detached;

static void fake_admission(uint32_t operation, void* outcome)
{
    memset(outcome, 0, 168);

    if (operation == 0)
    {
        admitted += 1;
    }
    else
    {
        rolled_back += 1;
    }
}

static void fake_prepare(void) {}
static void fake_detach(void) { detached += 1; }
static uintptr_t fake_access(void) { return 0; }

static static_identity fake_dependency(size_t ordinal)
{
    (void)ordinal;
    abort();
}

static void fake_destroy(uint32_t abandoned, void* outcome)
{
    (void)abandoned;
    memset(outcome, 0, 168);
    destroyed += 1;
}

static static_host_entry invalid_second_static(size_t ordinal)
{
    static_host_entry entry = {0};

    entry.abi_version = ordinal == 0 ? 2 : 0;
    entry.identity.bytes[0] = (uint8_t)(ordinal + 1);
    entry.order = ordinal;
    entry.access = fake_access;
    entry.prepare = fake_prepare;
    entry.finalizer.admission = fake_admission;
    entry.destroy = fake_destroy;
    entry.detach = fake_detach;
    entry.dependency = fake_dependency;

    return entry;
}

static atomic_uint observation_phase;
static provider_handle observed_handle;

static void pause_observation(provider_host_owner* owner)
{
    owner->observe = NULL;
    atomic_store_explicit(&observation_phase, 1, memory_order_release);

    while (atomic_load_explicit(&observation_phase, memory_order_acquire) != 2)
    {
        yield_native();
    }
}

#if defined(_WIN32)
static DWORD WINAPI release_thread(LPVOID value)
{
    provider_host_release((uintptr_t)value);
    return 0;
}
#else
static void* release_thread(void* value)
{
    provider_host_release((uintptr_t)value);
    return NULL;
}
#endif

static void verify_failed_formation_and_retirement_race(const char* path)
{
    void* library = provider_host_open(path);
    const product_host_descriptor* original =
        (const product_host_descriptor*)provider_host_symbol(library, "bray_linked_product_host");
    product_host_descriptor descriptor = *original;
    atomic_uintptr_t activation = ATOMIC_VAR_INIT(0);

    descriptor.static_entry = invalid_second_static;
    descriptor.static_count = 2;
    descriptor.activation = &activation;

    provider_host_owner owner = {library, ATOMIC_VAR_INIT(1), (uintptr_t)&descriptor, 0, NULL};
    provider_binding binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&owner, &provider_host_callbacks}, NULL, 0
    };
    provider_observation failed = {0};

    assert(descriptor.provider->form((uintptr_t)&descriptor, &binding, &failed) == 0);
    assert(failed.observation.status != 0 && failed.observation.state == 4);
    assert(failed.handle.epoch != 0 && provider_host_references((uintptr_t)&owner) == 2);
    assert(admitted == 1 && rolled_back == 1 && destroyed == 1 && detached == 1);

    observed_handle = failed.handle;
    owner.observe = pause_observation;

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, release_thread, &owner, 0, NULL);

    assert(thread != NULL);
#else
    pthread_t thread;

    assert(pthread_create(&thread, NULL, release_thread, &owner) == 0);
#endif

    while (atomic_load_explicit(&observation_phase, memory_order_acquire) != 1)
    {
        yield_native();
    }

    // This entered resident callback acquires and releases its pin completely
    // during the off-lock native reference observation.
    provider_observation entered = {0};

    assert(provider_host_domain.control(observed_handle.descriptor, observed_handle.epoch, 6, &entered) == 0);
    assert(entered.handle.epoch == observed_handle.epoch);
    atomic_store_explicit(&observation_phase, 2, memory_order_release);

#if defined(_WIN32)
    assert(WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0);
    CloseHandle(thread);
#else
    assert(pthread_join(thread, NULL) == 0);
#endif

    assert(owner.closes == 0);
    assert(admitted == 1 && rolled_back == 1 && destroyed == 1 && detached == 1);

    provider_retirement retired = {0};

    assert(provider_host_domain.retire(failed.handle.descriptor, failed.handle.epoch, &retired) == 0);
    assert(retired.observation.status == 0 && owner.closes == 0);
    assert(atomic_load(&activation) == 1);
    retired.provider.callbacks->release(retired.provider.context);
    assert(owner.closes == 1);

    provider_observation stale = {0};

    assert(provider_host_domain.control(failed.handle.descriptor, failed.handle.epoch, 6, &stale) == 0);
    assert(stale.observation.status == 3);
}

static void verify_bound_domain_export(const char* path)
{
    void* library = provider_host_open(path);

    assert(library != NULL);

    const product_host_descriptor* descriptor =
        (const product_host_descriptor*)provider_host_symbol(library, "bray_linked_product_host");
    uint32_t (*domain)(provider_domain*) =
        (uint32_t (*)(provider_domain*))provider_host_symbol(library, "bray_fixture_provider_domain");

    assert(descriptor != NULL && domain != NULL);

    provider_host_owner owner = {library, ATOMIC_VAR_INIT(1), (uintptr_t)descriptor, 0, NULL};
    provider_binding binding =
    {
        2, 1, &provider_host_domain, {(uintptr_t)&owner, &provider_host_callbacks}, NULL, 0
    };
    provider_observation formed = {0};

    assert(descriptor->provider->form((uintptr_t)descriptor, &binding, &formed) == 0);
    assert(formed.observation.status == 0);

    provider_domain exported = {0};
    uint32_t before = bray_allocation_probe_begin();

    assert(domain(&exported) == 0);
    assert(bray_allocation_probe_end(before) == 0);
    assert(exported.host == provider_host_domain.host &&
        exported.synchronization == provider_host_domain.synchronization &&
        exported.pool == provider_host_domain.pool &&
        exported.callbacks == provider_host_domain.callbacks &&
        exported.consumer == provider_host_domain.consumer &&
        exported.control == provider_host_domain.control &&
        exported.retire == provider_host_domain.retire &&
        exported.notify == provider_host_domain.notify);

    provider_observation closing = {0};

    assert(provider_host_domain.control(formed.handle.descriptor, formed.handle.epoch, 5, &closing) == 0);
    assert(closing.observation.status == 1 && owner.closes == 0);
    provider_host_release((uintptr_t)&owner);
    assert(owner.closes == 1);
}

int main(int count, char** arguments)
{
    assert(count == 4);
    provider_host_initialize();
    verify_two_loads(arguments[1], arguments[2]);
    verify_failed_formation_and_retirement_race(arguments[1]);
    verify_bound_domain_export(arguments[3]);
    puts("provider formation: concurrent publication, dependency order, rollback, retirement race");
    return 0;
}
