#include <cstdint>

extern "C"
{

struct BrayPlatformStatus
{
    std::uint32_t category;
    std::uint32_t reserved;
    std::int64_t native_code;
};

BrayPlatformStatus bray_platform_stream_write(
    std::uint64_t handle,
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
);

}

int main()
{
    std::uint64_t transferred = 0;

    const auto status = bray_platform_stream_write(2, nullptr, 0, &transferred);

    return static_cast<int>(status.category);
}
