#include "bray_standard_stream.h"
#include "lock.h"
#include "transfer.h"

namespace
{

BrayStandardStreamLock input_lock = BRAY_STANDARD_STREAM_LOCK_INITIALIZER;

}

extern "C" BrayPlatformStatus bray_platform_standard_input_read(
    std::uint8_t* destination,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
#if defined(_WIN32)
    return bray_read_standard_stream_unlocked(
        STD_INPUT_HANDLE, destination, length, transferred
    );
#else
    return bray_read_standard_stream_unlocked(STDIN_FILENO, destination, length, transferred);
#endif
}

extern "C" BrayPlatformStatus bray_platform_standard_input_lock()
{
    return bray_lock_standard_stream(&input_lock);
}

extern "C" BrayPlatformStatus bray_platform_standard_input_unlock()
{
    return bray_unlock_standard_stream(&input_lock);
}
