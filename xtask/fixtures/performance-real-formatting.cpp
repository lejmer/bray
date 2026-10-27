#if BRAY_WORKLOAD >= 21 && BRAY_WORKLOAD <= 25
#include <charconv>
#include <system_error>
#if BRAY_WORKLOAD == 24
#include <vector>
#endif

namespace
{
#if BRAY_WORKLOAD == 21 || BRAY_WORKLOAD == 22 || BRAY_WORKLOAD == 23
bool workload()
{
    std::size_t total = 0;

    for (std::uint32_t index = 0; index < 1024; ++index)
    {
        char bytes[128];
#if BRAY_WORKLOAD == 21
        const double value = static_cast<double>(index) + 0.125;
        const auto result = std::to_chars(bytes, bytes + sizeof(bytes), value);
#elif BRAY_WORKLOAD == 22
        const double value = 0.1;
        const int precision = index % 2 == 0 ? 40 : 80;
        const auto result = std::to_chars(bytes, bytes + sizeof(bytes), value,
            std::chars_format::fixed, precision);
#else
        const double value = 1.5;
        const auto result = std::to_chars(bytes, bytes + sizeof(bytes), value,
            std::chars_format::fixed, 3);
#endif
        if (result.ec != std::errc())
            return false;

        const std::size_t length = static_cast<std::size_t>(result.ptr - bytes);
        retain_work(bytes);
#if BRAY_WORKLOAD == 23
        // Count the repeated padding range in one destination operation.
        total += length < 1000000 ? 1000000 : length;
#else
        total += length;
#endif
    }

#if BRAY_WORKLOAD == 21
    return total == 7082;
#elif BRAY_WORKLOAD == 22
    return total == 63488;
#else
    return total == 1024000000;
#endif
}
#elif BRAY_WORKLOAD == 24
bool workload()
{
    std::vector<char> sink;
    sink.reserve(7082);

    for (std::uint32_t index = 0; index < 1024; ++index)
    {
        char bytes[32];
        const double value = static_cast<double>(index) + 0.125;
        const auto result = std::to_chars(bytes, bytes + sizeof(bytes), value);

        if (result.ec != std::errc())
            return false;

        sink.insert(sink.end(), bytes, result.ptr);
    }

    retain_work(sink.data());

    return sink.size() == 7082;
}
#else
struct RealValidatingWriter
{
    std::size_t length = 0;
    bool valid = true;

    void write(const char* first, const char* last)
    {
        constexpr char pattern[] = "1.25-2.50i";

        for (; first != last; ++first)
        {
            valid = valid && *first == pattern[length % 10];
            ++length;
        }
    }
};

bool workload()
{
    RealValidatingWriter sink;

    for (std::uint32_t index = 0; index < 1024; ++index)
    {
        char bytes[32];
        const auto real = std::to_chars(bytes, bytes + sizeof(bytes), 1.25,
            std::chars_format::fixed, 2);

        if (real.ec != std::errc())
            return false;

        sink.write(bytes, real.ptr);
        sink.write("-", "-" + 1);

        const auto imaginary = std::to_chars(bytes, bytes + sizeof(bytes), 2.5,
            std::chars_format::fixed, 2);

        if (imaginary.ec != std::errc())
            return false;

        sink.write(bytes, imaginary.ptr);
        sink.write("i", "i" + 1);
    }

    return sink.valid && sink.length == 10240;
}
#endif
}
#endif
