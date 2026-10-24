#include <assert.h>
#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

static atomic_uint allocation_callbacks;
static atomic_uint release_callbacks;
static atomic_uint successful_allocations;
static atomic_bool nest_allocation;
static atomic_bool nest_release;
static void* nested_memory;

// Volatile calls keep the compiler from eliding malloc/free pairs or treating
// the overridden allocation functions as their builtin implementations.
#ifdef _WIN32
static void* (*volatile observed_allocate)(size_t, size_t) = _aligned_malloc;
static void (*volatile observed_release)(void*) = _aligned_free;
#else
static void* (*volatile observed_allocate)(size_t, size_t) = aligned_alloc;
static void (*volatile observed_release)(void*) = free;
#endif
static void* (*volatile libc_allocate)(size_t) = malloc;
static void* (*volatile libc_resize)(void*, size_t) = realloc;
static void (*volatile libc_release)(void*) = free;

static void* allocate_observed(size_t bytes, size_t alignment)
{
#ifdef _WIN32
    void* memory = observed_allocate(bytes, alignment);
#else
    void* memory = observed_allocate(alignment, bytes);
#endif

    if (memory != NULL)
    {
        assert((uintptr_t)memory % alignment == 0);
        memset(memory, 0xa5, bytes);
        atomic_fetch_add(&successful_allocations, 1);
    }

    return memory;
}

static void on_allocation(void)
{
    atomic_fetch_add(&allocation_callbacks, 1);

    if (atomic_exchange(&nest_allocation, 0))
    {
        observed_release(nested_memory);
        void* memory = allocate_observed(64, 64);
        assert(memory != NULL);
        observed_release(memory);
    }
}

static void on_release(void)
{
    atomic_fetch_add(&release_callbacks, 1);

    if (atomic_exchange(&nest_release, 0))
    {
        observed_release(nested_memory);
        void* memory = allocate_observed(4096, 4096);
        assert(memory != NULL);
        observed_release(memory);
    }
}

#ifdef _WIN32
static DWORD WINAPI concurrent_allocations(void* input)
#else
static void* concurrent_allocations(void* input)
#endif
{
    // The first release crosses thread ownership. The loop overlaps insertion
    // and removal of differently aligned blocks on all four threads.
    observed_release(input);

    for (unsigned index = 0; index < 256; index++)
    {
        void* first = allocate_observed(64, 16);
        void* second = allocate_observed(4096, 4096);
        assert(first != NULL && second != NULL);
        observed_release(first);
        observed_release(second);
    }

    return 0;
}

int main(void)
{
    allocation_callback = on_allocation;
    release_callback = on_release;

    void* first = allocate_observed(64, 16);
    void* second = allocate_observed(4096, 4096);
    assert(first != NULL && second != NULL);
    assert(atomic_load(&live_allocations) == 2);
    assert(atomic_load(&allocated_bytes) == 64 + 4096);

    void* unrelated = libc_allocate(1);
    assert(unrelated != NULL);
    unrelated = libc_resize(unrelated, 128);
    assert(unrelated != NULL);
    libc_release(unrelated);
    libc_release(NULL);
    observed_release(NULL);

    assert(atomic_load(&live_allocations) == 2);
    assert(atomic_load(&release_callbacks) == 0);

    observed_release(first);
    assert(atomic_load(&live_allocations) == 1);
    assert(atomic_load(&release_callbacks) == 1);
    observed_release(second);
    assert(atomic_load(&live_allocations) == 0);
    assert(atomic_load(&release_callbacks) == 2);

    unsigned before = bray_allocation_probe_begin();
    assert(allocate_observed(64, 16) == NULL);
    assert(bray_allocation_probe_end(before) == 1);
    assert(atomic_load(&live_allocations) == 0);
    assert(allocate_observed(SIZE_MAX - 4095, 4096) == NULL);

    nested_memory = allocate_observed(64, 16);
    assert(nested_memory != NULL);
    atomic_store(&nest_allocation, 1);
    first = allocate_observed(64, 16);
    assert(first != NULL);
    observed_release(first);
    assert(atomic_load(&live_allocations) == 0);

    nested_memory = allocate_observed(64, 16);
    first = allocate_observed(64, 16);
    assert(nested_memory != NULL && first != NULL);
    atomic_store(&nest_release, 1);
    observed_release(first);
    assert(atomic_load(&live_allocations) == 0);

#ifdef _WIN32
    HANDLE threads[4];
#else
    pthread_t threads[4];
#endif

    for (unsigned index = 0; index < 4; index++)
    {
        void* memory = allocate_observed(64, 16);
        assert(memory != NULL);
#ifdef _WIN32
        threads[index] = CreateThread(NULL, 0, concurrent_allocations, memory, 0, NULL);
        assert(threads[index] != NULL);
#else
        assert(pthread_create(&threads[index], NULL, concurrent_allocations, memory) == 0);
#endif
    }

    for (unsigned index = 0; index < 4; index++)
    {
#ifdef _WIN32
        assert(WaitForSingleObject(threads[index], INFINITE) == WAIT_OBJECT_0);
        assert(CloseHandle(threads[index]));
#else
        assert(pthread_join(threads[index], NULL) == 0);
#endif
    }

    assert(atomic_load(&live_allocations) == 0);
    assert(report_allocations == NULL);
    assert(atomic_load(&release_callbacks) == atomic_load(&successful_allocations));
    assert(atomic_load(&allocation_callbacks) == atomic_load(&allocation_attempts));
    assert(atomic_load(&allocation_attempts) == atomic_load(&successful_allocations) + 2);
    puts("allocation probe: matched, untracked, reentrant, concurrent, refused, aligned");
    return 0;
}
