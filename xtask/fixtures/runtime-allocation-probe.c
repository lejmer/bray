#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#if defined(__APPLE__)
#include <malloc/malloc.h>
#elif defined(__linux__) && defined(__GLIBC__)
// Bray's Linux native targets use glibc. Bypass our free symbol without a
// dlsym initialization path that could itself free memory and reenter us.
extern void __libc_free(void* memory);
#elif !defined(_WIN32)
#error Unsupported allocation probe host
#endif

static atomic_bool reject_allocations;
static atomic_uint allocation_attempts;
static atomic_uint live_allocations;
static atomic_size_t allocated_bytes;
static void (*allocation_callback)(void);
static void (*release_callback)(void);
static atomic_bool cleanup_observed;

typedef struct ReportAllocation
{
    struct ReportAllocation* next;
    void* memory;
} ReportAllocation;

static ReportAllocation* report_allocations;
static atomic_flag allocation_lock = ATOMIC_FLAG_INIT;

static void lock_allocations(void)
{
    while (atomic_flag_test_and_set_explicit(&allocation_lock, memory_order_acquire))
    {}
}

static void unlock_allocations(void)
{
    atomic_flag_clear_explicit(&allocation_lock, memory_order_release);
}

static void free_heap_memory(void* memory)
{
#ifdef _WIN32
    free(memory);
#elif defined(__APPLE__)
    // Calling the zone API bypasses the interposed free entry point on Darwin.
    malloc_zone_free(malloc_zone_from_ptr(memory), memory);
#else
    __libc_free(memory);
#endif
}

uint32_t bray_allocation_probe_begin(void)
{
    unsigned attempts = atomic_load(&allocation_attempts);
    atomic_store(&reject_allocations, 1);
    return attempts;
}

uint32_t bray_allocation_probe_end(uint32_t before)
{
    atomic_store(&reject_allocations, 0);
    return atomic_load(&allocation_attempts) - before;
}

// Observe destruction independently after the product has drained its report sink.
static void verify_cleanup_observed(void)
{
    if (!atomic_load(&cleanup_observed))
        abort();
}

void bray_allocation_probe_expect_cleanup(void)
{
    if (atexit(verify_cleanup_observed) != 0)
        abort();
}

void bray_allocation_probe_observe_cleanup(void)
{
    if (atomic_exchange(&cleanup_observed, 1))
        abort();
}

static void* allocate_report_memory(size_t bytes, size_t alignment)
{
    atomic_fetch_add(&allocation_attempts, 1);

    if (allocation_callback != NULL)
        allocation_callback();

    if (atomic_load(&reject_allocations))
        return NULL;

    if (alignment == 0 || (alignment & (alignment - 1)) != 0)
        abort();

    if (alignment - 1 > SIZE_MAX - sizeof(ReportAllocation) ||
        bytes > SIZE_MAX - sizeof(ReportAllocation) - (alignment - 1))
        return NULL;

    ReportAllocation* allocation = malloc(sizeof(*allocation) + alignment - 1 + bytes);

    if (allocation != NULL)
    {
        uintptr_t start = (uintptr_t)(allocation + 1);
        allocation->memory = (void*)((start + alignment - 1) & ~(uintptr_t)(alignment - 1));

        lock_allocations();
        allocation->next = report_allocations;
        report_allocations = allocation;
        atomic_fetch_add(&live_allocations, 1);
        atomic_fetch_add(&allocated_bytes, bytes);
        unlock_allocations();
    }

    return allocation == NULL ? NULL : allocation->memory;
}

static void release_report_memory(void* memory)
{
    if (memory == NULL)
        return;

    // Look up ownership instead of reading a header before an arbitrary libc
    // pointer. Remove it before callbacks, which may allocate or release again.
    lock_allocations();
    ReportAllocation** cursor = &report_allocations;

    while (*cursor != NULL && (*cursor)->memory != memory)
        cursor = &(*cursor)->next;

    ReportAllocation* allocation = *cursor;

    if (allocation != NULL)
    {
        *cursor = allocation->next;
        atomic_fetch_sub(&live_allocations, 1);
    }

    unlock_allocations();

    if (allocation != NULL)
    {
        if (release_callback != NULL)
            release_callback();

        free_heap_memory(allocation);
    }
    else
    {
        free_heap_memory(memory);
    }
}

#ifdef _WIN32
void* _aligned_malloc(size_t bytes, size_t alignment)
{
    return allocate_report_memory(bytes, alignment);
}

void _aligned_free(void* memory)
{
    release_report_memory(memory);
}
#else
void* aligned_alloc(size_t alignment, size_t bytes)
{
    return allocate_report_memory(bytes, alignment);
}

void free(void* memory)
{
    release_report_memory(memory);
}
#endif
