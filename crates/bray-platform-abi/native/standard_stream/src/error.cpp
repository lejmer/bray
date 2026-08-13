#include "bray_standard_stream.h"
#include "lock.h"
#include "transfer.h"

namespace
{

BrayStandardStreamLock error_lock = BRAY_STANDARD_STREAM_LOCK_INITIALIZER;

}

extern "C" BrayPlatformStatus bray_platform_standard_error_write(
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
#if defined(_WIN32)
    return bray_write_standard_stream(STD_ERROR_HANDLE, source, length, transferred);
#else
    return bray_write_standard_stream(STDERR_FILENO, source, length, transferred);
#endif
}

extern "C" BrayPlatformStatus bray_platform_standard_error_flush()
{
    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

extern "C" BrayPlatformStatus bray_platform_standard_error_lock()
{
    return bray_lock_standard_stream(&error_lock);
}

extern "C" BrayPlatformStatus bray_platform_standard_error_unlock()
{
    return bray_unlock_standard_stream(&error_lock);
}
