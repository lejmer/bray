#include "bray_standard_stream.h"

#include <thread>

bool write_block(std::uint8_t byte)
{
    const auto acquired = bray_platform_standard_output_lock();

    if (acquired.category != BRAY_PLATFORM_SUCCESS)
        return false;

    bool successful = true;

    for (std::uint64_t index = 0; index < 64; ++index)
    {
        std::uint64_t transferred = 0;
        const auto written = bray_platform_standard_output_write(&byte, 1, &transferred);

        if (written.category != BRAY_PLATFORM_SUCCESS || transferred != 1)
        {
            successful = false;
            break;
        }
    }

    if (bray_platform_standard_output_flush().category != BRAY_PLATFORM_SUCCESS)
        successful = false;

    if (bray_platform_standard_output_unlock().category != BRAY_PLATFORM_SUCCESS)
        successful = false;

    return successful;
}

int main()
{
    const auto unowned = bray_platform_standard_output_unlock();

    if (unowned.category != BRAY_PLATFORM_INVALID_INPUT)
        return 1;

    std::uint64_t transferred = 9;

    const auto acquired = bray_platform_standard_output_lock();

    if (acquired.category != BRAY_PLATFORM_SUCCESS)
        return 2;

    const auto reentrant = bray_platform_standard_output_lock();

    if (reentrant.category != BRAY_PLATFORM_INVALID_INPUT)
        return 3;

    const auto status = bray_platform_standard_output_write(nullptr, 0, &transferred);

    if (status.category != BRAY_PLATFORM_SUCCESS || transferred != 0)
        return 4;

    const auto failed_write = bray_platform_standard_output_write(nullptr, 1, &transferred);

    if (failed_write.category != BRAY_PLATFORM_INVALID_INPUT || transferred != 0)
        return 5;

    const auto released = bray_platform_standard_output_unlock();

    if (released.category != BRAY_PLATFORM_SUCCESS)
        return 6;

    if (bray_platform_standard_output_lock().category != BRAY_PLATFORM_SUCCESS)
        return 7;

    if (bray_platform_standard_output_unlock().category != BRAY_PLATFORM_SUCCESS)
        return 8;

    bool first_result = false;
    bool second_result = false;
    std::thread first([&first_result] { first_result = write_block('a'); });
    std::thread second([&second_result] { second_result = write_block('b'); });

    first.join();
    second.join();

    return first_result && second_result ? 0 : 9;
}
