#include "bray_temporal.h"

int main()
{
    BrayTemporalDateTime result{};

    return static_cast<int>(bray_temporal_date_add(
        {2024, 1, 31, 0, 0, 0, 0},
        0,
        1,
        0,
        1,
        &result
    ));
}
