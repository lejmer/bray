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
typedef void (*static_cleanup)(void);

typedef struct {
    uint8_t bytes[32];
} static_identity;

typedef static_identity (*static_dependency)(size_t index);

typedef struct static_host_entry {
    uint32_t abi_version;
    uint32_t duration;
    static_identity identity;
    uint64_t order;
    uintptr_t storage;
    static_access access;
    static_cleanup cleanup;
    static_dependency dependency;
    size_t dependency_count;
} static_host_entry;

typedef static_host_entry (*static_host_lookup)(size_t index);

typedef struct {
    uint32_t abi_version;
    uint32_t reserved;
    uint8_t identity[32];
    static_host_lookup static_entry;
    size_t static_count;
} product_host_descriptor;

typedef struct {
    product_host_control control;
    static_access access;
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

static int observation_is(product_host_observation observation, uint32_t status, uint32_t state)
{
    return observation.status == status && observation.state == state;
}

static int run_thread_check(thread_context *context)
{
    product_host_observation attached = context->control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

    if (!observation_is(attached, 0, 1)) {
        return 1;
    }

    uintptr_t first = context->access();
    uintptr_t second = context->access();

    if (first == 0 || first != second || first == context->other_address) {
        return 2;
    }

    if (*(int32_t *)first != 42) {
        return 3;
    }

    product_host_observation detached = context->control(PRODUCT_HOST_DETACH_CURRENT_THREAD);

    if (!observation_is(detached, 0, 1)) {
        return 4;
    }

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

static int check_thread_static(product_host_control control, static_access access)
{
    product_host_observation attached = control(PRODUCT_HOST_ATTACH_CURRENT_THREAD);

    if (!observation_is(attached, 0, 1)) {
        return 20;
    }

    uintptr_t first = access();
    uintptr_t second = access();

    if (first == 0 || first != second || *(int32_t *)first != 42) {
        return 21;
    }

    *(int32_t *)first = 99;

    thread_context context = {control, access, first, 0};

#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, thread_entry, &context, 0, NULL);

    if (thread == NULL || WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0) {
        return 22;
    }

    CloseHandle(thread);
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, thread_entry, &context) != 0) {
        return 22;
    }

    if (pthread_join(thread, NULL) != 0) {
        return 23;
    }
#endif

    if (context.result != 0 || *(int32_t *)first != 99) {
        return 24 + context.result;
    }

    if (!observation_is(control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1)) {
        return 30;
    }

    if (!observation_is(control(PRODUCT_HOST_ATTACH_CURRENT_THREAD), 0, 1)) {
        return 31;
    }

    uintptr_t reattached = access();

    if (reattached == 0 || *(int32_t *)reattached != 42) {
        return 32;
    }

    if (!observation_is(control(PRODUCT_HOST_DETACH_CURRENT_THREAD), 0, 1)) {
        return 33;
    }

    return 0;
}

static int exercise_host(
    product_host_control control,
    const product_host_descriptor *descriptor,
    size_t *product_static_count
)
{
    product_host_observation formed = control(PRODUCT_HOST_FORM);

    if (!observation_is(formed, 0, 1) || descriptor->abi_version != 1) {
        return 40;
    }

    if (descriptor->static_count < 5 || descriptor->static_entry == NULL) {
        return 41;
    }

    static_access thread_access = NULL;
    uintptr_t product_addresses[64];
    size_t product_count = 0;

    if (descriptor->static_count > 64) {
        return 42;
    }

    for (size_t index = 0; index < descriptor->static_count; index += 1) {
        static_host_entry entry_value = descriptor->static_entry(index);
        const static_host_entry *entry = &entry_value;

        if (entry->abi_version != 1 || entry->order != index) {
            return 43;
        }

        if (entry->duration == 0) {
            uintptr_t first = entry->access();
            uintptr_t second = entry->access();

            if (first == 0 || first != second || first != entry->storage) {
                return 44;
            }

            for (size_t previous = 0; previous < product_count; previous += 1) {
                if (product_addresses[previous] == first) {
                    return 45;
                }
            }

            product_addresses[product_count] = first;
            product_count += 1;
        } else if (entry->duration == 1) {
            if (thread_access != NULL) {
                return 46;
            }

            thread_access = entry->access;
        } else {
            return 47;
        }
    }

    if (product_count < 4 || thread_access == NULL) {
        return 48;
    }

    int thread_result = check_thread_static(control, thread_access);

    if (thread_result != 0) {
        return thread_result;
    }

    if (!observation_is(control(PRODUCT_HOST_ACQUIRE_ENTRY), 0, 1)) {
        return 50;
    }

    if (!observation_is(control(PRODUCT_HOST_ACQUIRE_EXTERNAL), 0, 1)) {
        return 51;
    }

    if (!observation_is(control(PRODUCT_HOST_CLOSE), 1, 2)) {
        return 52;
    }

    if (control(PRODUCT_HOST_ACQUIRE_ENTRY).status != 2) {
        return 53;
    }

    if (!observation_is(control(PRODUCT_HOST_RELEASE_ENTRY), 1, 2)) {
        return 54;
    }

    product_host_observation closed = control(PRODUCT_HOST_RELEASE_EXTERNAL);

    if (!observation_is(closed, 2, 3)
        || closed.cleaned_statics != product_count
        || closed.cleanup_incidents != 0) {
        return 55;
    }

    *product_static_count = product_count;

    return 0;
}

int main(int argument_count, char **arguments)
{
    if (argument_count != 5) {
        return 60;
    }

    native_library first_library = open_library(arguments[1]);
    native_library second_library = open_library(arguments[2]);

    if (first_library == NULL || second_library == NULL || first_library == second_library) {
        return 61;
    }

    product_host_control first_control =
        (product_host_control)find_symbol(first_library, arguments[3]);
    product_host_control second_control =
        (product_host_control)find_symbol(second_library, arguments[3]);
    const product_host_descriptor *first_descriptor =
        (const product_host_descriptor *)find_symbol(first_library, arguments[4]);
    const product_host_descriptor *second_descriptor =
        (const product_host_descriptor *)find_symbol(second_library, arguments[4]);

    if (first_control == NULL || second_control == NULL
        || first_descriptor == NULL || second_descriptor == NULL
        || first_descriptor == second_descriptor) {
        return 62;
    }

    if (!observation_is(second_control(PRODUCT_HOST_FORM), 0, 1)) {
        return 63;
    }

    size_t product_static_count = 0;
    int first_result = exercise_host(first_control, first_descriptor, &product_static_count);

    if (first_result != 0) {
        return first_result;
    }

    product_host_observation second_open = second_control(PRODUCT_HOST_OBSERVE);

    if (!observation_is(second_open, 0, 1)
        || second_open.initialized_statics != product_static_count) {
        return 64;
    }

    product_host_observation second_closed = second_control(PRODUCT_HOST_CLOSE);

    if (!observation_is(second_closed, 2, 3)
        || second_closed.cleaned_statics != product_static_count) {
        return 65;
    }

    if (close_library(first_library) != 0 || close_library(second_library) != 0) {
        return 66;
    }

    return 0;
}
