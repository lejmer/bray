#include <stddef.h>
#include <stdint.h>
#include <string.h>

extern void bray_runtime_memory_observation_begin(void);
extern void bray_runtime_memory_allocation_observation(size_t bytes);
extern void bray_runtime_memory_copy_observation(size_t bytes);
extern void bray_runtime_performance_interval_begin(void);
extern void bray_runtime_performance_interval_end(void);

int main(int argc, char **argv)
{
    if (argc != 2)
        return 2;

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
