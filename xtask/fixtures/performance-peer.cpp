#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <vector>

#if defined(_WIN32) && BRAY_WORKLOAD == 5
#include <processthreadsapi.h>
#elif BRAY_WORKLOAD == 5
#include <unistd.h>
#endif

namespace {

constexpr char observation_header[] = "BRAYPO01";
constexpr std::uint8_t controlled_duration_record = 3;

template <typename Value>
void retain_work(Value const& value)
{
    __asm__ volatile("" : : "g"(&value) : "memory");
}

#if BRAY_WORKLOAD == 1
bool workload()
{
    return true;
}
#elif BRAY_WORKLOAD == 2
bool incremental_bytes(std::size_t count)
{
    std::vector<std::uint8_t> bytes;

    for (std::size_t index = 0; index < count; ++index)
        bytes.push_back(65);

    retain_work(bytes);
    return bytes.size() == count;
}

bool workload()
{
    return incremental_bytes(64);
}
#elif BRAY_WORKLOAD == 3
bool incremental_bytes(std::size_t count)
{
    std::vector<std::uint8_t> bytes;

    for (std::size_t index = 0; index < count; ++index)
        bytes.push_back(65);

    return bytes.size() == count;
}

bool workload()
{
    return incremental_bytes(4096);
}
#elif BRAY_WORKLOAD == 4
bool workload()
{
    std::error_code error;
    const auto path = std::filesystem::current_path(error);

    if (error)
        return false;

    for (std::size_t index = 0; index < 256; ++index)
    {
        const auto status = std::filesystem::status(path, error);

        if (error || status.type() == std::filesystem::file_type::not_found)
            return false;

        retain_work(status);
    }

    return true;
}
#elif BRAY_WORKLOAD == 5
bool workload()
{
    std::uint64_t combined = 0;

    for (std::size_t index = 0; index < 1024; ++index)
    {
#if defined(_WIN32)
        combined ^= static_cast<std::uint64_t>(GetCurrentProcessId());
#else
        combined ^= static_cast<std::uint64_t>(getpid());
#endif
    }

    retain_work(combined);
    return true;
}
#elif BRAY_WORKLOAD == 6
bool workload()
{
    auto last = std::chrono::steady_clock::now();

    for (std::size_t index = 0; index < 1024; ++index)
        last = std::chrono::steady_clock::now();

    retain_work(last);
    return true;
}
#else
#error "unsupported performance peer workload"
#endif

#if defined(BRAY_PEER_TIMING)
void write_duration(std::chrono::nanoseconds duration)
{
    const char* path = std::getenv("BRAY_PERFORMANCE_OBSERVATION_PATH");

    if (path == nullptr)
        std::abort();

    std::ofstream output(path, std::ios::binary | std::ios::trunc);

    if (!output)
        std::abort();

    output.write(observation_header, sizeof(observation_header) - 1);
    output.put(static_cast<char>(controlled_duration_record));

    const std::uint64_t nanoseconds = static_cast<std::uint64_t>(duration.count());

    for (std::size_t byte = 0; byte < sizeof(nanoseconds); ++byte)
        output.put(static_cast<char>((nanoseconds >> (byte * 8)) & 0xff));

    if (!output)
        std::abort();
}
#endif

}

int main()
{
#if defined(BRAY_PEER_TIMING)
    const auto started = std::chrono::steady_clock::now();
#endif

    const bool valid = workload();

#if defined(BRAY_PEER_TIMING)
    write_duration(std::chrono::duration_cast<std::chrono::nanoseconds>(
        std::chrono::steady_clock::now() - started));
#endif

    return valid ? 0 : 1;
}
