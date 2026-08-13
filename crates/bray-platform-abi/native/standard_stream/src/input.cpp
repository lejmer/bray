#include "bray_standard_stream.h"
#include "transfer.h"

extern "C" BrayPlatformStatus bray_platform_standard_input_read(
    std::uint8_t* destination,
    std::uint64_t length,
    std::uint64_t* transferred
)
{
#if defined(_WIN32)
    return bray_read_standard_stream(STD_INPUT_HANDLE, destination, length, transferred);
#else
    return bray_read_standard_stream(STDIN_FILENO, destination, length, transferred);
#endif
}
