#include "bray_temporal.h"

#include <cstdint>

int main()
{
    constexpr std::uint8_t name[] = {'U', 'T', 'C'};
    std::uint64_t handle = 0;

    const auto loaded = bray_temporal_zone_load(name, sizeof(name), &handle);

    if (loaded != 0)
        return static_cast<int>(loaded);

    return static_cast<int>(bray_temporal_zone_close(handle));
}
