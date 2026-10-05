#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "static_storage_host.h"

#if defined(_WIN32)
#include <windows.h>
typedef HMODULE native_library;
#else
#include <dlfcn.h>
#include <pthread.h>
typedef void *native_library;
#endif

typedef uintptr_t (*static_access)(void);
typedef void (*static_cleanup)(void *outcome);
typedef void (*static_transition)(void);
typedef uint32_t (*static_finalizer_start)(uintptr_t destination, void *outcome);
typedef uint32_t (*static_finalizer_resolve)(uintptr_t completion, uintptr_t incident, void *outcome);

typedef struct
{
    uint32_t execution;
    uint32_t outgoing_capacity;
    size_t result_size;
    size_t result_alignment;
    static_finalizer_start start;
    static_finalizer_resolve resolve;
} static_finalizer;

typedef struct
{
    uint8_t bytes[32];
} static_identity;

typedef static_identity (*static_dependency)(size_t index);

typedef struct static_host_entry
{
    uint32_t abi_version;
    uint32_t duration;
    static_identity identity;
    uint64_t order;
    uintptr_t storage;
    static_access access;
    static_transition prepare;
    static_finalizer finalizer;
    static_cleanup destroy;
    static_transition detach;
    static_dependency dependency;
    size_t dependency_count;
} static_host_entry;

typedef static_host_entry (*static_host_lookup)(size_t index);

typedef struct
{
    uint32_t abi_version;
    uint32_t services;
    uintptr_t domain;
    uintptr_t load_identity;
    provider_reference provider;
} product_binding;

typedef struct
{
    uint32_t abi_version;
    uint32_t required_services;
    uint8_t identity[32];
    static_host_lookup static_entry;
    size_t static_count;
    atomic_uintptr_t load;
    uintptr_t domain;
    product_binding binding;
} product_host_descriptor;

typedef struct
{
    uint32_t present, source, start, end;
    uint64_t version;
    uint8_t namespace[32];
} source_anchor;

typedef struct panic_report panic_report;
typedef struct run_outcome run_outcome;

struct panic_report
{
    source_anchor source;
    uint32_t cause;
    uintptr_t message;
    size_t message_length;
    uint32_t (*copy_message)(uintptr_t, size_t, void *, size_t);
    void (*release_message)(uintptr_t, size_t, run_outcome *);
    provider_reference provider;
    uintptr_t head, tail, count, reserved;
    uint32_t (*consume)(panic_report *, _Bool);
};

struct run_outcome
{
    uint32_t state;
    uintptr_t payload;
    panic_report report;
};

typedef struct
{
    uintptr_t payload;
    uint8_t type_identity[32];
    source_anchor source;
    uint32_t (*report)(uintptr_t);
    void (*destroy)(uintptr_t, run_outcome *, run_outcome *);
    provider_reference provider;
} cleanup_incident;

static void retain_provider(uintptr_t context)
{
    atomic_fetch_add((atomic_size_t *)context, 1);
}

static void release_provider(uintptr_t context)
{
    if (atomic_fetch_sub((atomic_size_t *)context, 1) == 0)
        abort();
}

static size_t provider_references(uintptr_t context)
{
    return atomic_load((atomic_size_t *)context);
}

static uintptr_t next_load = (UINTPTR_MAX / 2) + 1;

static void supply_binding(product_host_descriptor *descriptor, atomic_size_t *references)
{
    product_binding binding = {
        1, descriptor->required_services & 3, descriptor->domain, next_load++,
        { (uintptr_t)references, retain_provider, release_provider, provider_references }
    };

    descriptor->binding = binding;
}

static int consume_report(panic_report *report)
{
    provider_reference provider = report->provider;

    if (provider.retain == NULL || report->consume == NULL)
        return 1;

    // The host owns this callback guard until control has returned from the loaded image.
    provider.retain(provider.context);

    uint32_t status = report->consume(report, 0);

    provider.release(provider.context);

    return status != 0;
}


typedef struct
{
    product_host_control control;
    static_access access;
    static_access async_access;
    uintptr_t other_address;
    int result;
} thread_context;

static native_library open_library(const char *path)
{
#if defined(_WIN32)
    return LoadLibraryA(path);
#else
    return dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
}

static void *find_symbol(native_library library, const char *name)
{
#if defined(_WIN32)
    return (void *)GetProcAddress(library, name);
#else
    return dlsym(library, name);
#endif
}

