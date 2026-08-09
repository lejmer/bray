#ifndef BRAY_DYNAMIC_H
#define BRAY_DYNAMIC_H

#include <cstdint>

extern "C"
{

struct BrayPlatformPath
{
    const std::uint8_t* address;
    std::uint64_t length;
};

struct BrayPlatformStatus
{
    std::uint32_t category;
    std::uint32_t reserved;
    std::int64_t native_code;
};

BrayPlatformStatus bray_platform_dynamic_library_open_path(
    BrayPlatformPath path,
    std::uint32_t policy,
    std::uint64_t* opened
);

BrayPlatformStatus bray_platform_dynamic_library_open_system(
    std::uint32_t identity,
    std::uint32_t policy,
    std::uint64_t* opened
);

BrayPlatformStatus bray_platform_dynamic_library_symbol(
    std::uint64_t handle,
    const std::uint8_t* name,
    std::uint64_t length,
    std::uint8_t** address
);

BrayPlatformStatus bray_platform_dynamic_library_close(std::uint64_t handle);

}

#endif
