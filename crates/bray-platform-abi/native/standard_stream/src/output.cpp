#include "bray_standard_stream.h"
#include "lock.h"
#include "transfer.h"

namespace
{

BrayStandardStreamLock output_lock = BRAY_STANDARD_STREAM_LOCK_INITIALIZER;

}

extern "C" BrayPlatformStatus bray_platform_standard_output_write(
    const std::uint8_t* source,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
#if defined(_WIN32)
    return bray_write_standard_stream(STD_OUTPUT_HANDLE, source, length, transferred);
#else
    return bray_write_standard_stream(STDOUT_FILENO, source, length, transferred);
#endif
}

extern "C" BrayPlatformStatus bray_platform_standard_output_flush()
{
    return {BRAY_PLATFORM_SUCCESS, 0, 0};
}

extern "C" BrayPlatformStatus bray_platform_standard_output_lock()
{
    return bray_lock_standard_stream(&output_lock);
}

extern "C" BrayPlatformStatus bray_platform_standard_output_unlock()
{
    return bray_unlock_standard_stream(&output_lock);
}
