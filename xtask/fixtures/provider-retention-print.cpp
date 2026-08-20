#include "bray_standard_stream.h"

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <memory>
#include <mutex>
#include <thread>

bool write_owned_block(std::uint8_t byte)
{
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

    return successful;
}

int main()
{
    const auto completed = std::make_shared<std::atomic_bool>(false);

    std::thread(
        [completed]
        {
            std::this_thread::sleep_for(std::chrono::seconds(5));

            if (!completed->load())
                std::_Exit(10);
        }
    ).detach();

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

    std::mutex coordination_mutex;
    std::condition_variable coordination_changed;
    bool first_acquired = false;
    bool second_attempting = false;
    bool second_acquired = false;
    bool first_result = false;
    bool second_result = false;
    bool acquired_while_owned = false;

    std::thread first(
        [&]
        {
            if (bray_platform_standard_output_lock().category != BRAY_PLATFORM_SUCCESS)
                return;

            {
                std::lock_guard coordination(coordination_mutex);
                first_acquired = true;
            }

            coordination_changed.notify_all();

            {
                std::unique_lock coordination(coordination_mutex);

                coordination_changed.wait(
                    coordination,
                    [&second_attempting] { return second_attempting; }
                );

                acquired_while_owned = coordination_changed.wait_for(
                    coordination,
                    std::chrono::milliseconds(250),
                    [&second_acquired] { return second_acquired; }
                );
            }

            first_result = write_owned_block('a');

            if (bray_platform_standard_output_unlock().category != BRAY_PLATFORM_SUCCESS)
                first_result = false;
        }
    );

    std::thread second(
        [&]
        {
            {
                std::unique_lock coordination(coordination_mutex);

                coordination_changed.wait(
                    coordination,
                    [&first_acquired] { return first_acquired; }
                );

                second_attempting = true;
            }

            coordination_changed.notify_all();

            const auto acquired = bray_platform_standard_output_lock();

            {
                std::lock_guard coordination(coordination_mutex);
                second_acquired = acquired.category == BRAY_PLATFORM_SUCCESS;
            }

            coordination_changed.notify_all();

            if (!second_acquired)
                return;

            second_result = write_owned_block('b');

            if (bray_platform_standard_output_unlock().category != BRAY_PLATFORM_SUCCESS)
                second_result = false;
        }
    );

    first.join();
    second.join();

    completed->store(true);

    return first_result && second_result && !acquired_while_owned ? 0 : 9;
}
