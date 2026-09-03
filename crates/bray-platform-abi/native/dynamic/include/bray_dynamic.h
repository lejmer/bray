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

#include "bray_dynamic_contract.h"

}

#endif
