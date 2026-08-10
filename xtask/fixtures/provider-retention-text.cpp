#include "bray_temporal.h"

#include <cstdint>

int main()
{
    constexpr std::uint8_t source[] = {
        '2', '0', '2', '4', '-', '0', '1', '-', '3', '1'
    };

    BrayTemporalValue value{};
    std::uint64_t invalid_offset = 0;

    const auto parsed = bray_temporal_parse(
        0,
        source,
        sizeof(source),
        &value,
        &invalid_offset
    );

    if (parsed != 0)
        return static_cast<int>(parsed);

    std::uint8_t destination[32]{};
    std::uint64_t written = 0;

    return static_cast<int>(bray_temporal_format(
        0,
        value,
        destination,
        sizeof(destination),
        &written
    ));
}
