#include <stdio.h>
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
typedef uintptr_t (*static_cleanup)(void);
typedef void (*static_transition)(void);
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
    StaticFinalizer finalizer;
    static_cleanup destroy;
    static_transition detach;
    static_dependency dependency;
    size_t dependency_count;
} static_host_entry;

typedef static_host_entry (*static_host_lookup)(size_t index);

typedef struct
{
    uint32_t abi_version;
    uint32_t reserved;
    uint8_t identity[32];
    static_host_lookup static_entry;
    size_t static_count;
} product_host_descriptor;

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

static int observation_is(ProductHostObservation observation, uint32_t status, uint32_t state)
{
    return observation.status == status && observation.state == state;
}

static int identity_is(static_identity left, static_identity right)
{
    return memcmp(left.bytes, right.bytes, sizeof(left.bytes)) == 0;
}

static static_access find_thread_access(
    const product_host_descriptor *descriptor,
    uint32_t finalizer_execution
)
{
    static_access result = NULL;

    for (size_t index = 0; index < descriptor->static_count; index += 1)
    {
        static_host_entry entry = descriptor->static_entry(index);

        if (entry.duration != 1 || entry.finalizer.execution != finalizer_execution)
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
    ProductHostObservation attached = context->control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

    if (!observation_is(attached, 0, 1))
        return 1;

    uintptr_t first = context->access();
    uintptr_t second = context->access();
    uintptr_t cleanup = context->async_access();

    if (first == 0 || first != second || first == context->other_address || cleanup == 0)
        return 2;

    if (*(int32_t *)first != 42 || *(int32_t *)cleanup != 192837465)
        return 3;

    ProductHostObservation detached = context->control(PRODUCT_HOST_DETACH_CURRENT_THREAD);

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
    ProductHostObservation attached = control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

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
    const product_host_descriptor *second_descriptor
)
{
    static_access first_access = find_thread_access(first_descriptor, 0);
    static_access second_access = find_thread_access(second_descriptor, 0);

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
    uintptr_t *representative_address
)
{
    ProductHostObservation formed = control(PRODUCT_HOST_FORM);

    if (!observation_is(formed, 0, 1) || descriptor->abi_version != 1)
        return 40;

    if (descriptor->static_count < 6 || descriptor->static_entry == NULL)
        return 41;

    static_access thread_access = find_thread_access(descriptor, 0);
    static_access async_thread_access = find_thread_access(descriptor, 2);
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

    ProductHostObservation closed = control(PRODUCT_HOST_RELEASE_EXTERNAL);

    if (
        !observation_is(closed, 4, 3) ||
        closed.cleaned_statics != product_count ||
        closed.cleanup_incidents != 1
    )
    {
        fprintf(stderr, "closure status=%u state=%u cleaned=%zu expected=%zu incidents=%zu\n",
            closed.status, closed.state, closed.cleaned_statics, product_count, closed.cleanup_incidents);
        return 55;
    }

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

    ProductHostObservation formed_again = control(PRODUCT_HOST_FORM);

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

int main(int argument_count, char **arguments)
{
    if (argument_count != 6)
        return 60;

    native_library first_library = open_library(arguments[1]);
    native_library second_library = open_library(arguments[2]);

    if (first_library == NULL || second_library == NULL || first_library == second_library)
        return 61;

    product_host_control first_control =
        (product_host_control)find_symbol(first_library, arguments[3]);
    product_host_control second_control =
        (product_host_control)find_symbol(second_library, arguments[3]);
    const product_host_descriptor *first_descriptor =
        (const product_host_descriptor *)find_symbol(first_library, arguments[4]);
    const product_host_descriptor *second_descriptor =
        (const product_host_descriptor *)find_symbol(second_library, arguments[4]);

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

    if (!observation_is(second_control(PRODUCT_HOST_FORM), 0, 1))
        return 63;

    int scoped_thread_result = check_product_scoped_thread_statics(
        first_control,
        first_descriptor,
        second_control,
        second_descriptor
    );

    if (scoped_thread_result != 0)
        return scoped_thread_result;

    size_t product_static_count = 0;
    uintptr_t first_product_address = 0;

    int first_result = exercise_host(
        first_control,
        first_descriptor,
        &product_static_count,
        &first_product_address
    );

    if (first_result != 0)
        return first_result;

    ProductHostObservation second_open = second_control(PRODUCT_HOST_OBSERVE);

    if (
        !observation_is(second_open, 0, 1) ||
        second_open.initialized_statics != product_static_count
    )
        return 64;

    uintptr_t second_product_address = 0;

    for (size_t index = 0; index < second_descriptor->static_count; index += 1)
    {
        static_host_entry entry = second_descriptor->static_entry(index);

        if (entry.duration == 0) {
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

    ProductHostObservation second_closed = second_control(PRODUCT_HOST_CLOSE);

    if (
        !observation_is(second_closed, 4, 3) ||
        second_closed.cleaned_statics != product_static_count ||
        second_closed.cleanup_incidents != 1
    )
        return 66;

    if (close_library(first_library) != 0 || close_library(second_library) != 0)
        return 67;

    return 0;
}
