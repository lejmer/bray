#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <wchar.h>

#ifdef _WIN32
#include <windows.h>
#define BRAY_EXPORT __declspec(dllexport)
#else
#include <pthread.h>
#define BRAY_EXPORT __attribute__((visibility("default")))
#endif

typedef struct BrayForeignRecord
{
    int8_t narrow;
    uint8_t unsigned_narrow;
    int32_t signed_value;
    uint64_t wide;
} BrayForeignRecord;

typedef int32_t (*BrayForeignCallback)(void* context, int32_t value);
typedef int32_t (*BrayPlainCallback)(int32_t value);

typedef struct CallbackInvocation
{
    BrayForeignCallback callback;
    void* context;
    int32_t value;
    int32_t result;
} CallbackInvocation;

static _Atomic uint64_t next_resource = 1;
static _Atomic uint64_t live_resources = 0;

BRAY_EXPORT BrayForeignRecord bray_foreign_transform_record(BrayForeignRecord value)
{
    value.narrow -= 1;
    value.unsigned_narrow += 1;
    value.signed_value += 2;
    value.wide += 3;

    return value;
}

BRAY_EXPORT int32_t bray_foreign_validate_record(BrayForeignRecord value)
{
    return (
        value.narrow == 5 &&
        value.unsigned_narrow == 7 &&
        value.signed_value == 41 &&
        value.wide == 100
    );
}

BRAY_EXPORT uint64_t bray_foreign_record_size(void)
{
    return sizeof(BrayForeignRecord);
}

BRAY_EXPORT uint64_t bray_foreign_record_alignment(void)
{
    return _Alignof(BrayForeignRecord);
}

BRAY_EXPORT uint64_t bray_foreign_narrow_length(const char* value)
{
    uint64_t length = 0;

    while (value[length] != '\0')
        length += 1;

    return length;
}

BRAY_EXPORT uint64_t bray_foreign_wide_length(const wchar_t* value)
{
    uint64_t length = 0;

    while (value[length] != L'\0')
        length += 1;

    return length;
}

BRAY_EXPORT uint64_t bray_foreign_resource_acquire(bool succeed)
{
    if (!succeed)
        return 0;

    atomic_fetch_add_explicit(&live_resources, 1, memory_order_relaxed);

    return atomic_fetch_add_explicit(&next_resource, 1, memory_order_relaxed);
}

BRAY_EXPORT uint64_t bray_foreign_resource_duplicate(uint64_t handle)
{
    if (handle == 0)
        return 0;

    atomic_fetch_add_explicit(&live_resources, 1, memory_order_relaxed);

    return atomic_fetch_add_explicit(&next_resource, 1, memory_order_relaxed);
}

BRAY_EXPORT int32_t bray_foreign_resource_release(uint64_t handle, bool fail)
{
    if (handle == 0)
        return 6;

    if (fail)
        return 13;

    uint64_t current = atomic_load_explicit(&live_resources, memory_order_relaxed);

    while (
        current != 0 &&
        !atomic_compare_exchange_weak_explicit(
            &live_resources,
            &current,
            current - 1,
            memory_order_relaxed,
            memory_order_relaxed
        )
    )
    {
    }

    return current == 0 ? 6 : 0;
}

BRAY_EXPORT uint64_t bray_foreign_resource_live_count(void)
{
    return atomic_load_explicit(&live_resources, memory_order_relaxed);
}

BRAY_EXPORT int32_t bray_foreign_invoke(
    BrayForeignCallback callback,
    void* context,
    int32_t value
)
{
    if (callback == NULL)
        return -100;

    return callback(context, value);
}

BRAY_EXPORT int32_t bray_foreign_invoke_reentrant(
    BrayForeignCallback callback,
    void* context,
    int32_t value
)
{
    if (callback == NULL)
        return -100;

    return callback(context, value);
}

BRAY_EXPORT int32_t bray_foreign_invoke_plain(BrayPlainCallback callback, int32_t value)
{
    if (callback == NULL)
        return -100;

    return callback(value);
}

#ifdef _WIN32
static DWORD WINAPI invoke_callback_thread(LPVOID raw_invocation)
{
    CallbackInvocation* invocation = raw_invocation;

    invocation->result = invocation->callback(invocation->context, invocation->value);

    return 0;
}
#else
static void* invoke_callback_thread(void* raw_invocation)
{
    CallbackInvocation* invocation = raw_invocation;

    invocation->result = invocation->callback(invocation->context, invocation->value);

    return NULL;
}
#endif

BRAY_EXPORT int32_t bray_foreign_invoke_on_thread(
    BrayForeignCallback callback,
    void* context,
    int32_t value
)
{
    CallbackInvocation invocation = {callback, context, value, -100};

#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, invoke_callback_thread, &invocation, 0, NULL);

    if (thread == NULL)
        return -101;

    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
#else
    pthread_t thread;

    if (pthread_create(&thread, NULL, invoke_callback_thread, &invocation) != 0)
        return -101;

    if (pthread_join(thread, NULL) != 0)
        return -102;
#endif

    return invocation.result;
}

BRAY_EXPORT int32_t bray_foreign_invoke_concurrently(
    BrayForeignCallback callback,
    void* context,
    int32_t first_value,
    int32_t second_value
)
{
    CallbackInvocation first = {callback, context, first_value, -100};
    CallbackInvocation second = {callback, context, second_value, -100};

#ifdef _WIN32
    HANDLE first_thread = CreateThread(NULL, 0, invoke_callback_thread, &first, 0, NULL);

    if (first_thread == NULL)
        return -101;

    HANDLE second_thread = CreateThread(NULL, 0, invoke_callback_thread, &second, 0, NULL);

    if (second_thread == NULL)
    {
        WaitForSingleObject(first_thread, INFINITE);
        CloseHandle(first_thread);

        return -101;
    }

    WaitForSingleObject(first_thread, INFINITE);
    WaitForSingleObject(second_thread, INFINITE);
    CloseHandle(first_thread);
    CloseHandle(second_thread);
#else
    pthread_t first_thread;
    pthread_t second_thread;

    if (pthread_create(&first_thread, NULL, invoke_callback_thread, &first) != 0)
        return -101;

    if (pthread_create(&second_thread, NULL, invoke_callback_thread, &second) != 0)
    {
        pthread_join(first_thread, NULL);

        return -101;
    }

    const int first_join = pthread_join(first_thread, NULL);
    const int second_join = pthread_join(second_thread, NULL);

    if (first_join != 0 || second_join != 0)
        return -102;
#endif

    return first.result + second.result;
}
