#ifndef BRAY_STANDARD_STREAM_STATUS_H
#define BRAY_STANDARD_STREAM_STATUS_H

#include "bray_standard_stream.h"

#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>

inline BrayPlatformStatus bray_standard_stream_failure(DWORD code)
{
    std::uint32_t category = BRAY_PLATFORM_OTHER;

    switch (code)
    {
        case ERROR_NOT_SUPPORTED:
        case ERROR_CALL_NOT_IMPLEMENTED:
            category = BRAY_PLATFORM_UNSUPPORTED;
            break;
        case ERROR_ACCESS_DENIED:
            category = BRAY_PLATFORM_PERMISSION_DENIED;
            break;
        case ERROR_FILE_NOT_FOUND:
        case ERROR_PATH_NOT_FOUND:
            category = BRAY_PLATFORM_NOT_FOUND;
            break;
        case ERROR_INVALID_HANDLE:
        case ERROR_INVALID_PARAMETER:
            category = BRAY_PLATFORM_INVALID_INPUT;
            break;
        case ERROR_OPERATION_ABORTED:
            category = BRAY_PLATFORM_INTERRUPTED;
            break;
        case ERROR_NOT_ENOUGH_MEMORY:
        case ERROR_OUTOFMEMORY:
        case ERROR_DISK_FULL:
        case ERROR_HANDLE_DISK_FULL:
        case ERROR_TOO_MANY_OPEN_FILES:
            category = BRAY_PLATFORM_EXHAUSTED;
            break;
        case ERROR_BROKEN_PIPE:
        case ERROR_NO_DATA:
        case ERROR_PIPE_NOT_CONNECTED:
            category = BRAY_PLATFORM_BROKEN_STREAM;
            break;
        case ERROR_SEM_TIMEOUT:
            category = BRAY_PLATFORM_TIMED_OUT;
            break;
        default:
            break;
    }

    return {category, 0, static_cast<std::int64_t>(code)};
}

#else

#include <cerrno>

inline BrayPlatformStatus bray_standard_stream_failure(int code)
{
    std::uint32_t category = BRAY_PLATFORM_OTHER;

    switch (code)
    {
#if defined(ENOTSUP)
        case ENOTSUP:
            category = BRAY_PLATFORM_UNSUPPORTED;
            break;
#endif
        case EACCES:
        case EPERM:
            category = BRAY_PLATFORM_PERMISSION_DENIED;
            break;
        case ENOENT:
            category = BRAY_PLATFORM_NOT_FOUND;
            break;
        case EINVAL:
            category = BRAY_PLATFORM_INVALID_INPUT;
            break;
        case EINTR:
            category = BRAY_PLATFORM_INTERRUPTED;
            break;
        case ENOMEM:
        case ENOSPC:
        case EMFILE:
        case ENFILE:
            category = BRAY_PLATFORM_EXHAUSTED;
            break;
        case EPIPE:
            category = BRAY_PLATFORM_BROKEN_STREAM;
            break;
#if defined(ETIMEDOUT)
        case ETIMEDOUT:
            category = BRAY_PLATFORM_TIMED_OUT;
            break;
#endif
        default:
            break;
    }

    return {category, 0, code};
}

#endif

#endif
