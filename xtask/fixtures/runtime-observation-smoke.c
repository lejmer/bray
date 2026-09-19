#include <stddef.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <threads.h>

#define CONCURRENT_THREAD_COUNT 32
#define CONCURRENT_RECORDS_PER_THREAD 256

extern void bray_runtime_memory_observation_begin(void);
extern void bray_runtime_memory_allocation_observation(size_t bytes);
extern void bray_runtime_memory_copy_observation(size_t bytes);
extern void bray_runtime_performance_interval_begin(void);
extern void bray_runtime_performance_interval_end(void);

struct concurrent_observation_state
{
    atomic_uint *ready;
    atomic_bool *start;
    size_t value;
};

static int observe_concurrent_allocations(void *context)
{
    struct concurrent_observation_state *state = context;
    size_t record = 0;

    atomic_fetch_add_explicit(state->ready, 1, memory_order_release);

    while (!atomic_load_explicit(state->start, memory_order_acquire))
    {
        thrd_yield();
    }

    while (record < CONCURRENT_RECORDS_PER_THREAD)
    {
        thrd_yield();
        bray_runtime_memory_allocation_observation(state->value);
        ++record;
    }

    return 0;
}

int main(int argc, char **argv)
{
    if (argc != 2)
    {
        return 2;
    }

    if (strcmp(argv[1], "empty") == 0)
    {
        bray_runtime_memory_observation_begin();
        return 0;
    }

    if (strcmp(argv[1], "records") == 0)
    {
        bray_runtime_memory_allocation_observation(3);
        bray_runtime_memory_copy_observation(5);
        bray_runtime_performance_interval_begin();
        bray_runtime_performance_interval_end();

        return 0;
    }

    if (strcmp(argv[1], "concurrent-first-hooks") == 0)
    {
        atomic_uint ready = 0;
        atomic_bool start = false;
        thrd_t threads[CONCURRENT_THREAD_COUNT];
        struct concurrent_observation_state states[CONCURRENT_THREAD_COUNT];
        size_t index = 0;

        while (index < CONCURRENT_THREAD_COUNT)
        {
            states[index].ready = &ready;
            states[index].start = &start;
            states[index].value = index + 1;

            if (thrd_create(&threads[index], observe_concurrent_allocations, &states[index]) != thrd_success)
            {
                return 3;
            }

            ++index;
        }

        while (atomic_load_explicit(&ready, memory_order_acquire) != CONCURRENT_THREAD_COUNT)
        {
            thrd_yield();
        }

        atomic_store_explicit(&start, true, memory_order_release);
        index = 0;

        while (index < CONCURRENT_THREAD_COUNT)
        {
            int result = 0;

            if (thrd_join(threads[index], &result) != thrd_success || result != 0)
            {
                return 4;
            }

            ++index;
        }

        return 0;
    }

    if (strcmp(argv[1], "invalid-interval") == 0)
    {
        bray_runtime_memory_observation_begin();
        bray_runtime_performance_interval_end();

        return 0;
    }

    if (strcmp(argv[1], "repeated-begin") == 0)
    {
        bray_runtime_memory_observation_begin();
        bray_runtime_performance_interval_begin();
        bray_runtime_performance_interval_begin();

        return 0;
    }

    if (strcmp(argv[1], "repeated-interval") == 0)
    {
        bray_runtime_performance_interval_begin();
        bray_runtime_performance_interval_end();
        bray_runtime_performance_interval_begin();

        return 0;
    }

    if (strcmp(argv[1], "record-cap") == 0)
    {
        uint64_t record = 0;

        bray_runtime_memory_observation_begin();

        while (record <= UINT64_C(1000000))
        {
            bray_runtime_memory_allocation_observation(1);
            ++record;
        }

        return 0;
    }

    return 2;
}