static int close_library(native_library library)
{
#if defined(_WIN32)
    return FreeLibrary(library) ? 0 : 1;
#else
    return dlclose(library);
#endif
}

static product_host_observation retire_host(product_host_control control)
{
    product_host_observation observation = control(PRODUCT_HOST_RETIRE);

    // The successful result owns the formation pin until the image callback has returned.
    if (observation.retired_provider.release != NULL)
        observation.retired_provider.release(observation.retired_provider.context);

    memset(&observation.retired_provider, 0, sizeof(observation.retired_provider));

    return observation;
}

static int check_failed_formation(const char *path, const char *control_name, const char *descriptor_name, const char *probe_name, int mode)
{
    native_library library = open_library(path);
    product_host_control control = (product_host_control)find_symbol(library, control_name);
    product_host_descriptor *descriptor = (product_host_descriptor *)find_symbol(library, descriptor_name);
    atomic_size_t references = 0;

    if (library == NULL || control == NULL || descriptor == NULL)
        return 80;

    if (mode != 0)
        supply_binding(descriptor, &references);

    if (mode == 1)
        descriptor->binding.services = 0;
    if (mode == 2)
        descriptor->binding.domain ^= 1;
    if (mode == 3)
        descriptor->static_count = 1000001;
    if (mode == 4)
        descriptor->binding.services = 1;
    if (mode == 5)
        descriptor->binding.abi_version = 2;
    if (mode == 6)
        descriptor->binding.provider.release = NULL;
    if (mode == 7)
        descriptor->abi_version = 2;

    int32_t (*probe)(int32_t) = (int32_t (*)(int32_t))find_symbol(library, probe_name);

    if (probe == NULL || probe(42) != 0)
        return 100;

    if (control(PRODUCT_HOST_FORM).status != 3)
        return 81;

    uintptr_t failed_load = atomic_load(&descriptor->load);

    if (failed_load == 0 || failed_load == UINTPTR_MAX || control(PRODUCT_HOST_FORM).status != 3 || atomic_load(&descriptor->load) != failed_load)
        return 82;

    if (atomic_load(&references) != (mode == 3 ? 1 : 0))
        return 83;

    if (retire_host(control).status != 0 || atomic_load(&references) != 0 || atomic_load(&descriptor->load) != UINTPTR_MAX)
        return 84;

    if (control(PRODUCT_HOST_FORM).status != 3)
        return 85;

    return close_library(library) == 0 ? 0 : 86;
}

static int observation_is(product_host_observation observation, uint32_t status, uint32_t state)
{
    return observation.status == status && observation.state == state;
}

static int identity_is(static_identity left, static_identity right)
{
    return memcmp(left.bytes, right.bytes, sizeof(left.bytes)) == 0;
}

static static_access find_thread_access(
    const product_host_descriptor *descriptor,
    static_identity identity,
    uint32_t finalizer_execution
)
{
    static_access result = NULL;

    for (size_t index = 0; index < descriptor->static_count; index += 1)
    {
        static_host_entry entry = descriptor->static_entry(index);

        if (
            entry.duration != 1 ||
            entry.finalizer.execution != finalizer_execution ||
            !identity_is(entry.identity, identity)
        )
            continue;

        if (result != NULL)
            return NULL;

        result = entry.access;
    }

    return result;
}

static uintptr_t find_product_value_address(
    const static_host_entry *entries,
    size_t count,
    int32_t expected
)
{
    uintptr_t result = 0;

    for (size_t index = 0; index < count; index += 1)
    {
        int32_t value = 0;
        memcpy(&value, (void *)entries[index].storage, sizeof(value));

        if (value != expected)
            continue;

        if (result != 0)
            return 0;

        result = entries[index].storage;
    }

    return result;
}

static int run_thread_check(thread_context *context)
{
    product_host_observation attached = context->control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

    if (!observation_is(attached, 0, 1))
        return 1;

    uintptr_t first = context->access();
    uintptr_t second = context->access();
    uintptr_t cleanup = context->async_access();

    if (first == 0 || first != second || first == context->other_address || cleanup == 0)
        return 2;

    if (*(int32_t *)first != 42 || *(int32_t *)cleanup != 192837465)
        return 3;

    product_host_observation detached = context->control(PRODUCT_HOST_DETACH_CURRENT_THREAD);

    if (!observation_is(detached, 0, 1) || detached.cleanup_incidents != 0)
        return 4;

    return 0;
}

