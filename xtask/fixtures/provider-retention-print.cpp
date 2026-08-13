#include "bray_standard_stream.h"

int main()
{
    const auto unowned = bray_platform_standard_output_unlock();

    if (unowned.category != BRAY_PLATFORM_INVALID_INPUT)
    {
        return 1;
    }

    const auto acquired = bray_platform_standard_output_lock();

    if (acquired.category != BRAY_PLATFORM_SUCCESS)
    {
        return 2;
    }

    const auto reentrant = bray_platform_standard_output_lock();

    if (reentrant.category != BRAY_PLATFORM_INVALID_INPUT)
    {
        return 3;
    }

    const auto released = bray_platform_standard_output_unlock();

    if (released.category != BRAY_PLATFORM_SUCCESS)
    {
        return 4;
    }

    std::uint64_t transferred = 0;

    const auto status = bray_platform_standard_output_write(nullptr, 0, &transferred);

    return status.category == BRAY_PLATFORM_SUCCESS && transferred == 0 ? 0 : 5;
}
