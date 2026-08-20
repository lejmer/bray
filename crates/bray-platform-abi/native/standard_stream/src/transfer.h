#ifndef BRAY_STANDARD_STREAM_TRANSFER_H
#define BRAY_STANDARD_STREAM_TRANSFER_H

#include "bray_standard_stream.h"
#include "status.h"

#include <cstddef>
#include <limits>

#if !defined(_WIN32)
#include <unistd.h>
#endif

inline bool bray_validate_standard_stream_transfer(
    const void* bytes,
    std::uint64_t length,
    const std::uint64_t* transferred
)
{
    if (
        transferred == nullptr
        || reinterpret_cast<std::uintptr_t>(transferred) % alignof(std::uint64_t) != 0
        || (bytes == nullptr && length != 0)
        || length > std::numeric_limits<std::size_t>::max()
    )
        return false;

    const auto bytes_start = reinterpret_cast<std::uintptr_t>(bytes);
    const auto transferred_start = reinterpret_cast<std::uintptr_t>(transferred);

    if (
        length > std::numeric_limits<std::uintptr_t>::max() - bytes_start
        || sizeof(std::uint64_t) > std::numeric_limits<std::uintptr_t>::max() - transferred_start
    )
        return false;

    const auto bytes_end = bytes_start + static_cast<std::uintptr_t>(length);
    const auto transferred_end = transferred_start + sizeof(std::uint64_t);

    return bytes_end <= transferred_start || transferred_end <= bytes_start;
}

inline BrayPlatformStatus bray_read_standard_stream_unlocked(
#if defined(_WIN32)
    DWORD stream,
#else
    int stream,
#endif
    std::uint8_t* destination,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
    if (!bray_validate_standard_stream_transfer(destination, length, transferred))
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    *transferred = 0;

    if (length == 0)
        return {BRAY_PLATFORM_SUCCESS, 0, 0};

#if defined(_WIN32)
    DWORD count = 0;
    const DWORD requested = length > MAXDWORD ? MAXDWORD : static_cast<DWORD>(length);

    if (!ReadFile(GetStdHandle(stream), destination, requested, &count, nullptr))
        return bray_standard_stream_failure(GetLastError());

    *transferred = count;
#else
    const auto maximum = static_cast<std::uint64_t>(std::numeric_limits<ssize_t>::max());
    const auto requested = static_cast<std::size_t>(length > maximum ? maximum : length);
    const auto count = ::read(stream, destination, requested);

    if (count < 0)
        return bray_standard_stream_failure(errno);

    *transferred = static_cast<std::uint64_t>(count);
#endif

    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

inline BrayPlatformStatus bray_write_standard_stream_unlocked(
#if defined(_WIN32)
    DWORD stream,
#else
    int stream,
#endif
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
    if (!bray_validate_standard_stream_transfer(source, length, transferred))
        return {BRAY_PLATFORM_INVALID_INPUT, 0, 0};

    *transferred = 0;

    if (length == 0)
        return {BRAY_PLATFORM_SUCCESS, 0, 0};

#if defined(_WIN32)
    DWORD count = 0;
    const DWORD requested = length > MAXDWORD ? MAXDWORD : static_cast<DWORD>(length);

    if (!WriteFile(GetStdHandle(stream), source, requested, &count, nullptr))
        return bray_standard_stream_failure(GetLastError());

    *transferred = count;
#else
    const auto maximum = static_cast<std::uint64_t>(std::numeric_limits<ssize_t>::max());
    const auto requested = static_cast<std::size_t>(length > maximum ? maximum : length);
    const auto count = ::write(stream, source, requested);

    if (count < 0)
        return bray_standard_stream_failure(errno);

    *transferred = static_cast<std::uint64_t>(count);
#endif

    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

#endif