#if defined(_WIN32)
static DWORD WINAPI thread_entry(LPVOID value)
{
    thread_context *context = (thread_context *)value;
    context->result = run_thread_check(context);
    return 0;
}
#else
static void *thread_entry(void *value)
{
    thread_context *context = (thread_context *)value;
    context->result = run_thread_check(context);
    return NULL;
}
#endif

static int check_thread_static(
    product_host_control control,
    static_access access,
    static_access async_access
)
{
    product_host_observation attached = control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

    if (!observation_is(attached, 0, 1))
        return 20;

    uintptr_t first = access();
    uintptr_t second = access();
    uintptr_t cleanup = async_access();

    if (
        first == 0 ||
        first != second ||
        *(int32_t *)first != 42 ||
        cleanup == 0 ||
        *(int32_t *)cleanup != 192837465
    )
        return 21;

    *(int32_t *)first = 99;

    thread_context context = {control, access, async_access, first, 0};

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, thread_entry, &context, 0, NULL);

    if (thread == NULL || WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0)
        return 22;

    CloseHandle(thread);
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, thread_entry, &context) != 0)
        return 22;

    if (pthread_join(thread, NULL) != 0)
        return 23;
#endif

    if (context.result != 0 || *(int32_t *)first != 99)
        return 24 + context.result;

    if (!observation_is(control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1))
        return 30;

    if (!observation_is(control(PRODUCT_HOST_ATTACH_CURRENT_THREAD), 0, 1))
        return 31;

    uintptr_t reattached = access();

    if (reattached == 0 || *(int32_t *)reattached != 42)
        return 32;

    if (!observation_is(control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1))
        return 33;

    return 0;
}

static int check_product_scoped_thread_statics(
    product_host_control first_control,
    const product_host_descriptor *first_descriptor,
    product_host_control second_control,
    const product_host_descriptor *second_descriptor,
    static_identity thread_identity
)
{
    static_access first_access = find_thread_access(first_descriptor, thread_identity, 0);
    static_access second_access = find_thread_access(second_descriptor, thread_identity, 0);

    if (first_access == NULL || second_access == NULL)
        return 34;

    if (
        !observation_is(first_control(PRODUCT_HOST_ATTACH_CURRENT_THREAD), 0, 1) ||
        !observation_is(second_control(PRODUCT_HOST_ATTACH_CURRENT_THREAD), 0, 1)
    )
        return 35;

    uintptr_t first = first_access();
    uintptr_t second = second_access();

    if (first == 0 || second == 0 || first == second)
        return 36;

    *(int32_t *)first = 71;
    *(int32_t *)second = 72;

    if (!observation_is(first_control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1))
        return 37;

    if (first_access() != 0 || second_access() != second || *(int32_t *)second != 72)
        return 38;

    if (!observation_is(second_control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1))
        return 39;

    return 0;
}

