#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdarg.h>
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

BRAY_EXPORT int32_t bray_foreign_counter = 40;

#if !defined(BRAY_SHARED_FIXTURE)
extern int32_t bray_exported_value;

BRAY_EXPORT int32_t bray_foreign_read_exported_value(void)
{
    return bray_exported_value;
}
#endif

BRAY_EXPORT int32_t bray_foreign_read_counter(const int32_t* value)
{
    if (value != &bray_foreign_counter)
        return -1;

    return *value;
}

BRAY_EXPORT int32_t bray_foreign_sum_variadic(int32_t count, ...)
{
    int32_t result = 0;
    va_list arguments;

    va_start(arguments, count);

    for (int32_t index = 0; index < count; index += 1)
        result += va_arg(arguments, int32_t);

    va_end(arguments);

    return result;
}

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

typedef struct { int8_t narrow; uint8_t unsigned_narrow; int32_t signed_value; } BraySmallRecord;
typedef struct { float first; float second; } BrayFloatPair;
typedef struct { int32_t integer; double real; } BrayMixedRecord;
typedef struct { double real; int32_t integer; } BrayReversedRecord;
typedef struct { double first; double second; } BrayDoublePair;
typedef struct { uint64_t first; uint64_t second; uint64_t third; } BrayLargeRecord;
typedef struct { uint8_t first; uint8_t second; uint8_t third; } BrayByteTriple;
typedef struct { BraySmallRecord inner; uint64_t wide; } BrayNestedRecord;

BRAY_EXPORT BrayByteTriple bray_foreign_byte_triple(BrayByteTriple value)
{
    if (value.first != 5 || value.second != 7 || value.third != 9)
        return (BrayByteTriple){0, 0, 0};

    return (BrayByteTriple){6, 9, 12};
}

BRAY_EXPORT BraySmallRecord bray_foreign_small_record(BraySmallRecord value)
{
    if (value.narrow != 5 || value.unsigned_narrow != 7 || value.signed_value != 41)
        return (BraySmallRecord){-100, 0, -100};

    return (BraySmallRecord){4, 8, 43};
}

BRAY_EXPORT BrayNestedRecord bray_foreign_nested_record(BrayNestedRecord value)
{
    BraySmallRecord inner = bray_foreign_small_record(value.inner);

    return (BrayNestedRecord){inner, value.wide + 3};
}

BRAY_EXPORT BrayFloatPair bray_foreign_float_pair(BrayFloatPair value)
{
    if (value.first != 1.25f || value.second != 2.5f)
        return (BrayFloatPair){-1, -1};

    return (BrayFloatPair){2.5f, 5.0f};
}

BRAY_EXPORT BrayMixedRecord bray_foreign_mixed_record(BrayMixedRecord value)
{
    if (value.integer != 41 || value.real != 2.5)
        return (BrayMixedRecord){-1, -1};

    return (BrayMixedRecord){43, 3.0};
}

BRAY_EXPORT BrayReversedRecord bray_foreign_reversed_record(BrayReversedRecord value)
{
    if (value.integer != 41 || value.real != 2.5)
        return (BrayReversedRecord){-1, -1};

    return (BrayReversedRecord){3.0, 43};
}

BRAY_EXPORT BrayLargeRecord bray_foreign_large_record(BrayLargeRecord value)
{
    if (value.first != 1 || value.second != 2 || value.third != 3)
        return (BrayLargeRecord){0, 0, 0};

    return (BrayLargeRecord){4, 5, 6};
}

BRAY_EXPORT BrayForeignRecord bray_foreign_combined_record(
    BrayForeignRecord first, BrayLargeRecord second, uint64_t tail)
{
    if (first.narrow != 5 || first.unsigned_narrow != 7 || first.signed_value != 41 || first.wide != 100 ||
        second.first != 4 || second.second != 5 || second.third != 6 || tail != 7)
        return (BrayForeignRecord){0, 0, 0, 0};

    first.wide += second.first + second.second + second.third + tail;
    return first;
}

BRAY_EXPORT int32_t bray_foreign_integer_pressure(
    uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t e,
    BrayForeignRecord value, uint64_t tail)
{
    return a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && tail == 6 &&
        bray_foreign_validate_record(value);
}

BRAY_EXPORT int32_t bray_foreign_mixed_pressure(
    uint64_t a, uint64_t b, uint64_t c, uint64_t d, uint64_t e, uint64_t f,
    BrayMixedRecord value, double tail)
{
    return a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6 && tail == 8.0 &&
        value.integer == 41 && value.real == 2.5;
}

BRAY_EXPORT int32_t bray_foreign_vector_pressure(
    double a, double b, double c, double d, double e, double f, double g,
    BrayDoublePair value, double tail)
{
    return a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6 && g == 7 &&
        tail == 8 && value.first == 10 && value.second == 20;
}

BRAY_EXPORT BrayLargeRecord bray_foreign_result_pressure(
    uint64_t a, uint64_t b, uint64_t c, uint64_t d, BrayForeignRecord value, uint64_t tail)
{
    if (a != 1 || b != 2 || c != 3 || d != 4 || !bray_foreign_validate_record(value))
        return (BrayLargeRecord){0, 0, 0};

    return (BrayLargeRecord){value.wide, tail, a + b + c + d};
}

BRAY_EXPORT int32_t bray_foreign_record_callback(BrayForeignRecord (*callback)(BrayForeignRecord))
{
    BrayForeignRecord result = callback((BrayForeignRecord){5, 7, 41, 100});

    return result.narrow == 6 && result.unsigned_narrow == 8 && result.signed_value == 42 && result.wide == 101;
}

BRAY_EXPORT int32_t bray_foreign_float_pair_callback(BrayFloatPair (*callback)(BrayFloatPair))
{
    BrayFloatPair result = callback((BrayFloatPair){1.25f, 2.5f});

    return result.first == 2.25f && result.second == 3.5f;
}

BRAY_EXPORT int32_t bray_foreign_mixed_record_callback(BrayMixedRecord (*callback)(BrayMixedRecord))
{
    BrayMixedRecord result = callback((BrayMixedRecord){41, 2.5});

    return result.integer == 42 && result.real == 3.5;
}

BRAY_EXPORT int32_t bray_foreign_large_record_callback(BrayLargeRecord (*callback)(BrayLargeRecord))
{
    BrayLargeRecord result = callback((BrayLargeRecord){1, 2, 3});

    return result.first == 2 && result.second == 3 && result.third == 4;
}

BRAY_EXPORT int32_t bray_foreign_result_pressure_callback(
    BrayLargeRecord (*callback)(uint64_t, uint64_t, uint64_t, uint64_t, BrayForeignRecord, uint64_t))
{
    BrayLargeRecord result = callback(1, 2, 3, 4, (BrayForeignRecord){5, 7, 41, 100}, 5);

    return result.first == 100 && result.second == 5 && result.third == 10;
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
