#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

static atomic_bool reject_allocations;
static atomic_uint allocation_attempts;
static atomic_uint live_allocations;
static atomic_size_t allocated_bytes;
static void (*allocation_callback)(void);
static void (*release_callback)(void);

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

static void* allocate_report_memory(size_t bytes, size_t alignment)
{
    atomic_fetch_add(&allocation_attempts, 1);

    if (allocation_callback != NULL)
        allocation_callback();

    if (atomic_load(&reject_allocations))
        return NULL;

    if (alignment > _Alignof(max_align_t))
        abort();

    void* allocation = malloc(bytes);

    if (allocation != NULL)
    {
        atomic_fetch_add(&live_allocations, 1);
        atomic_fetch_add(&allocated_bytes, bytes);
    }

    return allocation;
}

#ifdef _WIN32
void* _aligned_malloc(size_t bytes, size_t alignment)
{
    return allocate_report_memory(bytes, alignment);
}

void _aligned_free(void* memory)
{
    if (memory != NULL)
    {
        atomic_fetch_sub(&live_allocations, 1);

        if (release_callback != NULL)
            release_callback();
    }

    free(memory);
}
#else
void* aligned_alloc(size_t alignment, size_t bytes)
{
    return allocate_report_memory(bytes, alignment);
}
#endif