static int exercise_host(
    product_host_control control,
    const product_host_descriptor *descriptor,
    size_t *product_static_count,
    uintptr_t *representative_address,
    static_identity thread_identity,
    static_identity async_thread_identity
)
{
    product_host_observation formed = control(PRODUCT_HOST_FORM);

    if (!observation_is(formed, 0, 1) || descriptor->abi_version != 1)
        return 40;

    if (descriptor->static_count < 6 || descriptor->static_entry == NULL)
        return 41;

    static_access thread_access = find_thread_access(descriptor, thread_identity, 0);
    static_access async_thread_access = find_thread_access(descriptor, async_thread_identity, 2);
    static_host_entry product_entries[64];

    size_t product_count = 0;
    int found_static_relocation = 0;

    if (descriptor->static_count > 64)
        return 42;

    for (size_t index = 0; index < descriptor->static_count; index += 1)
    {
        static_host_entry entry_value = descriptor->static_entry(index);
        const static_host_entry *entry = &entry_value;

        if (entry->abi_version != 1 || entry->order != index)
            return 43;

        if (entry->duration == 0)
        {
            uintptr_t first = entry->access();
            uintptr_t second = entry->access();

            if (first == 0 || first != second || first != entry->storage)
                return 44;

            for (size_t previous = 0; previous < product_count; previous += 1)
            {
                if (product_entries[previous].storage == first)
                    return 45;
            }

            product_entries[product_count] = entry_value;
            product_count += 1;
        }
        else if (entry->duration != 1)
        {
            return 47;
        }
    }

    if (product_count < 4 || thread_access == NULL || async_thread_access == NULL)
        return 48;

    for (size_t candidate = 0; candidate < product_count; candidate += 1)
    {
        if (product_entries[candidate].dependency_count == 0)
            continue;

        uintptr_t relocated = 0;
        memcpy(&relocated, (void *)product_entries[candidate].storage, sizeof(relocated));

        static_identity dependency = product_entries[candidate].dependency(0);

        for (size_t target = 0; target < product_count; target += 1)
        {
            if (
                candidate != target &&
                relocated == product_entries[target].storage &&
                identity_is(dependency, product_entries[target].identity)
            )
                found_static_relocation = 1;
        }
    }

    if (!found_static_relocation)
        return 49;

    uintptr_t cleanup_probe = find_product_value_address(
        product_entries,
        product_count,
        135791113
    );

    if (cleanup_probe == 0)
        return 69;

    uintptr_t async_cleanup_probe = find_product_value_address(
        product_entries,
        product_count,
        975318642
    );

    if (async_cleanup_probe == 0)
        return 70;

    uintptr_t async_failing_cleanup_probe = find_product_value_address(
        product_entries,
        product_count,
        864209753
    );

    if (async_failing_cleanup_probe == 0)
        return 73;

    int thread_result = check_thread_static(control, thread_access, async_thread_access);

    if (thread_result != 0)
        return thread_result;

    if (!observation_is(control(PRODUCT_HOST_ACQUIRE_ENTRY), 0, 1))
        return 50;

    if (!observation_is(control(PRODUCT_HOST_ACQUIRE_EXTERNAL), 0, 1))
        return 51;

    if (!observation_is(control(PRODUCT_HOST_CLOSE), 1, 2))
        return 52;

    if (control(PRODUCT_HOST_ACQUIRE_ENTRY).status != 2)
        return 53;

    if (!observation_is(control(PRODUCT_HOST_RELEASE_ENTRY), 1, 2))
        return 54;

    product_host_observation closed = control(PRODUCT_HOST_RELEASE_EXTERNAL);

    if (
        !observation_is(closed, 4, 3) ||
        closed.cleaned_statics != product_count ||
        closed.cleanup_incidents != 1
    )
        return 55;

    int32_t cleaned_probe = 0;
    memcpy(&cleaned_probe, (void *)cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 59;

    memcpy(&cleaned_probe, (void *)async_cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 71;

    memcpy(&cleaned_probe, (void *)async_failing_cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 74;

    product_host_observation formed_again = control(PRODUCT_HOST_FORM);

    if (
        !observation_is(formed_again, 4, 3) ||
        formed_again.cleaned_statics != product_count
    )
        return 60;

    memcpy(&cleaned_probe, (void *)cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 61;

    memcpy(&cleaned_probe, (void *)async_cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 72;

    memcpy(&cleaned_probe, (void *)async_failing_cleanup_probe, sizeof(cleaned_probe));

    if (cleaned_probe != 0)
        return 75;

    *product_static_count = product_count;
    *representative_address = product_entries[0].storage;

    return 0;
}

typedef struct
{
    product_host_descriptor *descriptor;
    product_host_control control;
    run_outcome escaped;
    cleanup_incident error;
    int result;
} producer_context;

static int produce_escaped(producer_context *context)
{
    if (context->control(PRODUCT_HOST_ACQUIRE_ENTRY).status != 0)
        return 97;

    int found_report = 0;
    int found_error = 0;

    for (size_t index = 0; index < context->descriptor->static_count; index += 1)
    {
        static_host_entry entry = context->descriptor->static_entry(index);

        if (entry.duration != 0)
            continue;

        int32_t marker = 0;

        memcpy(&marker, (void *)entry.access(), sizeof(marker));

        if (marker == 123789456)
        {
            *(int32_t *)entry.storage = 0;
            entry.finalizer.start(0, &context->escaped);
            found_report = context->escaped.state == 2 && context->escaped.report.provider.retain != NULL;
        }
        if (marker == 456789123)
        {
            *(int32_t *)entry.storage = 0;

            run_outcome outcome = {0};

            entry.finalizer.start((uintptr_t)&context->error, &outcome);
            found_error = outcome.state == 0 && context->error.provider.retain != NULL;
        }
    }

    if (!found_report || !found_error)
        return 89;

    return context->control(PRODUCT_HOST_RELEASE_ENTRY).status == 0 ? 0 : 98;
}

#if defined(_WIN32)
static DWORD WINAPI producer_entry(LPVOID value)
{
    producer_context *context = (producer_context *)value;
    context->result = produce_escaped(context);
    return 0;
}
#else
static void *producer_entry(void *value)
{
    producer_context *context = (producer_context *)value;
    context->result = produce_escaped(context);
    return NULL;
}
#endif

typedef struct
{
    int argument_count;
    char **arguments;
    native_library first_library;
    native_library second_library;
    uintptr_t previous_load;
} argument_context;

static int run_host(void *raw_context)
{
    argument_context *context = (argument_context *)raw_context;
    int argument_count = context->argument_count;
    char **arguments = context->arguments;

    if (argument_count != 9)
        return 60;

    static_identity thread_identity;
    static_identity async_thread_identity;

    if (strlen(arguments[6]) != 64 || strlen(arguments[7]) != 64)
        return 76;

    for (size_t index = 0; index < 32; index += 1)
    {
        if (
            sscanf(arguments[6] + index * 2, "%2hhx", &thread_identity.bytes[index]) != 1 ||
            sscanf(arguments[7] + index * 2, "%2hhx", &async_thread_identity.bytes[index]) != 1
        )
            return 77;
    }

    for (int mode = 0; mode < 8; mode += 1)
    {
        int failure_result = check_failed_formation(arguments[1], arguments[3], arguments[4], arguments[8], mode);

        if (failure_result != 0)
            return failure_result;
    }

    native_library first_library = open_library(arguments[1]);
    native_library second_library = open_library(arguments[2]);

    if (first_library == NULL || second_library == NULL || first_library == second_library)
        return 61;

    product_host_control first_control =
        (product_host_control)find_symbol(first_library, arguments[3]);
    product_host_control second_control =
        (product_host_control)find_symbol(second_library, arguments[3]);
    product_host_descriptor *first_descriptor =
        (product_host_descriptor *)find_symbol(first_library, arguments[4]);
    product_host_descriptor *second_descriptor =
        (product_host_descriptor *)find_symbol(second_library, arguments[4]);

    if (
        first_control == NULL ||
        second_control == NULL ||
        first_descriptor == NULL ||
        second_descriptor == NULL ||
        first_descriptor == second_descriptor
    )
        return 62;

    if (
        find_symbol(first_library, arguments[5]) != NULL ||
        find_symbol(second_library, arguments[5]) != NULL
    )
        return 68;

    atomic_size_t first_references = 0;
    atomic_size_t second_references = 0;

    supply_binding(first_descriptor, &first_references);
    supply_binding(second_descriptor, &second_references);

    if (first_descriptor->binding.load_identity == second_descriptor->binding.load_identity || memcmp(first_descriptor->identity, second_descriptor->identity, 32) != 0)
        return 87;

    if (!observation_is(second_control(PRODUCT_HOST_FORM), 0, 1))
        return 63;

    if (!observation_is(first_control(PRODUCT_HOST_FORM), 0, 1))
        return 101;

    product_binding admitted = first_descriptor->binding;

    first_descriptor->binding.domain ^= 1;

    if (first_control(PRODUCT_HOST_FORM).status != 3 || second_control(PRODUCT_HOST_OBSERVE).status != 0)
        return 102;

    first_descriptor->binding = admitted;

    int32_t (*probe)(int32_t) = (int32_t (*)(int32_t))find_symbol(first_library, arguments[8]);

    /* Check the C ABI parameter and result through the formed product entry. */
    if (probe == NULL || probe(42) != 42 || probe(-17) != -17 || probe(1234567) != 1234567)
        return 103;

    int scoped_thread_result = check_product_scoped_thread_statics(
        first_control,
        first_descriptor,
        second_control,
        second_descriptor,
        thread_identity
    );

    if (scoped_thread_result != 0)
        return scoped_thread_result;

    if (!observation_is(first_control(PRODUCT_HOST_FORM), 0, 1))
        return 88;

    producer_context producer = { .descriptor = first_descriptor, .control = first_control };

#if defined(_WIN32)
    HANDLE producer_thread = CreateThread(NULL, 0, producer_entry, &producer, 0, NULL);

    if (producer_thread == NULL || WaitForSingleObject(producer_thread, INFINITE) != WAIT_OBJECT_0 || !CloseHandle(producer_thread))
        return 99;
#else
    pthread_t producer_thread;

    if (pthread_create(&producer_thread, NULL, producer_entry, &producer) != 0 || pthread_join(producer_thread, NULL) != 0)
        return 99;
#endif

    if (producer.result != 0 || atomic_load(&first_references) != 3)
        return 89;

    run_outcome escaped = producer.escaped;
    cleanup_incident escaped_error = producer.error;

    size_t product_static_count = 0;
    uintptr_t first_product_address = 0;

    int first_result = exercise_host(
        first_control,
        first_descriptor,
        &product_static_count,
        &first_product_address,
        thread_identity,
        async_thread_identity
    );

    if (first_result != 0)
        return first_result;

    product_host_observation second_open = second_control(PRODUCT_HOST_OBSERVE);

    if (
        !observation_is(second_open, 0, 1) ||
        second_open.initialized_statics != product_static_count
    )
        return 64;

    uintptr_t second_product_address = 0;

    for (size_t index = 0; index < second_descriptor->static_count; index += 1)
    {
        static_host_entry entry = second_descriptor->static_entry(index);

        if (entry.duration == 0)
        {
            second_product_address = entry.access();
            break;
        }
    }

    if (
        first_product_address == 0 ||
        second_product_address == 0 ||
        first_product_address == second_product_address
    )
        return 65;

    product_host_observation second_closed = second_control(PRODUCT_HOST_CLOSE);

    if (
        !observation_is(second_closed, 4, 3) ||
        second_closed.cleaned_statics != product_static_count ||
        second_closed.cleanup_incidents != 1
    )
        return 66;

    if (probe(42) != 0)
        return 104;

    if (retire_host(first_control).status != 1)
        return 90;

    char message[7];

    if (escaped.report.copy_message(escaped.report.message, 0, message, sizeof(message)) != 0 || memcmp(message, "escaped", sizeof(message)) != 0)
        return 91;

    run_outcome error_destruction = {0};
    run_outcome backing_release = {0};
    provider_reference error_provider = escaped_error.provider;

    escaped_error.destroy(escaped_error.payload, &error_destruction, &backing_release);
    error_provider.release(error_provider.context);

    if (error_destruction.state != 2 || backing_release.state != 0 || consume_report(&error_destruction.report) != 0 || consume_report(&escaped.report) != 0)
        return 92;

    if (atomic_load(&first_references) != 1 || retire_host(first_control).status != 0 || retire_host(second_control).status != 0 || atomic_load(&first_references) != 0 || atomic_load(&second_references) != 0)
        return 93;

    context->previous_load = first_descriptor->binding.load_identity;
    context->first_library = first_library;
    context->second_library = second_library;

    return 0;
}

typedef struct
{
    int (*work)(void *);
    void *context;
    int result;
} native_work;

#if defined(_WIN32)
static DWORD WINAPI native_work_entry(LPVOID value)
{
    native_work *work = (native_work *)value;
    work->result = work->work(work->context);
    return 0;
}
#else
static void *native_work_entry(void *value)
{
    native_work *work = (native_work *)value;
    work->result = work->work(work->context);
    return NULL;
}
#endif

static int run_on_thread(int (*callback)(void *), void *context)
{
    native_work work = { callback, context, 0 };

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, native_work_entry, &work, 0, NULL);

    if (thread == NULL || WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0 || !CloseHandle(thread))
        return 105;
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, native_work_entry, &work) != 0 || pthread_join(thread, NULL) != 0)
        return 105;
#endif

    return work.result;
}

static int close_reloaded(void *context)
{
    product_host_control control = *(product_host_control *)context;

    if (!observation_is(control(PRODUCT_HOST_FORM), 0, 1) || control(PRODUCT_HOST_CLOSE).state != 3 || retire_host(control).status != 0)
        return 96;

    return 0;
}

int main(int argument_count, char **arguments)
{
    argument_context context = { .argument_count = argument_count, .arguments = arguments };
    int result = run_on_thread(run_host, &context);

    if (result != 0)
        return result;

    // Every thread that entered image code has exited, including its native TLS destructors.
    if (close_library(context.first_library) != 0 || close_library(context.second_library) != 0)
        return 67;

    native_library reloaded = open_library(arguments[1]);
    product_host_descriptor *descriptor = (product_host_descriptor *)find_symbol(reloaded, arguments[4]);
    product_host_control control = (product_host_control)find_symbol(reloaded, arguments[3]);
    atomic_size_t references = 0;

    if (reloaded == NULL || descriptor == NULL || control == NULL || atomic_load(&descriptor->load) != 0)
        return 94;

    supply_binding(descriptor, &references);

    if (descriptor->binding.load_identity == context.previous_load)
        return 95;

    result = run_on_thread(close_reloaded, &control);

    if (result != 0)
        return result;

    if (atomic_load(&references) != 0 || close_library(reloaded) != 0)
        return 96;

    return 0;
}
