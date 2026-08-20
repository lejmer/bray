#include "bray_standard_stream.h"

int main()
{
    if (bray_platform_standard_input_unlock().category != BRAY_PLATFORM_INVALID_INPUT)
        return 1;

    if (bray_platform_standard_input_lock().category != BRAY_PLATFORM_SUCCESS)
        return 2;

    if (bray_platform_standard_input_lock().category != BRAY_PLATFORM_INVALID_INPUT)
        return 3;

    if (bray_platform_standard_input_unlock().category != BRAY_PLATFORM_SUCCESS)
        return 4;

    if (bray_platform_standard_input_lock().category != BRAY_PLATFORM_SUCCESS)
        return 5;

    return bray_platform_standard_input_unlock().category == BRAY_PLATFORM_SUCCESS ? 0 : 6;
}
