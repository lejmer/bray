#define NOMINMAX
#include <windows.h>
#include "bray_dynamic.h"

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

int main()
{
    std::vector<wchar_t> directory(32768);
    const auto length = GetSystemDirectoryW(directory.data(), static_cast<UINT>(directory.size()));

    if (length == 0 || length >= directory.size())
        return 1;

    std::wstring path{directory.data(), length};
    path += L"\\ucrtbase.dll";
    const auto byte_count = path.size() * sizeof(wchar_t);
    std::vector<std::uint8_t> bytes(byte_count + 2, 0xff);
    const auto offset = reinterpret_cast<std::uintptr_t>(bytes.data()) % alignof(wchar_t) == 0 ? 1 : 0;
    std::memcpy(bytes.data() + offset, path.data(), byte_count);
    const BrayPlatformPath borrowed{bytes.data() + offset, byte_count, 0};

    if (reinterpret_cast<std::uintptr_t>(borrowed.address) % alignof(wchar_t) == 0)
        return 2;

    std::uint64_t handle = 0;
    auto status = bray_platform_dynamic_library_open_path(borrowed, 0, &handle);

    if (status.category != 0 || handle == 0)
        return 3;

    const std::uint8_t name[] = {'s', 't', 'r', 'l', 'e', 'n'};
    std::uint8_t* symbol = nullptr;
    status = bray_platform_dynamic_library_symbol(handle, name, sizeof(name), &symbol);

    if (status.category != 0 || symbol == nullptr || bray_platform_dynamic_library_close(handle).category != 0)
        return 4;

    auto odd = borrowed;
    --odd.length;
    status = bray_platform_dynamic_library_open_path(odd, 0, &handle);

    if (status.category != 5)
        return 5;

    bytes[offset + 3 * sizeof(wchar_t)] = 0;
    bytes[offset + 3 * sizeof(wchar_t) + 1] = 0;
    status = bray_platform_dynamic_library_open_path(borrowed, 0, &handle);

    if (status.category != 5)
        return 6;

    return 0;
}
