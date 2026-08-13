#ifndef BRAY_STANDARD_STREAM_LOCK_H
#define BRAY_STANDARD_STREAM_LOCK_H

#include "status.h"

#if defined(_WIN32)

struct BrayStandardStreamLock
{
    INIT_ONCE once;
    CRITICAL_SECTION section;
    DWORD owner;
};

#define BRAY_STANDARD_STREAM_LOCK_INITIALIZER {INIT_ONCE_STATIC_INIT, {}, 0}

inline BOOL CALLBACK bray_initialize_standard_stream_lock(PINIT_ONCE, PVOID parameter, PVOID*)
{
    auto* lock = static_cast<BrayStandardStreamLock*>(parameter);
    return InitializeCriticalSectionEx(&lock->section, 0, 0);
}

inline bool bray_ensure_standard_stream_lock_initialized(BrayStandardStreamLock* lock)
{
    return InitOnceExecuteOnce(
        &lock->once,
        bray_initialize_standard_stream_lock,
        lock,
        nullptr
    ) != FALSE;
}

inline BrayPlatformStatus bray_lock_standard_stream(BrayStandardStreamLock* lock)
{
    const auto thread = GetCurrentThreadId();

    if (!bray_ensure_standard_stream_lock_initialized(lock))
        return {BRAY_PLATFORM_OTHER, 0, static_cast<std::int64_t>(GetLastError())};

    EnterCriticalSection(&lock->section);

    if (lock->owner == thread)
    {
        LeaveCriticalSection(&lock->section);
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};
    }

    lock->owner = thread;

    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

inline BrayPlatformStatus bray_unlock_standard_stream(BrayStandardStreamLock* lock)
{
    const auto thread = GetCurrentThreadId();

    if (!bray_ensure_standard_stream_lock_initialized(lock))
        return {BRAY_PLATFORM_OTHER, 0, static_cast<std::int64_t>(GetLastError())};

    if (!TryEnterCriticalSection(&lock->section))
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    const bool owner = lock->owner == thread;

    if (owner)
        lock->owner = 0;

    LeaveCriticalSection(&lock->section);

    if (!owner)
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    LeaveCriticalSection(&lock->section);

    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

#else

#include <atomic>
#include <pthread.h>

struct BrayStandardStreamLock
{
    pthread_mutex_t mutex;
    std::atomic_flag owner_guard;
    pthread_t owner;
    bool owned;
};

#define BRAY_STANDARD_STREAM_LOCK_INITIALIZER \
    {PTHREAD_MUTEX_INITIALIZER, ATOMIC_FLAG_INIT, {}, false}

inline void bray_lock_standard_stream_owner(BrayStandardStreamLock* lock)
{
    while (lock->owner_guard.test_and_set(std::memory_order_acquire))
    {
    }
}

inline void bray_unlock_standard_stream_owner(BrayStandardStreamLock* lock)
{
    lock->owner_guard.clear(std::memory_order_release);
}

inline BrayPlatformStatus bray_lock_standard_stream(BrayStandardStreamLock* lock)
{
    const auto thread = pthread_self();

    bray_lock_standard_stream_owner(lock);

    const bool reentrant = lock->owned && pthread_equal(lock->owner, thread) != 0;

    bray_unlock_standard_stream_owner(lock);

    if (reentrant)
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    const auto result = pthread_mutex_lock(&lock->mutex);

    if (result != 0)
        return {BRAY_PLATFORM_OTHER, 0, result};

    bray_lock_standard_stream_owner(lock);

    lock->owner = thread;
    lock->owned = true;

    bray_unlock_standard_stream_owner(lock);

    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

inline BrayPlatformStatus bray_unlock_standard_stream(BrayStandardStreamLock* lock)
{
    const auto thread = pthread_self();

    bray_lock_standard_stream_owner(lock);

    const bool owner = lock->owned && pthread_equal(lock->owner, thread) != 0;

    if (owner)
        lock->owned = false;

    bray_unlock_standard_stream_owner(lock);

    if (!owner)
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    const auto result = pthread_mutex_unlock(&lock->mutex);

    return result == 0
        ? BrayPlatformStatus {BRAY_PLATFORM_SUCCESS, 0, 0}
        : BrayPlatformStatus {BRAY_PLATFORM_OTHER, 0, result};
}

#endif

#endif
