#ifndef BRAY_STANDARD_STREAM_H
#define BRAY_STANDARD_STREAM_H

#include <cstddef>
#include <cstdint>

inline constexpr std::uint32_t BRAY_PLATFORM_SUCCESS = 0;
inline constexpr std::uint32_t BRAY_PLATFORM_UNSUPPORTED = 1;
inline constexpr std::uint32_t BRAY_PLATFORM_PERMISSION_DENIED = 2;
inline constexpr std::uint32_t BRAY_PLATFORM_NOT_FOUND = 3;
inline constexpr std::uint32_t BRAY_PLATFORM_INVALID_INPUT = 5;
inline constexpr std::uint32_t BRAY_PLATFORM_INTERRUPTED = 6;
inline constexpr std::uint32_t BRAY_PLATFORM_EXHAUSTED = 7;
inline constexpr std::uint32_t BRAY_PLATFORM_BROKEN_STREAM = 8;
inline constexpr std::uint32_t BRAY_PLATFORM_TIMED_OUT = 9;
inline constexpr std::uint32_t BRAY_PLATFORM_OTHER = 12;

extern "C"
{

struct BrayPlatformStatus
{
    std::uint32_t category;
    std::uint32_t reserved;
    std::int64_t native_code;
};

static_assert(sizeof(BrayPlatformStatus) == 16);
static_assert(alignof(BrayPlatformStatus) == 8);
static_assert(offsetof(BrayPlatformStatus, category) == 0);
static_assert(offsetof(BrayPlatformStatus, reserved) == 4);
static_assert(offsetof(BrayPlatformStatus, native_code) == 8);

BrayPlatformStatus bray_platform_standard_input_read(
    std::uint8_t* destination,
    std::uint64_t length,
    std::uint64_t* transferred
);

BrayPlatformStatus bray_platform_standard_input_lock();
BrayPlatformStatus bray_platform_standard_input_unlock();

BrayPlatformStatus bray_platform_standard_output_write(
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
);

BrayPlatformStatus bray_platform_standard_output_flush();
BrayPlatformStatus bray_platform_standard_output_lock();
BrayPlatformStatus bray_platform_standard_output_unlock();

BrayPlatformStatus bray_platform_standard_error_write(
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
);

BrayPlatformStatus bray_platform_standard_error_flush();
BrayPlatformStatus bray_platform_standard_error_lock();
BrayPlatformStatus bray_platform_standard_error_unlock();

}

#endif
